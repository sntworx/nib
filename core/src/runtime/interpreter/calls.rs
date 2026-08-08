// Calling things: user functions, native host functions, and the closed set
// of built-in pseudo-methods.

use crate::ast::types::Expr;
use crate::runtime::types::{Function, MethodResult, RuntimeError, Value};

use super::{Flow, Interpreter};

impl Interpreter {
    // Mutating methods (see `Value::call_method`) reuse `assign_to_target` to
    // write the receiver back - fails on a non-lvalue receiver the same way
    // index-assignment does.
    pub(super) fn eval_method_call(
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

        if method == "push"
            && let Some(pushed) = arg_values.first()
        {
            self.check_push_room(&receiver, pushed)?;
        }

        // Only after the args are evaluated - they may still read the binding
        // (`a.push(a.len())`), and detaching earlier would hand them a Null.
        let detached = Value::method_mutates(method) && self.detach_binding(target);

        let result = match receiver.call_method(method, &arg_values) {
            Ok(result) => result,
            Err(msg) => {
                // put the binding back, or a failed `pop()` leaves it Null
                if detached {
                    self.restore_binding(target, receiver);
                }
                return Err(self.error(msg));
            }
        };

        match result {
            MethodResult::Pure(value) => {
                self.check_size_limits(&value)?;
                Ok(value)
            }
            MethodResult::Mutating(value) => {
                // No size check here: `push` was pre-checked above (the only
                // grower), and pop/remove only shrink. Checking after the fact
                // would be worse than useless - detach_binding has already
                // left the binding Null, so an error here would strand it.
                self.assign_to_target(target, receiver)?;
                Ok(value)
            }
        }
    }

    pub(super) fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
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
