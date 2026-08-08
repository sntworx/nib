// Indexing and index-assignment - `a[i]`, `m[k]`, and the copy-on-write
// write-back machinery behind mutating them. The subtlest code in the crate:
// see `detach_binding` for why a mutation has to take the value out of its
// binding first, and why the size budgets are pre-checked rather than
// verified after the fact.

use std::rc::Rc;

use crate::ast::types::{BinaryOp, Expr};
use crate::runtime::types::{RuntimeError, Value};

use super::Interpreter;

impl Interpreter {
    pub(super) fn eval_index(
        &mut self,
        object: &Expr,
        index: &Expr,
    ) -> Result<Value, RuntimeError> {
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
                .lookup(&key)
                .cloned()
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
                    Rc::make_mut(items).set(idx as usize, new_elem);
                    self.check_size_limits(&container)?;
                    Ok(container)
                }
                other => Err(self.error(format!(
                    "array index must be an integer, got {}",
                    other.type_name()
                ))),
            },
            Value::Map(pairs) => match index_val {
                Value::Str(key) => {
                    Rc::make_mut(pairs).insert(key, new_elem);
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
    pub(super) fn assign_to_target(
        &mut self,
        target: &Expr,
        value: Value,
    ) -> Result<(), RuntimeError> {
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

    pub(super) fn eval_index_assign(
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
                let current_nodes = current_elem.nodes();
                let rhs = self.eval(value)?;
                let new_elem = match op {
                    Some(op) => self.apply_binary_op(op, current_elem, rhs)?,
                    None => rhs,
                };
                // Replacing an element can still grow the *tree* (`a[0] = a`),
                // so the budget is checked before the in-place write - after
                // detach_binding there is nothing left to roll back to.
                self.check_node_budget(
                    items
                        .node_count()
                        .saturating_sub(current_nodes)
                        .saturating_add(new_elem.nodes()),
                )?;
                self.check_depth_budget(items.depth_count().max(new_elem.depth() + 1))?;
                // Safe to detach only now: nothing below can fail and strand
                // the binding.
                self.detach_binding(object);
                Rc::make_mut(&mut items).set(idx_usize, new_elem.clone());
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
                let existing = pairs.position(&key);
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
                // Everything is pre-checked so the write below can't fail:
                // detach_binding leaves nothing to roll back to, and without
                // detaching, make_mut would deep-copy the whole map on every
                // single insert - which is what made filling one quadratic.
                if existing.is_none() && pairs.len() >= self.max_map_size {
                    return Err(self.error(format!(
                        "map exceeds maximum size of {} entries",
                        self.max_map_size
                    )));
                }
                let replaced_nodes = existing.map_or(0, |pos| pairs[pos].1.nodes());
                self.check_node_budget(
                    pairs
                        .node_count()
                        .saturating_sub(replaced_nodes)
                        .saturating_add(new_elem.nodes()),
                )?;
                self.check_depth_budget(pairs.depth_count().max(new_elem.depth() + 1))?;

                self.detach_binding(object);
                // upsert - `existing` above only decided whether a compound
                // op had a value to combine with
                Rc::make_mut(&mut pairs).insert(key, new_elem.clone());
                self.assign_to_target(object, Value::Map(pairs))?;
                Ok(new_elem)
            }
            other => Err(self.error(format!("cannot index into {}", other.type_name()))),
        }
    }

    pub(super) fn detach_binding(&mut self, target: &Expr) -> bool {
        match target {
            Expr::Ident(name) => self.env.take(name).is_some(),
            _ => false,
        }
    }

    pub(super) fn restore_binding(&mut self, target: &Expr, value: Value) {
        if let Expr::Ident(name) = target {
            let _ = self.env.assign(name, value);
        }
    }
}
