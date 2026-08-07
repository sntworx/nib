use std::fmt;
use std::rc::Rc;

use crate::ast::types::AstNode;
use crate::runtime::helpers::checked_i64_from_f64;

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<AstNode>,
}

// A Rust function injected into Nib's global scope. Takes already-evaluated
// arguments and returns a plain message on failure (rather than a
// RuntimeError) since it has no access to the interpreter's source position -
// same convention as the other places in runtime/ that can't build one
// directly (see `Environment::assign`, `checked_float`).
pub struct NativeFunction {
    pub name: String,
    pub func: Box<dyn Fn(&[Value]) -> Result<Value, String>>,
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
    Function(Rc<Function>),
    NativeFunction(Rc<NativeFunction>),
    Null,
}

impl Value {
    // Used in "wrong type" runtime error messages instead of the value's own
    // Display - a script-controlled `Str`/`Array` can contain arbitrary raw
    // bytes (there's no escaping in `Display`, since it's also what backs
    // deliberate string output like `+`'s stringification and a host's
    // `print`), and those error messages get written straight to the host's
    // terminal/log with no sanitization anywhere in the pipeline. Reporting
    // the type instead of the value avoids ever echoing attacker-controlled
    // content (e.g. terminal escape sequences) through a diagnostic path the
    // script didn't explicitly ask to print through.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Bool(_) => "bool",
            Value::Array(_) => "array",
            Value::Function(_) => "function",
            Value::NativeFunction(_) => "native function",
            Value::Null => "null",
        }
    }

    // Dispatch for `target.method(args)` - a small, closed, interpreter-known
    // set of pseudo-methods on built-in types, not general member access or
    // user-extensible dispatch (there's no way for a script or host to add
    // to this). Returns a plain message on failure, same convention as
    // `Environment::assign`/`checked_float`/native functions, since this has
    // no access to the interpreter's source position either.
    //
    // Mutating methods mutate `self` in place and return `Mutating(value)`;
    // the interpreter is responsible for writing `self`'s new state back to
    // wherever the receiver expression came from (see `assign_to_target`).
    // `value` is what the *expression* evaluates to, which isn't always
    // `self`'s new state - `pop` mutates the array but evaluates to the
    // removed element, not the shrunk array.
    pub(crate) fn call_method(&mut self, name: &str, args: &[Value]) -> Result<MethodResult, String> {
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
            (Value::Str(s), "len") => {
                if !args.is_empty() {
                    return Err(format!("'len' expects 0 arguments, got {}", args.len()));
                }
                // char count, not byte count - `String::len` is O(1) but
                // wrong for anything with multi-byte characters, and
                // silently-wrong length is worse than the O(n) cost here
                Ok(MethodResult::Pure(Value::Int(s.chars().count() as i64)))
            }
            // Bridges a string into an array of single-character strings,
            // rather than teaching `eval_index`/`exec_for_in` a second
            // `Value::Str` case each - `s.chars()[i]` and
            // `for c in s.chars() { }` reuse all of Array's existing
            // indexing/iteration machinery for free instead of duplicating it.
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
            // Float-only: an Int is already an integer, so there's nothing
            // for these to do - falls through to the "no method" catch-all
            // below, same as calling an Array method on a Str would.
            // Returns Int (not a Float with a zeroed fraction) since the
            // usual reason to want this conversion is to use the result as
            // an array index, which requires a literal `Value::Int`.
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
