use ext_php_rs::convert::{IntoZval, IntoZvalDyn};
use ext_php_rs::error::Error as PhpRsError;
use ext_php_rs::prelude::*;
use ext_php_rs::types::{ZendCallable, ZendObject, Zval};
use ext_php_rs::zend::ClassEntry;
use lame_core::{Lame as LameCore, Value};

#[php_class]
pub struct Lame {
    lame: LameCore,
}

#[php_impl]
impl Lame {
    pub fn __construct() -> Self {
        Lame {
            lame: LameCore::new(),
        }
    }

    pub fn parse(&mut self, source: String) -> PhpResult<()> {
        self.lame.parse(&source).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn run(&mut self) -> PhpResult<()> {
        self.lame.run().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn disable_keywords(&mut self, keywords: Vec<String>) {
        self.lame
            .disable_keywords(keywords.iter().map(|k| k.as_str()).collect());
    }

    pub fn register_func(&mut self, name: String, callback: &Zval) -> PhpResult<()> {
        // Reflect the callback once, up front, so calls from lame scripts with
        // the wrong arity are rejected the same way lame_core already rejects
        // wrong-arity calls to its own user-defined functions.
        let arity = reflect_arity(callback).map_err(|e| e.to_string())?;

        // Clone (refcount bump, not a deep copy) so the callable outlives this call.
        let callable =
            ZendCallable::new_owned(callback.shallow_clone()).map_err(|e| e.to_string())?;

        let display_name = name.clone();
        self.lame
            .register_func(name, move |args: &[Value]| -> Result<Value, String> {
                if !arity.accepts(args.len()) {
                    return Err(format!(
                        "'{}' expects {}, got {}",
                        display_name,
                        arity.describe(),
                        args.len()
                    ));
                }

                let zvals = args
                    .iter()
                    .map(value_to_zval)
                    .collect::<Result<Vec<_>, _>>()?;
                let params: Vec<&dyn IntoZvalDyn> =
                    zvals.iter().map(|z| z as &dyn IntoZvalDyn).collect();

                let result = callable.try_call(params).map_err(describe_call_error)?;
                zval_to_value(&result)
            });

        Ok(())
    }
}

/// The parameter-count contract of a registered PHP callable. Obtained via
/// `ReflectionFunction::createFromCallable`, which - despite the name -
/// covers closures, named functions, invokable objects, and
/// `[$obj, "method"]`/`"Class::method"` callables uniformly: all of them
/// report through the same `ReflectionFunctionAbstract` methods.
struct Arity {
    min: usize,
    /// `None` means variadic (`...$args`): any count at or above `min` is accepted.
    max: Option<usize>,
}

impl Arity {
    fn accepts(&self, n: usize) -> bool {
        n >= self.min && self.max.is_none_or(|max| n <= max)
    }

    fn describe(&self) -> String {
        match self.max {
            Some(max) if max == self.min => format!("{} argument(s)", self.min),
            Some(max) => format!("{}-{} argument(s)", self.min, max),
            None => format!("at least {} argument(s)", self.min),
        }
    }
}

fn reflect_arity(callback: &Zval) -> Result<Arity, String> {
    // Normalize any callable shape (closure, named function, invokable
    // object, `[$obj, "method"]`, `"Class::method"`) into a `Closure` via
    // PHP's own `Closure::fromCallable`, so a single `ReflectionFunction`
    // can report on all of them uniformly - `ReflectionFunction` accepts
    // a `Closure` directly, whatever it originally wrapped.
    let to_closure =
        ZendCallable::try_from_name("Closure::fromCallable").map_err(|e| e.to_string())?;
    let closure = to_closure
        .try_call(vec![callback as &dyn IntoZvalDyn])
        .map_err(|e| e.to_string())?;

    let ce = ClassEntry::try_find("ReflectionFunction")
        .ok_or_else(|| "ReflectionFunction class not found".to_string())?;
    let reflected = ZendObject::new(ce);
    reflected
        .try_call_method("__construct", vec![&closure as &dyn IntoZvalDyn])
        .map_err(|e| e.to_string())?;

    let min = reflected
        .try_call_method("getNumberOfRequiredParameters", vec![])
        .map_err(|e| e.to_string())?
        .long()
        .ok_or_else(|| "expected int from getNumberOfRequiredParameters".to_string())?
        as usize;

    let variadic = reflected
        .try_call_method("isVariadic", vec![])
        .map_err(|e| e.to_string())?
        .bool()
        .ok_or_else(|| "expected bool from isVariadic".to_string())?;

    let max = if variadic {
        None
    } else {
        let n = reflected
            .try_call_method("getNumberOfParameters", vec![])
            .map_err(|e| e.to_string())?
            .long()
            .ok_or_else(|| "expected int from getNumberOfParameters".to_string())?;
        Some(n as usize)
    };

    Ok(Arity { min, max })
}

/// `ZendCallable::try_call`'s `Error::Exception` variant carries the raw
/// thrown object; its `Display` dumps the whole thing Zval-by-Zval (message,
/// trace, file, line, ...), which is both unreadable and has been observed to
/// embed a NUL byte that then fails a *second*, unrelated conversion when
/// ext-php-rs turns our returned `String` into a thrown PHP exception -
/// surfacing as a "contains NUL-bytes" error that buries the real one. Pull
/// out just `getMessage()` instead, which is what a caller actually wants.
fn describe_call_error(err: PhpRsError) -> String {
    match err {
        PhpRsError::Exception(exception) => exception
            .try_call_method("getMessage", vec![])
            .ok()
            .and_then(|message| message.string())
            .unwrap_or_else(|| "PHP callback threw an exception".to_string()),
        other => other.to_string(),
    }
}

fn value_to_zval(value: &Value) -> Result<Zval, String> {
    let zval = match value {
        Value::Int(i) => (*i).into_zval(false).map_err(|e| e.to_string())?,
        Value::Float(f) => (*f).into_zval(false).map_err(|e| e.to_string())?,
        Value::Str(s) => s.clone().into_zval(false).map_err(|e| e.to_string())?,
        Value::Bool(b) => (*b).into_zval(false).map_err(|e| e.to_string())?,
        Value::Null => Zval::null(),
        Value::Array(items) => items
            .iter()
            .map(value_to_zval)
            .collect::<Result<Vec<_>, _>>()?
            .into_zval(false)
            .map_err(|e| e.to_string())?,
        Value::Function(_) | Value::NativeFunction(_) => {
            return Err("cannot pass a function value to a PHP callback".to_string());
        }
    };
    Ok(zval)
}

fn zval_to_value(zval: &Zval) -> Result<Value, String> {
    if let Some(i) = zval.long() {
        Ok(Value::Int(i))
    } else if let Some(f) = zval.double() {
        Ok(Value::Float(f))
    } else if let Some(b) = zval.bool() {
        Ok(Value::Bool(b))
    } else if let Some(s) = zval.string() {
        Ok(Value::Str(s))
    } else if zval.is_null() {
        Ok(Value::Null)
    } else if let Some(arr) = zval.array() {
        arr.values()
            .map(zval_to_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    } else {
        Err("unsupported PHP value returned from callback".to_string())
    }
}

#[php_module]
pub fn module(module: ModuleBuilder) -> ModuleBuilder {
    module.name("lame").class::<Lame>()
}
