use std::fmt;
use std::rc::Rc;

use crate::ast::types::AstNode;
use crate::runtime::helpers::{checked_float, checked_i64_from_f64};

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<AstNode>,
}

pub type NativeCallback = Box<dyn Fn(&[Value]) -> Result<Value, String>>;

// Injected into Nib's global scope by the host. Returns a plain message on
// failure, not a RuntimeError, since it has no access to the interpreter's
// source position (see `Environment::assign`, `checked_float`).
pub struct NativeFunction {
    pub name: String,
    pub func: NativeCallback,
}

impl fmt::Debug for NativeFunction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NativeFunction({})", self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Array(Vec<Value>),
    Map(Vec<(String, Value)>),
    Function(Rc<Function>),
    NativeFunction(Rc<NativeFunction>),
    Null,
}

impl Value {
    // Used in "wrong type" error messages instead of the value's own Display,
    // which is unescaped - avoids echoing script-controlled content (e.g.
    // terminal escape sequences) through a diagnostic path.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Bool(_) => "bool",
            Value::Array(_) => "array",
            Value::Map(_) => "map",
            Value::Function(_) => "function",
            Value::NativeFunction(_) => "native function",
            Value::Null => "null",
        }
    }

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
                items.push(args[0].clone());
                Ok(MethodResult::Mutating(Value::Array(items.clone())))
            }
            (Value::Array(items), "pop") => {
                if !args.is_empty() {
                    return Err(format!("'pop' expects 0 arguments, got {}", args.len()));
                }
                let popped = items
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
                    pairs.iter().any(|(k, _)| k == key),
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
                    pairs
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(Value::Null),
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
                    .iter()
                    .position(|(k, _)| *k == key)
                    .ok_or_else(|| format!("key '{}' not found in map", key))?;
                let (_, removed) = pairs.remove(pos);
                Ok(MethodResult::Mutating(removed))
            }
            (Value::Map(pairs), "keys") => {
                if !args.is_empty() {
                    return Err(format!("'keys' expects 0 arguments, got {}", args.len()));
                }
                let keys = pairs.iter().map(|(k, _)| Value::Str(k.clone())).collect();
                Ok(MethodResult::Pure(Value::Array(keys)))
            }
            (Value::Map(pairs), "values") => {
                if !args.is_empty() {
                    return Err(format!("'values' expects 0 arguments, got {}", args.len()));
                }
                let values = pairs.iter().map(|(_, v)| v.clone()).collect();
                Ok(MethodResult::Pure(Value::Array(values)))
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
                Ok(MethodResult::Pure(Value::Array(chars)))
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

pub(crate) enum MethodResult {
    Pure(Value),
    Mutating(Value),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            // Order-independent: unlike Vec<(String,Value)>'s own derived
            // PartialEq, two maps with the same keys/values in different
            // insertion order must compare equal.
            (Value::Map(a), Value::Map(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2))
            }
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::NativeFunction(a), Value::NativeFunction(b)) => Rc::ptr_eq(a, b),
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::Str(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(pairs) => {
                write!(f, "{{")?;
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Function(func) => write!(f, "<function {}>", func.name),
            Value::NativeFunction(func) => write!(f, "<native function {}>", func.name),
            Value::Null => write!(f, "null"),
        }
    }
}

#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
    pub line: usize,
    pub col: usize,
    // Set only when raised via a script's own `throw expr;` (see
    // Interpreter::throw_error) - lets `catch` rebind the original Value, not
    // just its stringified message. None for every interpreter-raised error
    // (div by zero, missing key, etc.), which `catch` falls back to wrapping
    // as a Str of `message`.
    pub value: Option<Value>,
    // Propagates past `try` instead of being caught. Set only for a blown
    // `max_steps` budget: alone among the limit errors, its counter stays
    // exhausted, so a catch block's own first tick() would re-error before
    // running a single statement - catching it could never do anything but
    // misreport the failure at the catch block's position. Same reasoning
    // that keeps `exit;` off this channel entirely. `max_call_depth` and the
    // size limits are *not* fatal: they leave no counter exhausted, so a
    // catch after one runs normally.
    pub fatal: bool,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Runtime error at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}
