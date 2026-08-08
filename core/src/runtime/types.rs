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

// Payload behind Value::Array. Caches `depth` (0 for a leaf, so an array of
// scalars is 1) purely so a nesting limit can be enforced in O(1) when a value
// is built - computing it on demand would be O(nodes). Every recursive walk
// over a Value burns one native stack frame per level, and `Drop` is the walk
// that can't fail gracefully, so the limit is what keeps them all safe.
// `Deref` gives read-only Vec access at the ~40 sites that only read; writes go
// through the methods below, which are the only things allowed to touch
// `items`, since they're what keep `depth` honest.
#[derive(Debug, Clone)]
pub struct ArrayData {
    items: Vec<Value>,
    depth: usize,
    nodes: usize,
}

impl ArrayData {
    fn new(items: Vec<Value>) -> Self {
        let depth = items.iter().map(|v| v.depth() + 1).max().unwrap_or(1);
        let nodes = items
            .iter()
            .fold(1usize, |acc, v| acc.saturating_add(v.nodes()));
        ArrayData {
            items,
            depth,
            nodes,
        }
    }

    fn push(&mut self, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self.nodes.saturating_add(value.nodes());
        self.items.push(value);
    }

    fn pop(&mut self) -> Option<Value> {
        // `depth` deliberately isn't recomputed: shrinking can only lower it,
        // so the stale value stays a safe upper bound, and recomputing would
        // make `pop` O(len). `nodes` *is* exact - subtracting is O(1).
        let popped = self.items.pop();
        if let Some(value) = &popped {
            self.nodes = self.nodes.saturating_sub(value.nodes());
        }
        popped
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes
    }

    pub(crate) fn depth_count(&self) -> usize {
        self.depth
    }

    pub(crate) fn set(&mut self, index: usize, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self
            .nodes
            .saturating_sub(self.items[index].nodes())
            .saturating_add(value.nodes());
        self.items[index] = value;
    }
}

impl std::ops::Deref for ArrayData {
    type Target = Vec<Value>;
    fn deref(&self) -> &Vec<Value> {
        &self.items
    }
}

// Structural, ignoring the cached depth - it's an upper bound (see `pop`), so
// two equal arrays can legitimately carry different values for it.
impl PartialEq for ArrayData {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl Drop for ArrayData {
    fn drop(&mut self) {
        drop_nested(std::mem::take(&mut self.items));
    }
}

// Payload behind Value::Map - same rationale as ArrayData above.
#[derive(Debug, Clone)]
pub struct MapData {
    pairs: Vec<(String, Value)>,
    depth: usize,
    nodes: usize,
}

impl MapData {
    fn new(pairs: Vec<(String, Value)>) -> Self {
        let depth = pairs.iter().map(|(_, v)| v.depth() + 1).max().unwrap_or(1);
        let nodes = pairs
            .iter()
            .fold(1usize, |acc, (_, v)| acc.saturating_add(v.nodes()));
        MapData {
            pairs,
            depth,
            nodes,
        }
    }

    // Upsert: the one write maps need, and the only place a map grows.
    pub(crate) fn insert(&mut self, key: String, value: Value) {
        self.depth = self.depth.max(value.depth() + 1);
        self.nodes = self.nodes.saturating_add(value.nodes());
        match self.pairs.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => {
                self.nodes = self.nodes.saturating_sub(slot.nodes());
                *slot = value;
            }
            None => self.pairs.push((key, value)),
        }
    }

    fn remove_at(&mut self, index: usize) -> (String, Value) {
        // stale `depth` stays a safe upper bound, same as ArrayData::pop
        let removed = self.pairs.remove(index);
        self.nodes = self.nodes.saturating_sub(removed.1.nodes());
        removed
    }
}

impl std::ops::Deref for MapData {
    type Target = Vec<(String, Value)>;
    fn deref(&self) -> &Vec<(String, Value)> {
        &self.pairs
    }
}

impl PartialEq for MapData {
    fn eq(&self, other: &Self) -> bool {
        self.pairs == other.pairs
    }
}

impl Drop for MapData {
    fn drop(&mut self) {
        drop_nested(self.pairs.drain(..).map(|(_, v)| v).collect());
    }
}

// Tears nested containers down with an explicit worklist. The derived
// recursive drop walks one native stack frame per nesting level and aborts the
// process on a deep enough value - and unlike a RuntimeError, a Drop can't
// fail gracefully or be caught, so it has to be bounded structurally rather
// than checked. Taking the children out of each node before it falls out of
// scope is what stops the recursion re-entering.
fn drop_nested(mut worklist: Vec<Value>) {
    while let Some(value) = worklist.pop() {
        match value {
            Value::Array(rc) => {
                if let Some(mut data) = Rc::into_inner(rc) {
                    worklist.append(&mut data.items);
                }
            }
            Value::Map(rc) => {
                if let Some(mut data) = Rc::into_inner(rc) {
                    worklist.extend(data.pairs.drain(..).map(|(_, v)| v));
                }
            }
            _ => {}
        }
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
            Value::Array(data) => data.depth,
            Value::Map(data) => data.depth,
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
            Value::Array(data) => data.nodes,
            Value::Map(data) => data.nodes,
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
            // insertion order must compare equal.
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
