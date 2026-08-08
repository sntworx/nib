// The sandbox budgets, all in one place: the per-run step budget and the
// four value-size/shape checks. Kept together deliberately - "what bounds an
// untrusted script" should be answerable by reading one file.

use crate::runtime::types::{RuntimeError, Value};

use super::Interpreter;

impl Interpreter {
    // Called once per statement executed and once per loop iteration (see
    // exec_while/exec_for_body/exec_for_in), not once per exec() alone - a
    // non-empty loop body ticks twice per iteration (once for the loop
    // construct, once per body statement). Steps are a "units of work done"
    // budget, not a precise iteration count.
    pub(super) fn tick(&mut self) -> Result<(), RuntimeError> {
        if self.step_count >= self.max_steps {
            let mut err = self.error(format!(
                "exceeded maximum execution steps of {}",
                self.max_steps
            ));
            // uncatchable - the budget a catch block would need is the very
            // thing that just ran out (see RuntimeError::fatal)
            err.fatal = true;
            return Err(err);
        }
        self.step_count += 1;
        Ok(())
    }

    // Checked at every point a Str/Array/Map is constructed or grown (see
    // call sites) - a &self, not &mut self, check so it's callable from
    // apply_binary_op too.
    pub(super) fn check_size_limits(&self, value: &Value) -> Result<(), RuntimeError> {
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
            // Guards the native stack rather than memory - see max_value_depth
            value if value.depth() > self.max_value_depth => Err(self.error(format!(
                "value nested deeper than {} levels",
                self.max_value_depth
            ))),
            // Bounds the logical tree the per-container limits can't see
            value if value.nodes() > self.max_value_nodes => Err(self.error(format!(
                "value exceeds maximum total size of {} elements",
                self.max_value_nodes
            ))),
            _ => Ok(()),
        }
    }

    // Drops the environment's own reference to a bare identifier's binding so
    // the interpreter's copy of the value becomes uniquely owned, letting
    // `Rc::make_mut` mutate in place instead of deep-copying. Returns whether
    // anything was detached; if so the caller MUST write the value back
    // (`assign_to_target`) or restore it, since the binding now holds Null.
    // Aliased values (`var b = a;`) still have a refcount above 1 afterwards,
    // so make_mut correctly copies and `b` is left untouched.
    // `push` is the only mutating method that can grow a collection past a
    // size limit, and once `make_mut` has mutated in place there's no
    // pre-mutation copy left to roll back to - so its room is checked
    // *before* the mutation instead of the result being checked after.
    pub(super) fn check_node_budget(&self, nodes: usize) -> Result<(), RuntimeError> {
        if nodes > self.max_value_nodes {
            return Err(self.error(format!(
                "value exceeds maximum total size of {} elements",
                self.max_value_nodes
            )));
        }
        Ok(())
    }

    pub(super) fn check_depth_budget(&self, depth: usize) -> Result<(), RuntimeError> {
        if depth > self.max_value_depth {
            return Err(self.error(format!(
                "value nested deeper than {} levels",
                self.max_value_depth
            )));
        }
        Ok(())
    }

    pub(super) fn check_push_room(
        &self,
        receiver: &Value,
        value: &Value,
    ) -> Result<(), RuntimeError> {
        match receiver {
            Value::Array(items) if items.len() >= self.max_array_length => {
                Err(self.error(format!(
                    "array exceeds maximum length of {} elements",
                    self.max_array_length
                )))
            }
            Value::Array(_) => {
                self.check_node_budget(receiver.nodes().saturating_add(value.nodes()))?;
                self.check_depth_budget(receiver.depth().max(value.depth() + 1))
            }
            _ => Ok(()),
        }
    }
}
