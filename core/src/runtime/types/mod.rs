// The value model: `Value` itself, the callable payloads, and `RuntimeError`.
// `containers` holds the array/map payloads (private fields, so mutation is
// forced through their methods) and `methods` holds the pseudo-method set.
mod containers;
mod methods;

use std::fmt;
use std::rc::Rc;

use crate::ast::types::AstNode;

pub(crate) use containers::{ArrayData, MapData};

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

// Arrays and maps stay *value* types to a script - `var b = a; b[0] = 1;`
// must never touch `a` - but the Rc means that promise costs a refcount bump
// instead of a deep copy. Every write goes through `Rc::make_mut`, which
// clones only when the data is actually shared, so the observable semantics
// are unchanged while `a.push(x)` stops being O(len) and `a = [a, a]` stops
// physically duplicating anything. Nothing may mutate the inner Vec except
// through `make_mut`: an aliased `&mut` would leak sharing into script-visible
// behavior, which is the one way this representation can go wrong.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Array(Rc<ArrayData>),
    Map(Rc<MapData>),
    Function(Rc<Function>),
    NativeFunction(Rc<NativeFunction>),
    Null,
}

impl Value {
    // Public: a host building a return value for `register_func` shouldn't
    // have to know the payload is behind an Rc.
    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(ArrayData::new(items)))
    }

    pub fn map(pairs: Vec<(String, Value)>) -> Value {
        Value::Map(Rc::new(MapData::new(pairs)))
    }

    // 0 for a leaf, so `[1, 2]` is 1 and `[[1]]` is 2. Cached, not computed -
    // see ArrayData.
    pub(crate) fn depth(&self) -> usize {
        match self {
            Value::Array(data) => data.depth_count(),
            Value::Map(data) => data.depth_count(),
            _ => 0,
        }
    }

    // Total values in this subtree counting itself, so `[1, 2]` is 3. Counts
    // the *logical* tree, not physical memory: copy-on-write means `[a, a]`
    // stores one shared `a` but still prints, compares and converts as two, so
    // this is what actually bounds those walks. Cached and maintained in O(1),
    // saturating rather than wrapping - `a = [a, a]` doubles it, so it reaches
    // usize::MAX in 64 steps.
    pub(crate) fn nodes(&self) -> usize {
        match self {
            Value::Array(data) => data.node_count(),
            Value::Map(data) => data.node_count(),
            _ => 1,
        }
    }

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

    // Whether `call_method` will mutate its receiver. Knowing this *before*
    // dispatch lets the interpreter hand over a uniquely-owned receiver (see
    // `detach_binding`), so `Rc::make_mut` mutates in place rather than
    // deep-copying - the difference between O(1) and O(len) per `push`. Keep
    // in sync with the `MethodResult::Mutating` arms below.
    pub(crate) fn method_mutates(name: &str) -> bool {
        matches!(name, "push" | "pop" | "remove")
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
            // ptr_eq first: after copy-on-write, comparing a value against a
            // copy of itself is the common case, and identity settles it
            // without walking either side.
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b) || a == b,
            // Order-independent: unlike Vec<(String,Value)>'s own derived
            // PartialEq, two maps with the same keys/values in different
            // insertion order must compare equal. Done here rather than via a
            // `PartialEq for MapData` precisely so no one can delegate to a
            // field-wise impl and silently make map equality order-sensitive.
            (Value::Map(a), Value::Map(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter()
                            .all(|(k, v)| b.iter().any(|(k2, v2)| k == k2 && v == v2)))
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
