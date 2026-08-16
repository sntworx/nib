// The interpreter core: state, construction, and the cross-cutting error
// constructors. The behavior lives in the submodules below, which are children
// rather than siblings on purpose: Rust privacy is module-tree scoped, so a
// child can reach `Interpreter`'s private fields directly and the split costs
// no `pub(crate)` widening anywhere.
mod calls;
mod exec;
mod expr;
mod index;
mod limits;

use std::rc::Rc;

use crate::ast::Ast;
use crate::runtime::environment::Environment;
use crate::runtime::types::{NativeFunction, RuntimeError, Value};
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
    max_value_depth: usize,
    max_value_nodes: usize,
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
            max_value_depth: config.max_value_depth,
            max_value_nodes: config.max_value_nodes,
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
            fatal: false,
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
            fatal: false,
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

    // Same global-scope binding `register_native` uses, minus the
    // `NativeFunction` wrapping - any `Value` the host already has in hand.
    pub fn register_value(&mut self, name: impl Into<String>, value: Value) {
        self.env.define(name.into(), value);
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
}
