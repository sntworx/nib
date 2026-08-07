use std::rc::Rc;

use crate::ast::Ast;
use crate::ast::types::{
    AstNode, AstNodeKind, BinaryOp, Expr, ForStmt, FuncDecl, IfStmt, Literal, MatchStmt, UnaryOp,
    VarAssign, WhileStmt,
};
use crate::runtime::environment::Environment;
use crate::runtime::helpers::{as_f64, checked_float, values_equal};
use crate::runtime::types::{Function, NativeFunction, RuntimeError, Value};

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

const MAX_CALL_DEPTH: usize = 1000;

pub struct Interpreter {
    env: Environment,
    call_depth: usize,
    current_pos: (usize, usize),
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            env: Environment::new(),
            call_depth: 0,
            current_pos: (0, 0),
        }
    }

    fn error(&self, message: String) -> RuntimeError {
        RuntimeError {
            message,
            line: self.current_pos.0,
            col: self.current_pos.1,
        }
    }

    // Binds a Rust function into the global scope under `name`, callable from
    // Nib scripts like any other function. It's just another Value, so it
    // composes with the rest of the interpreter for free (can be passed
    // around, shadowed, etc.) - the only new code is dispatching to it in
    // `eval_call` below.
    pub fn register_native(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) {
        let name = name.into();
        let native = NativeFunction {
            name: name.clone(),
            func: Box::new(f),
        };
        self.env
            .define(name, Value::NativeFunction(Rc::new(native)));
    }

    pub fn run(&mut self, ast: &Ast) -> Result<(), RuntimeError> {
        match self.exec_all(ast.nodes())? {
            Flow::Normal => Ok(()),
            Flow::Return(_) => Err(self.error("'return' outside of function".to_string())),
            Flow::Break | Flow::Continue => {
                unreachable!("parser guarantees break/continue only appear inside loops")
            }
        }
    }

    fn exec(&mut self, node: &AstNode) -> Result<Flow, RuntimeError> {
        self.current_pos = (node.line, node.col);
        match &node.kind {
            AstNodeKind::VarAssign(VarAssign { name, value }) => {
                let value = self.eval(value)?;
                self.env.define(name.clone(), value);
                Ok(Flow::Normal)
            }
            AstNodeKind::ExprStmt(expr) => {
                self.eval(expr)?;
                Ok(Flow::Normal)
            }
            AstNodeKind::If(if_stmt) => self.exec_if(if_stmt),
            AstNodeKind::Block(nodes) => self.exec_block(nodes),
            AstNodeKind::FuncDecl(FuncDecl { name, params, body }) => {
                let function = Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                };
                self.env
                    .define(name.clone(), Value::Function(Rc::new(function)));
                Ok(Flow::Normal)
            }
            AstNodeKind::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr)?,
                    None => Value::Null,
                };
                Ok(Flow::Return(value))
            }
            AstNodeKind::While(while_stmt) => self.exec_while(while_stmt),
            AstNodeKind::For(for_stmt) => self.exec_for(for_stmt),
            AstNodeKind::Match(match_stmt) => self.exec_match(match_stmt),
            AstNodeKind::Break => Ok(Flow::Break),
            AstNodeKind::Continue => Ok(Flow::Continue),
        }
    }

    fn exec_if(&mut self, if_stmt: &IfStmt) -> Result<Flow, RuntimeError> {
        match self.eval(&if_stmt.condition)? {
            Value::Bool(true) => self.exec_block(&if_stmt.then_branch),
            Value::Bool(false) => match &if_stmt.else_branch {
                Some(else_branch) => self.exec_block(else_branch),
                None => Ok(Flow::Normal),
            },
            other => Err(self.error(format!("if condition must be a bool, got {}", other))),
        }
    }

    // Arms are tested top-to-bottom using the same equality semantics as
    // `==`/`!=` (`values_equal` - numeric Int/Float coercion, no cross-type
    // coercion otherwise). First match wins, no fallthrough between arms.
    fn exec_match(&mut self, match_stmt: &MatchStmt) -> Result<Flow, RuntimeError> {
        let subject = self.eval(&match_stmt.subject)?;
        for arm in &match_stmt.arms {
            let pattern = self.eval(&arm.pattern)?;
            if values_equal(&subject, &pattern) {
                return self.exec_block(&arm.body);
            }
        }
        match &match_stmt.else_branch {
            Some(else_branch) => self.exec_block(else_branch),
            None => Ok(Flow::Normal),
        }
    }

    fn exec_while(&mut self, while_stmt: &WhileStmt) -> Result<Flow, RuntimeError> {
        loop {
            match self.eval(&while_stmt.condition)? {
                Value::Bool(true) => {}
                Value::Bool(false) => return Ok(Flow::Normal),
                other => {
                    return Err(
                        self.error(format!("while condition must be a bool, got {}", other))
                    );
                }
            }
            match self.exec_block(&while_stmt.body)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }
        }
    }

    // `init` lives in its own scope for the loop's whole lifetime (not
    // per-iteration, unlike `body`), so it's visible to condition/post/body
    // but doesn't leak into the surrounding scope.
    fn exec_for(&mut self, for_stmt: &ForStmt) -> Result<Flow, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_for_body(for_stmt);
        self.env.pop_scope();
        result
    }

    fn exec_for_body(&mut self, for_stmt: &ForStmt) -> Result<Flow, RuntimeError> {
        if let Some(init) = &for_stmt.init {
            self.exec(init)?;
        }
        loop {
            let should_continue = match &for_stmt.condition {
                Some(condition) => match self.eval(condition)? {
                    Value::Bool(b) => b,
                    other => {
                        return Err(
                            self.error(format!("for condition must be a bool, got {}", other))
                        );
                    }
                },
                None => true,
            };
            if !should_continue {
                return Ok(Flow::Normal);
            }

            match self.exec_block(&for_stmt.body)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }

            if let Some(post) = &for_stmt.post {
                self.eval(post)?;
            }
        }
    }

    fn exec_block(&mut self, nodes: &[AstNode]) -> Result<Flow, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_all(nodes);
        self.env.pop_scope();
        result
    }

    // Runs statements in order, stopping early (without running the rest) as
    // soon as one of them returns/breaks/continues.
    fn exec_all(&mut self, nodes: &[AstNode]) -> Result<Flow, RuntimeError> {
        for node in nodes {
            match self.exec(node)? {
                Flow::Normal => {}
                flow => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Literal(lit) => Ok(match lit {
                Literal::Int(v) => Value::Int(*v),
                Literal::Float(v) => Value::Float(*v),
                Literal::Str(v) => Value::Str(v.clone()),
                Literal::Bool(v) => Value::Bool(*v),
                Literal::Null => Value::Null,
            }),
            Expr::Ident(name) => self
                .env
                .get(name)
                .cloned()
                .ok_or_else(|| self.error(format!("undefined variable '{}'", name))),
            Expr::Unary { op, expr } => self.eval_unary(op, expr),
            Expr::Binary { op, left, right } => self.eval_binary(op, left, right),
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
                Ok(Value::Array(values))
            }
            Expr::Grouping(inner) => self.eval(inner),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, expr: &Expr) -> Result<Value, RuntimeError> {
        let value = self.eval(expr)?;
        match (op, &value) {
            (UnaryOp::Neg, Value::Int(v)) => Ok(Value::Int(-v)),
            (UnaryOp::Neg, Value::Float(v)) => Ok(Value::Float(-v)),
            (UnaryOp::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
            _ => Err(self.error(format!("unary operator cannot be applied to {}", value))),
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
            let left_bool = match self.eval(left)? {
                Value::Bool(b) => b,
                other => return Err(self.error(format!("expected bool operand, got {}", other))),
            };
            return match (op, left_bool) {
                (BinaryOp::And, false) => Ok(Value::Bool(false)),
                (BinaryOp::Or, true) => Ok(Value::Bool(true)),
                _ => match self.eval(right)? {
                    Value::Bool(b) => Ok(Value::Bool(b)),
                    other => Err(self.error(format!("expected bool operand, got {}", other))),
                },
            };
        }

        let left_val = self.eval(left)?;
        let right_val = self.eval(right)?;
        self.apply_binary_op(op, left_val, right_val)
    }

    // The value-level half of eval_binary, split out so compound index
    // assignment (`arr[i] += value`) can reuse it without re-evaluating
    // `left`/`right` as expressions - it already has both sides as Values.
    fn apply_binary_op(
        &self,
        op: &BinaryOp,
        left_val: Value,
        right_val: Value,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;
        match (op, left_val, right_val) {
            (Eq, a, b) => Ok(Value::Bool(values_equal(&a, &b))),
            (NotEq, a, b) => Ok(Value::Bool(!values_equal(&a, &b))),
            (Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
            (Add, Value::Str(a), Value::Int(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Add, Value::Str(a), Value::Float(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Add, Value::Int(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Add, Value::Float(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
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

            (_, a, b) => Err(self.error(format!("operator cannot be applied to {} and {}", a, b))),
        }
    }

    fn eval_index(&mut self, object: &Expr, index: &Expr) -> Result<Value, RuntimeError> {
        let object_val = self.eval(object)?;
        let index_val = self.eval(index)?;

        let items = match object_val {
            Value::Array(items) => items,
            other => return Err(self.error(format!("cannot index into {}", other))),
        };

        let idx = match index_val {
            Value::Int(i) => i,
            other => {
                return Err(self.error(format!("array index must be an integer, got {}", other)));
            }
        };

        if idx < 0 || idx as usize >= items.len() {
            return Err(self.error(format!(
                "index {} out of bounds for array of length {}",
                idx,
                items.len()
            )));
        }

        Ok(items[idx as usize].clone())
    }

    fn eval_index_value(&mut self, index: &Expr) -> Result<i64, RuntimeError> {
        match self.eval(index)? {
            Value::Int(i) => Ok(i),
            other => Err(self.error(format!("array index must be an integer, got {}", other))),
        }
    }

    // Arrays are a value type here, like every other Value (cloned on
    // read/assign) - there's no shared mutable storage to reach into. So
    // "mutating" one means: read a copy of the whole array, splice in the new
    // element, and hand the patched copy back to the caller to write
    // wherever `object` actually lives (a variable, or another level of
    // array nesting).
    fn with_index_replaced(
        &mut self,
        object: &Expr,
        idx: i64,
        new_elem: Value,
    ) -> Result<Value, RuntimeError> {
        let mut array = self.eval(object)?;
        match &mut array {
            Value::Array(items) => {
                if idx < 0 || idx as usize >= items.len() {
                    return Err(self.error(format!(
                        "index {} out of bounds for array of length {}",
                        idx,
                        items.len()
                    )));
                }
                items[idx as usize] = new_elem;
                Ok(array)
            }
            other => Err(self.error(format!("cannot index into {}", other))),
        }
    }

    // Writes `value` to an lvalue: a bare variable, or (recursively) an
    // index into an array reached through one, e.g. `matrix[0][1] = x`. Each
    // level patches its own copy of its array and hands it up to the next.
    //
    // Note: this re-evaluates `object`/`index` at each nesting level beyond
    // the first (they were already evaluated once by the caller to read the
    // current value). That's only observable if those sub-expressions have
    // side effects (e.g. `matrix[i()][j()] = x` calling i()/j() twice) -
    // accepted as a known limitation rather than adding a full lvalue-path
    // pre-evaluation pass for what should be a rare case.
    fn assign_to_target(&mut self, target: &Expr, value: Value) -> Result<(), RuntimeError> {
        match target {
            Expr::Ident(name) => self.env.assign(name, value).map_err(|msg| self.error(msg)),
            Expr::Index { object, index } => {
                let idx = self.eval_index_value(index)?;
                let patched = self.with_index_replaced(object, idx, value)?;
                self.assign_to_target(object, patched)
            }
            _ => Err(self.error("invalid assignment target".to_string())),
        }
    }

    fn eval_index_assign(
        &mut self,
        object: &Expr,
        index: &Expr,
        op: Option<&BinaryOp>,
        value: &Expr,
    ) -> Result<Value, RuntimeError> {
        let idx = self.eval_index_value(index)?;
        let current_array = self.eval(object)?;

        let items = match &current_array {
            Value::Array(items) => items,
            other => return Err(self.error(format!("cannot index into {}", other))),
        };
        if idx < 0 || idx as usize >= items.len() {
            return Err(self.error(format!(
                "index {} out of bounds for array of length {}",
                idx,
                items.len()
            )));
        }
        let idx_usize = idx as usize;
        let current_elem = items[idx_usize].clone();

        let rhs = self.eval(value)?;
        let new_elem = match op {
            Some(op) => self.apply_binary_op(op, current_elem, rhs)?,
            None => rhs,
        };

        let mut patched = current_array;
        if let Value::Array(items) = &mut patched {
            items[idx_usize] = new_elem.clone();
        }

        self.assign_to_target(object, patched)?;
        Ok(new_elem)
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value, RuntimeError> {
        let callee_val = match callee {
            Expr::Ident(name) => match self.env.get(name).cloned() {
                Some(value) => value,
                None => return Err(self.error(format!("undefined function '{}'", name))),
            },
            other => self.eval(other)?,
        };

        match callee_val {
            Value::Function(function) => {
                if args.len() != function.params.len() {
                    return Err(self.error(format!(
                        "function '{}' expects {} argument(s), got {}",
                        function.name,
                        function.params.len(),
                        args.len()
                    )));
                }
                let arg_values = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call_function(&function, arg_values)
            }
            Value::NativeFunction(native) => {
                let arg_values = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                (native.func)(&arg_values).map_err(|msg| self.error(msg))
            }
            other => Err(self.error(format!("cannot call {}", other))),
        }
    }

    // Function bodies only see the global scope plus their own parameters and
    // locals - not the caller's locals - so a call temporarily strips those
    // away and restores them once the call returns.
    fn call_function(
        &mut self,
        function: &Function,
        arg_values: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(self.error(format!(
                "stack overflow: exceeded maximum call depth of {}",
                MAX_CALL_DEPTH
            )));
        }
        self.call_depth += 1;

        let saved_pos = self.current_pos;
        let saved_locals = self.env.enter_call();
        self.env.push_scope();
        for (param, value) in function.params.iter().zip(arg_values) {
            self.env.define(param.clone(), value);
        }
        let flow = self.exec_all(&function.body);
        self.env.pop_scope();
        self.env.exit_call(saved_locals);
        self.current_pos = saved_pos;
        self.call_depth -= 1;

        match flow? {
            Flow::Return(value) => Ok(value),
            Flow::Normal => Ok(Value::Null),
            Flow::Break | Flow::Continue => {
                unreachable!("parser guarantees break/continue only appear inside loops")
            }
        }
    }
}
