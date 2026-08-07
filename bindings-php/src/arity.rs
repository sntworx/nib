use ext_php_rs::convert::IntoZvalDyn;
use ext_php_rs::types::{ZendCallable, ZendObject, Zval};
use ext_php_rs::zend::ClassEntry;

/// The parameter-count contract of a registered PHP callable. Obtained via
/// `ReflectionFunction::createFromCallable`, which - despite the name -
/// covers closures, named functions, invokable objects, and
/// `[$obj, "method"]`/`"Class::method"` callables uniformly: all of them
/// report through the same `ReflectionFunctionAbstract` methods.
pub struct Arity {
    min: usize,
    /// `None` means variadic (`...$args`): any count at or above `min` is accepted.
    max: Option<usize>,
}

impl Arity {
    pub fn accepts(&self, n: usize) -> bool {
        n >= self.min && self.max.is_none_or(|max| n <= max)
    }

    pub fn describe(&self) -> String {
        match self.max {
            Some(max) if max == self.min => format!("{} argument(s)", self.min),
            Some(max) => format!("{}-{} argument(s)", self.min, max),
            None => format!("at least {} argument(s)", self.min),
        }
    }
}

pub fn reflect_arity(callback: &Zval) -> Result<Arity, String> {
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
