// The closed set of built-in pseudo-methods - `Value::call_method` and its
// Pure/Mutating result. Not general member access and not extensible by a
// script or a host: `.` resolves here or it's a runtime error.

use std::rc::Rc;

use super::{MethodResult, Value};
use crate::runtime::helpers::{checked_float, checked_i64_from_f64};

impl Value {
    // Dispatch for `target.method(args)` - a small, closed set of
    // pseudo-methods, not general/user-extensible member access. Mutating
    // methods mutate `self` and return `Mutating(value)`; the interpreter
    // writes `self` back to the receiver (see `assign_to_target`). `value` is
    // what the expression evaluates to, not always `self`'s new state - e.g.
    // `pop` evaluates to the removed element, not the shrunk array.
    pub(crate) fn call_method(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Result<MethodResult, String> {
        match (self, name) {
            (Value::Array(items), "len") => {
                if !args.is_empty() {
                    return Err(format!("'len' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Int(items.len() as i64)))
            }
            (Value::Array(items), "push") => {
                if args.len() != 1 {
                    return Err(format!("'push' expects 1 argument, got {}", args.len()));
                }
                Rc::make_mut(items).push(args[0].clone());
                // Rc clone, not a deep copy - `push` evaluating to the new
                // array used to cost a full duplicate of it.
                Ok(MethodResult::Mutating(Value::Array(items.clone())))
            }
            (Value::Array(items), "pop") => {
                if !args.is_empty() {
                    return Err(format!("'pop' expects 0 arguments, got {}", args.len()));
                }
                let popped = Rc::make_mut(items)
                    .pop()
                    .ok_or_else(|| "cannot pop from an empty array".to_string())?;
                Ok(MethodResult::Mutating(popped))
            }
            (Value::Map(pairs), "len") => {
                if !args.is_empty() {
                    return Err(format!("'len' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Int(pairs.len() as i64)))
            }
            (Value::Map(pairs), "has") => {
                if args.len() != 1 {
                    return Err(format!("'has' expects 1 argument, got {}", args.len()));
                }
                let key = match &args[0] {
                    Value::Str(s) => s,
                    _ => return Err("'has' expects a string argument".to_string()),
                };
                Ok(MethodResult::Pure(Value::Bool(
                    pairs.position(key).is_some(),
                )))
            }
            (Value::Map(pairs), "get") => {
                if args.len() != 1 {
                    return Err(format!("'get' expects 1 argument, got {}", args.len()));
                }
                let key = match &args[0] {
                    Value::Str(s) => s,
                    _ => return Err("'get' expects a string argument".to_string()),
                };
                Ok(MethodResult::Pure(
                    pairs.lookup(key).cloned().unwrap_or(Value::Null),
                ))
            }
            (Value::Map(pairs), "remove") => {
                if args.len() != 1 {
                    return Err(format!("'remove' expects 1 argument, got {}", args.len()));
                }
                let key = match &args[0] {
                    Value::Str(s) => s.clone(),
                    _ => return Err("'remove' expects a string argument".to_string()),
                };
                let pos = pairs
                    .position(&key)
                    .ok_or_else(|| format!("key '{}' not found in map", key))?;
                let (_, removed) = Rc::make_mut(pairs).remove_at(pos);
                Ok(MethodResult::Mutating(removed))
            }
            (Value::Map(pairs), "keys") => {
                if !args.is_empty() {
                    return Err(format!("'keys' expects 0 arguments, got {}", args.len()));
                }
                let keys = pairs.iter().map(|(k, _)| Value::Str(k.clone())).collect();
                Ok(MethodResult::Pure(Value::array(keys)))
            }
            (Value::Map(pairs), "values") => {
                if !args.is_empty() {
                    return Err(format!("'values' expects 0 arguments, got {}", args.len()));
                }
                let values = pairs.iter().map(|(_, v)| v.clone()).collect();
                Ok(MethodResult::Pure(Value::array(values)))
            }
            (Value::Str(s), "len") => {
                if !args.is_empty() {
                    return Err(format!("'len' expects 0 arguments, got {}", args.len()));
                }
                // char count, not byte count - String::len is O(1) but wrong for multi-byte chars
                Ok(MethodResult::Pure(Value::Int(s.chars().count() as i64)))
            }
            // Bridges to Array so indexing/iteration come for free instead of
            // duplicating that machinery for Str.
            (Value::Str(s), "chars") => {
                if !args.is_empty() {
                    return Err(format!("'chars' expects 0 arguments, got {}", args.len()));
                }
                let chars = s.chars().map(|c| Value::Str(c.to_string())).collect();
                Ok(MethodResult::Pure(Value::array(chars)))
            }
            (Value::Str(s), "upper") => {
                if !args.is_empty() {
                    return Err(format!("'upper' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Str(s.to_uppercase())))
            }
            (Value::Str(s), "lower") => {
                if !args.is_empty() {
                    return Err(format!("'lower' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Str(s.to_lowercase())))
            }
            (Value::Str(s), "trim") => {
                if !args.is_empty() {
                    return Err(format!("'trim' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Str(s.trim().to_string())))
            }
            // Fail loud on unparseable input, same convention as pop()/remove()
            // rather than a get()-style Null fallback. The message omits the
            // string itself, same reasoning as type_name() above - don't echo
            // unescaped script-controlled content (e.g. terminal escapes)
            // through a diagnostic path.
            (Value::Str(s), "to_int") => {
                if !args.is_empty() {
                    return Err(format!("'to_int' expects 0 arguments, got {}", args.len()));
                }
                let parsed = s
                    .parse::<i64>()
                    .map_err(|_| "cannot convert string to int".to_string())?;
                Ok(MethodResult::Pure(Value::Int(parsed)))
            }
            // Routed through checked_float since Rust's f64::from_str accepts
            // "inf"/"nan" as valid floats, which would violate the
            // non-finite-is-an-error invariant every other float path enforces.
            (Value::Str(s), "to_float") => {
                if !args.is_empty() {
                    return Err(format!(
                        "'to_float' expects 0 arguments, got {}",
                        args.len()
                    ));
                }
                let parsed = s
                    .parse::<f64>()
                    .map_err(|_| "cannot convert string to float".to_string())?;
                checked_float(parsed).map(MethodResult::Pure)
            }
            (Value::Int(v), "to_float") => {
                if !args.is_empty() {
                    return Err(format!(
                        "'to_float' expects 0 arguments, got {}",
                        args.len()
                    ));
                }
                Ok(MethodResult::Pure(Value::Float(*v as f64)))
            }
            (Value::Int(v), "to_str") => {
                if !args.is_empty() {
                    return Err(format!("'to_str' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Str(v.to_string())))
            }
            // Float-only (an Int has nothing to convert). Returns Int, not a
            // Float with a zeroed fraction, since the usual reason to want
            // this is to use the result as an array index.
            (Value::Float(f), "floor") => {
                if !args.is_empty() {
                    return Err(format!("'floor' expects 0 arguments, got {}", args.len()));
                }
                checked_i64_from_f64(f.floor()).map(|i| MethodResult::Pure(Value::Int(i)))
            }
            (Value::Float(f), "ceil") => {
                if !args.is_empty() {
                    return Err(format!("'ceil' expects 0 arguments, got {}", args.len()));
                }
                checked_i64_from_f64(f.ceil()).map(|i| MethodResult::Pure(Value::Int(i)))
            }
            (Value::Float(f), "round") => {
                if !args.is_empty() {
                    return Err(format!("'round' expects 0 arguments, got {}", args.len()));
                }
                checked_i64_from_f64(f.round()).map(|i| MethodResult::Pure(Value::Int(i)))
            }
            // Truncates toward zero (like Rust's `as i64`), distinct from
            // floor/ceil/round - this is a cast, not a fourth rounding mode.
            (Value::Float(f), "to_int") => {
                if !args.is_empty() {
                    return Err(format!("'to_int' expects 0 arguments, got {}", args.len()));
                }
                checked_i64_from_f64(*f).map(|i| MethodResult::Pure(Value::Int(i)))
            }
            (Value::Float(f), "to_str") => {
                if !args.is_empty() {
                    return Err(format!("'to_str' expects 0 arguments, got {}", args.len()));
                }
                Ok(MethodResult::Pure(Value::Str(f.to_string())))
            }
            (value, name) => Err(format!("no method '{}' on {}", name, value.type_name())),
        }
    }
}
