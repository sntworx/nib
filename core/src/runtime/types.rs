use std::fmt;
use std::rc::Rc;

use crate::ast::types::AstNode;

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<AstNode>,
}

// A Rust function injected into Lame's global scope. Takes already-evaluated
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
