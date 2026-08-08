use std::rc::Rc;

use crate::ast::Ast;
use crate::ast::types::{
    AstNode, AstNodeKind, BinaryOp, Expr, ForInStmt, ForStmt, FuncDecl, IfStmt, Literal, MatchStmt,
    TryStmt, UnaryOp, VarAssign, WhileStmt,
};
use crate::runtime::environment::Environment;
use crate::runtime::helpers::{as_f64, checked_float, values_equal};
use crate::runtime::types::{Function, MethodResult, NativeFunction, RuntimeError, Value};
use crate::types::Config;

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

pub struct Interpreter {
    env: Environment,
    call_depth: usize,
    max_call_depth: usize,
    step_count: usize,
    max_steps: usize,
    max_string_length: usize,
    max_array_length: usize,
    max_map_size: usize,
    current_pos: (usize, usize),
    should_exit: bool,
}

impl Interpreter {
    pub fn new(config: &Config) -> Self {
        Interpreter {
            env: Environment::new(),
            call_depth: 0,
            max_call_depth: config.max_call_depth,
            step_count: 0,
            max_steps: config.max_steps,
            max_string_length: config.max_string_length,
            max_array_length: config.max_array_length,
            max_map_size: config.max_map_size,
            current_pos: (0, 0),
            should_exit: false,
        }
    }

    fn error(&self, message: String) -> RuntimeError {
        RuntimeError {
            message,
            line: self.current_pos.0,
            col: self.current_pos.1,
            value: None,
        }
    }

    // `throw expr;` - unlike `error()`, carries the actual thrown Value
    // through to a `catch`, not just its stringified message.
    fn throw_error(&self, value: Value) -> RuntimeError {
        RuntimeError {
            message: value.to_string(),
            line: self.current_pos.0,
            col: self.current_pos.1,
            value: Some(value),
        }
    }

    // Called once per statement executed and once per loop iteration (see
    // exec_while/exec_for_body/exec_for_in), not once per exec() alone - a
    // non-empty loop body ticks twice per iteration (once for the loop
    // construct, once per body statement). Steps are a "units of work done"
    // budget, not a precise iteration count.
    fn tick(&mut self) -> Result<(), RuntimeError> {
        if self.step_count >= self.max_steps {
            return Err(self.error(format!(
                "exceeded maximum execution steps of {}",
                self.max_steps
            )));
        }
        self.step_count += 1;
        Ok(())
    }

    // Checked at every point a Str/Array/Map is constructed or grown (see
    // call sites) - a &self, not &mut self, check so it's callable from
    // apply_binary_op too.
    fn check_size_limits(&self, value: &Value) -> Result<(), RuntimeError> {
        match value {
            Value::Str(s) if s.chars().count() > self.max_string_length => {
                Err(self.error(format!(
                    "string exceeds maximum length of {} characters",
                    self.max_string_length
                )))
            }
            Value::Array(items) if items.len() > self.max_array_length => Err(self.error(format!(
                "array exceeds maximum length of {} elements",
                self.max_array_length
            ))),
            Value::Map(pairs) if pairs.len() > self.max_map_size => Err(self.error(format!(
                "map exceeds maximum size of {} entries",
                self.max_map_size
            ))),
            _ => Ok(()),
        }
    }

