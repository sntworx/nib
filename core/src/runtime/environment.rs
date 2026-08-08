use std::collections::HashMap;

use crate::runtime::types::Value;

pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.scopes
            .last_mut()
            .expect("at least one scope")
            .insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    // Takes the value out of a binding, leaving the name bound to Null.
    // Used to drop the environment's own Rc reference so the interpreter's
    // copy becomes uniquely owned and `Rc::make_mut` can mutate in place
    // instead of deep-copying - the caller MUST write the value back (or
    // restore it on failure), or the variable is left holding Null.
    pub fn take(&mut self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                return Some(std::mem::replace(slot, Value::Null));
            }
        }
        None
    }

    // Returns a plain message (rather than a RuntimeError) since Environment
    // has no access to the interpreter's current source position.
    pub fn assign(&mut self, name: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(format!("undefined variable '{}'", name))
    }

    // Function calls only see the global scope plus their own frames, not the
    // caller's locals. This strips everything but the global scope, handing
    // the stripped frames back so the caller can be restored afterward.
    pub fn enter_call(&mut self) -> Vec<HashMap<String, Value>> {
        self.scopes.split_off(1)
    }

    pub fn exit_call(&mut self, saved_locals: Vec<HashMap<String, Value>>) {
        self.scopes.truncate(1);
        self.scopes.extend(saved_locals);
    }
}
