// Expression evaluation: the `eval` dispatch plus the unary/binary operators.

use crate::ast::types::{BinaryOp, Expr, Literal, UnaryOp};
use crate::runtime::helpers::{as_f64, checked_float, is_truthy, values_equal};
use crate::runtime::types::{RuntimeError, Value};

use super::Interpreter;

impl Interpreter {
    pub(super) fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Literal(lit) => {
                let value = match lit {
                    Literal::Int(v) => Value::Int(*v),
                    Literal::Float(v) => Value::Float(*v),
                    Literal::Str(v) => Value::Str(v.clone()),
                    Literal::Bool(v) => Value::Bool(*v),
                    Literal::Null => Value::Null,
                };
                self.check_size_limits(&value)?;
                Ok(value)
            }
            Expr::Ident(name) => self
                .env
                .get(name)
                .cloned()
                .ok_or_else(|| self.error(format!("undefined variable '{}'", name))),
            Expr::Unary { op, expr } => self.eval_unary(op, expr),
            Expr::Binary { op, left, right } => self.eval_binary(op, left, right),
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let branch = if is_truthy(&self.eval(cond)?) {
                    then_expr
                } else {
                    else_expr
                };
                self.eval(branch)
            }
            Expr::Assign { name, value } => {
                let value = self.eval(value)?;
                self.env
                    .assign(name, value.clone())
                    .map_err(|msg| self.error(msg))?;
                Ok(value)
            }
            Expr::Call { callee, args } => self.eval_call(callee, args),
            Expr::Index { object, index } => self.eval_index(object, index),
            Expr::IndexAssign {
                object,
                index,
                op,
                value,
            } => self.eval_index_assign(object, index, op.as_ref(), value),
            Expr::Array(elements) => {
                let values = elements
                    .iter()
                    .map(|e| self.eval(e))
                    .collect::<Result<Vec<_>, _>>()?;
                let value = Value::array(values);
                self.check_size_limits(&value)?;
                Ok(value)
            }
            Expr::Map(pairs) => {
                let mut entries: Vec<(String, Value)> = Vec::with_capacity(pairs.len());
                for (key, expr) in pairs {
                    let value = self.eval(expr)?;
                    // duplicate literal keys are collapsed by MapData::new
                    entries.push((key.clone(), value));
                }
                let value = Value::map(entries);
                self.check_size_limits(&value)?;
                Ok(value)
            }
            Expr::Grouping(inner) => self.eval(inner),
            Expr::MethodCall {
                target,
                method,
                args,
            } => self.eval_method_call(target, method, args),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, expr: &Expr) -> Result<Value, RuntimeError> {
        let value = self.eval(expr)?;
        match (op, &value) {
            // checked, like every other integer op - negating i64::MIN
            // overflows, which would panic in debug and wrap in release
            (UnaryOp::Neg, Value::Int(v)) => v
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| self.error("integer overflow".to_string())),
            (UnaryOp::Neg, Value::Float(v)) => Ok(Value::Float(-v)),
            (UnaryOp::Not, _) => Ok(Value::Bool(!is_truthy(&value))),
            _ => Err(self.error(format!(
                "unary operator cannot be applied to {}",
                value.type_name()
            ))),
        }
    }

    fn eval_binary(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<Value, RuntimeError> {
        // logical operators short-circuit, so evaluate the right side lazily
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let left_bool = is_truthy(&self.eval(left)?);
            // Operands are coerced by truthiness, but the result is always a
            // real Bool - `"" || "x"` is `true`, not `"x"` as it'd be in JS.
            return match (op, left_bool) {
                (BinaryOp::And, false) => Ok(Value::Bool(false)),
                (BinaryOp::Or, true) => Ok(Value::Bool(true)),
                _ => Ok(Value::Bool(is_truthy(&self.eval(right)?))),
            };
        }

        let left_val = self.eval(left)?;
        let right_val = self.eval(right)?;
        self.apply_binary_op(op, left_val, right_val)
    }

    // The value-level half of eval_binary, split out so compound index
    // assignment (`arr[i] += value`) can reuse it without re-evaluating
    // `left`/`right` as expressions - it already has both sides as Values.
    pub(super) fn apply_binary_op(
        &self,
        op: &BinaryOp,
        left_val: Value,
        right_val: Value,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;
        match (op, left_val, right_val) {
            (Eq, a, b) => Ok(Value::Bool(values_equal(&a, &b))),
            (NotEq, a, b) => Ok(Value::Bool(!values_equal(&a, &b))),
            (Add, Value::Str(a), Value::Str(b)) => {
                let result = Value::Str(a + &b);
                self.check_size_limits(&result)?;
                Ok(result)
            }
            (Add, Value::Str(a), Value::Int(b)) => {
                let result = Value::Str(format!("{}{}", a, b));
                self.check_size_limits(&result)?;
                Ok(result)
            }
            (Add, Value::Str(a), Value::Float(b)) => {
                let result = Value::Str(format!("{}{}", a, b));
                self.check_size_limits(&result)?;
                Ok(result)
            }
            (Add, Value::Int(a), Value::Str(b)) => {
                let result = Value::Str(format!("{}{}", a, b));
                self.check_size_limits(&result)?;
                Ok(result)
            }
            (Add, Value::Float(a), Value::Str(b)) => {
                let result = Value::Str(format!("{}{}", a, b));
                self.check_size_limits(&result)?;
                Ok(result)
            }
            (Add, Value::Int(a), Value::Int(b)) => a
                .checked_add(b)
                .map(Value::Int)
                .ok_or_else(|| self.error("integer overflow".to_string())),
            (Sub, Value::Int(a), Value::Int(b)) => a
                .checked_sub(b)
                .map(Value::Int)
                .ok_or_else(|| self.error("integer overflow".to_string())),
            (Mul, Value::Int(a), Value::Int(b)) => a
                .checked_mul(b)
                .map(Value::Int)
                .ok_or_else(|| self.error("integer overflow".to_string())),
            (Div, Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    Err(self.error("division by zero".to_string()))
                } else {
                    a.checked_div(b)
                        .map(Value::Int)
                        .ok_or_else(|| self.error("integer overflow".to_string()))
                }
            }
            (Mod, Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    Err(self.error("modulo by zero".to_string()))
                } else {
                    a.checked_rem(b)
                        .map(Value::Int)
                        .ok_or_else(|| self.error("integer overflow".to_string()))
                }
            }
            (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (LtEq, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (GtEq, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (Lt, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a < b)),
            (LtEq, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a <= b)),
            (Gt, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a > b)),
            (GtEq, Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a >= b)),

            // any other numeric pairing (float/float, or a mix of int/float) promotes to float
            (op, a, b) if as_f64(&a).is_some() && as_f64(&b).is_some() => {
                let (a, b) = (as_f64(&a).unwrap(), as_f64(&b).unwrap());
                let result = match op {
                    Add => checked_float(a + b),
                    Sub => checked_float(a - b),
                    Mul => checked_float(a * b),
                    Div if b == 0.0 => return Err(self.error("division by zero".to_string())),
                    Div => checked_float(a / b),
                    Mod if b == 0.0 => return Err(self.error("modulo by zero".to_string())),
                    Mod => checked_float(a % b),
                    Lt => return Ok(Value::Bool(a < b)),
                    LtEq => return Ok(Value::Bool(a <= b)),
                    Gt => return Ok(Value::Bool(a > b)),
                    GtEq => return Ok(Value::Bool(a >= b)),
                    Eq | NotEq | And | Or => unreachable!("handled earlier"),
                };
                result.map_err(|msg| self.error(msg))
            }

            (_, a, b) => Err(self.error(format!(
                "operator cannot be applied to {} and {}",
                a.type_name(),
                b.type_name()
            ))),
        }
    }
}