    // Just another Value bound in the global scope, so it composes for free
    // (can be shadowed, passed around, etc.).
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
        self.step_count = 0;
        self.should_exit = false;
        match self.exec_all(ast.nodes())? {
            Flow::Normal => Ok(()),
            Flow::Return(_) => Err(self.error("'return' outside of function".to_string())),
            Flow::Break | Flow::Continue => {
                unreachable!("parser guarantees break/continue only appear inside loops")
            }
        }
    }

    fn exec(&mut self, node: &AstNode) -> Result<Flow, RuntimeError> {
        if self.should_exit {
            return Ok(Flow::Normal);
        }
        self.current_pos = (node.line, node.col);
        self.tick()?;
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
            AstNodeKind::ForIn(for_in_stmt) => self.exec_for_in(for_in_stmt),
            AstNodeKind::Match(match_stmt) => self.exec_match(match_stmt),
            AstNodeKind::Break => Ok(Flow::Break),
            AstNodeKind::Continue => Ok(Flow::Continue),
            AstNodeKind::Try(try_stmt) => self.exec_try(try_stmt),
            AstNodeKind::Throw(expr) => {
                let value = self.eval(expr)?;
                Err(self.throw_error(value))
            }
            AstNodeKind::Exit => {
                self.should_exit = true;
                Ok(Flow::Normal)
            }
        }
    }

    fn exec_if(&mut self, if_stmt: &IfStmt) -> Result<Flow, RuntimeError> {
        match self.eval(&if_stmt.condition)? {
            Value::Bool(true) => self.exec_block(&if_stmt.then_branch),
            Value::Bool(false) => match &if_stmt.else_branch {
                Some(else_branch) => self.exec_block(else_branch),
                None => Ok(Flow::Normal),
            },
            other => Err(self.error(format!(
                "if condition must be a bool, got {}",
                other.type_name()
            ))),
        }
    }

    // Arms tested top-to-bottom with `==`'s equality; first match wins, no fallthrough.
    fn exec_match(&mut self, match_stmt: &MatchStmt) -> Result<Flow, RuntimeError> {
        let subject = self.eval(&match_stmt.subject)?;
        for arm in &match_stmt.arms {
            let pattern = self.eval(&arm.pattern)?;
            if values_equal(&subject, &pattern) {
                return self.exec_block(&arm.body);
            }
        }
        match &match_stmt.default_branch {
            Some(default_branch) => self.exec_block(default_branch),
            None => Ok(Flow::Normal),
        }
    }

    // Only `try_block`'s own execution is guarded - an error raised inside
    // `catch_block` propagates normally rather than being caught by its own
    // try. Every RuntimeError is catchable, including step/call-depth/size
    // limit errors: those counters are already restored to their pre-call
    // state by the time the error reaches here (call_function decrements
    // call_depth before propagating, tick()'s step_count never rewinds
    // either way), so catching one can't be used to bypass the budget it
    // guards.
    fn exec_try(&mut self, try_stmt: &TryStmt) -> Result<Flow, RuntimeError> {
        match self.exec_block(&try_stmt.try_block) {
            Ok(flow) => Ok(flow),
            Err(err) => {
                let caught = err.value.unwrap_or(Value::Str(err.message));
                self.env.push_scope();
                self.env.define(try_stmt.catch_var.clone(), caught);
                let flow = self.exec_all(&try_stmt.catch_block);
                self.env.pop_scope();
                flow
            }
        }
    }

    fn exec_while(&mut self, while_stmt: &WhileStmt) -> Result<Flow, RuntimeError> {
        loop {
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            match self.eval(&while_stmt.condition)? {
                Value::Bool(true) => {}
                Value::Bool(false) => return Ok(Flow::Normal),
                other => {
                    return Err(self.error(format!(
                        "while condition must be a bool, got {}",
                        other.type_name()
                    )));
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
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            let should_continue = match &for_stmt.condition {
                Some(condition) => match self.eval(condition)? {
                    Value::Bool(b) => b,
                    other => {
                        return Err(self.error(format!(
                            "for condition must be a bool, got {}",
                            other.type_name()
                        )));
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

    // `iterable` is evaluated once up front, so reassigning it mid-loop
    // doesn't change what's iterated; each element is handed to the body by
    // value, so mutating the loop variable never writes back into the array.
    fn exec_for_in(&mut self, for_in_stmt: &ForInStmt) -> Result<Flow, RuntimeError> {
        let iterable = self.eval(&for_in_stmt.iterable)?;
        let items = match iterable {
            Value::Array(items) => items,
            other => {
                return Err(self.error(format!("cannot iterate over {}", other.type_name())));
            }
        };

        for item in items {
            if self.should_exit {
                return Ok(Flow::Normal);
            }
            self.tick()?;
            self.env.push_scope();
            self.env.define(for_in_stmt.var_name.clone(), item);
            let flow = self.exec_all(&for_in_stmt.body);
            self.env.pop_scope();
            match flow? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                flow @ Flow::Return(_) => return Ok(flow),
            }
        }
        Ok(Flow::Normal)
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
                let value = Value::Array(values);
                self.check_size_limits(&value)?;
                Ok(value)
            }
            Expr::Map(pairs) => {
                let mut entries: Vec<(String, Value)> = Vec::with_capacity(pairs.len());
                for (key, expr) in pairs {
                    let value = self.eval(expr)?;
                    match entries.iter_mut().find(|(k, _)| k == key) {
                        Some((_, v)) => *v = value,
                        None => entries.push((key.clone(), value)),
                    }
                }
                let value = Value::Map(entries);
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
            (UnaryOp::Neg, Value::Int(v)) => Ok(Value::Int(-v)),
            (UnaryOp::Neg, Value::Float(v)) => Ok(Value::Float(-v)),
            (UnaryOp::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
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
            let left_bool = match self.eval(left)? {
                Value::Bool(b) => b,
                other => {
                    return Err(
                        self.error(format!("expected bool operand, got {}", other.type_name()))
                    );
                }
            };
            return match (op, left_bool) {
                (BinaryOp::And, false) => Ok(Value::Bool(false)),
                (BinaryOp::Or, true) => Ok(Value::Bool(true)),
                _ => match self.eval(right)? {
                    Value::Bool(b) => Ok(Value::Bool(b)),
                    other => {
                        Err(self.error(format!("expected bool operand, got {}", other.type_name())))
                    }
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

    fn eval_index(&mut self, object: &Expr, index: &Expr) -> Result<Value, RuntimeError> {
        let object_val = self.eval(object)?;
        let index_val = self.eval(index)?;

        match (object_val, index_val) {
            (Value::Array(items), Value::Int(idx)) => {
                if idx < 0 || idx as usize >= items.len() {
                    return Err(self.error(format!(
                        "index {} out of bounds for array of length {}",
                        idx,
                        items.len()
                    )));
                }
                Ok(items[idx as usize].clone())
            }
            (Value::Array(_), other) => Err(self.error(format!(
                "array index must be an integer, got {}",
                other.type_name()
            ))),
            (Value::Map(pairs), Value::Str(key)) => pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| self.error(format!("key '{}' not found in map", key))),
            (Value::Map(_), other) => Err(self.error(format!(
                "map key must be a string, got {}",
                other.type_name()
            ))),
            (other, _) => Err(self.error(format!("cannot index into {}", other.type_name()))),
        }
    }

    // Arrays and maps are both value types - "mutating" means reading a
    // copy, splicing in the new element/entry, and handing the patched copy
    // back to the caller to write wherever `object` actually lives.
    fn with_index_replaced(
        &mut self,
        object: &Expr,
        index_val: Value,
        new_elem: Value,
    ) -> Result<Value, RuntimeError> {
        let mut container = self.eval(object)?;
        match &mut container {
            Value::Array(items) => match index_val {
                Value::Int(idx) => {
                    if idx < 0 || idx as usize >= items.len() {
                        return Err(self.error(format!(
                            "index {} out of bounds for array of length {}",
                            idx,
                            items.len()
                        )));
                    }
                    items[idx as usize] = new_elem;
                    Ok(container)
                }
                other => Err(self.error(format!(
                    "array index must be an integer, got {}",
                    other.type_name()
                ))),
            },
            Value::Map(pairs) => match index_val {
                Value::Str(key) => {
                    match pairs.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, v)) => *v = new_elem,
                        None => pairs.push((key, new_elem)),
                    }
                    self.check_size_limits(&container)?;
                    Ok(container)
                }
                other => Err(self.error(format!(
                    "map key must be a string, got {}",
                    other.type_name()
                ))),
            },
            other => Err(self.error(format!("cannot index into {}", other.type_name()))),
        }
    }

    // Writes `value` to an lvalue: a bare variable, or (recursively) an index
    // into an array/map reached through one, e.g. `matrix[0][1] = x`.
    // Re-evaluates `object`/`index` at each nesting level beyond the first -
    // only observable if those sub-expressions have side effects, a known
    // limitation rather than a full lvalue-path pre-evaluation pass.
    fn assign_to_target(&mut self, target: &Expr, value: Value) -> Result<(), RuntimeError> {
        match target {
            Expr::Ident(name) => self.env.assign(name, value).map_err(|msg| self.error(msg)),
            Expr::Index { object, index } => {
                let index_val = self.eval(index)?;
                let patched = self.with_index_replaced(object, index_val, value)?;
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
        let index_val = self.eval(index)?;
        let current_container = self.eval(object)?;

        match current_container {
            Value::Array(mut items) => {
                let idx = match index_val {
                    Value::Int(i) => i,
                    other => {
                        return Err(self.error(format!(
                            "array index must be an integer, got {}",
                            other.type_name()
                        )));
                    }
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
                items[idx_usize] = new_elem.clone();
                self.assign_to_target(object, Value::Array(items))?;
                Ok(new_elem)
            }
            Value::Map(mut pairs) => {
                let key = match index_val {
                    Value::Str(s) => s,
                    other => {
                        return Err(self.error(format!(
                            "map key must be a string, got {}",
                            other.type_name()
                        )));
                    }
                };
                let existing = pairs.iter().position(|(k, _)| *k == key);
                let new_elem = match op {
                    Some(op) => {
                        let pos = existing.ok_or_else(|| {
                            self.error(format!(
                                "cannot use compound assignment on missing map key '{}'",
                                key
                            ))
                        })?;
                        let current_val = pairs[pos].1.clone();
                        let rhs = self.eval(value)?;
                        self.apply_binary_op(op, current_val, rhs)?
                    }
                    None => self.eval(value)?,
                };
                match existing {
                    Some(pos) => pairs[pos].1 = new_elem.clone(),
                    None => pairs.push((key, new_elem.clone())),
                }
                let patched = Value::Map(pairs);
                self.check_size_limits(&patched)?;
                self.assign_to_target(object, patched)?;
                Ok(new_elem)
            }
            other => Err(self.error(format!("cannot index into {}", other.type_name()))),
        }
    }

    // Mutating methods (see `Value::call_method`) reuse `assign_to_target` to
    // write the receiver back - fails on a non-lvalue receiver the same way
    // index-assignment does.
    fn eval_method_call(
        &mut self,
        target: &Expr,
        method: &str,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
        let mut receiver = self.eval(target)?;
        let arg_values = args
            .iter()
            .map(|arg| self.eval(arg))
            .collect::<Result<Vec<_>, _>>()?;

        let result = receiver
            .call_method(method, &arg_values)
            .map_err(|msg| self.error(msg))?;

        match result {
            MethodResult::Pure(value) => {
                self.check_size_limits(&value)?;
                Ok(value)
            }
            MethodResult::Mutating(value) => {
                self.check_size_limits(&receiver)?;
                self.assign_to_target(target, receiver)?;
                Ok(value)
            }
        }
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
            other => Err(self.error(format!("cannot call {}", other.type_name()))),
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
        if self.call_depth >= self.max_call_depth {
            return Err(self.error(format!(
                "stack overflow: exceeded maximum call depth of {}",
                self.max_call_depth
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
