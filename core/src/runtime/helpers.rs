use crate::runtime::types::Value;

pub fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(v) => Some(*v as f64),
        Value::Float(v) => Some(*v),
        _ => None,
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (as_f64(a), as_f64(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

// Falsy: false, null, zero, empty string, empty array, empty map. Everything
// else - including "0", [0] and any function - is truthy. Empty collections
// are falsy so `if items { }` reads as "has items"; "0" is deliberately not,
// unlike PHP, since a non-empty string being falsy surprises everyone once.
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(v) => *v,
        Value::Null => false,
        Value::Int(v) => *v != 0,
        Value::Float(v) => *v != 0.0,
        Value::Str(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Map(v) => !v.is_empty(),
        Value::Function(_) | Value::NativeFunction(_) => true,
    }
}

// Float ops silently produce inf/-inf/NaN instead of panicking on overflow -
// turns that into an error instead of letting it propagate as a bad value.
pub fn checked_float(result: f64) -> Result<Value, String> {
    if result.is_finite() {
        Ok(Value::Float(result))
    } else {
        Err("floating-point overflow".to_string())
    }
}

// Rust's `f64 as i64` cast silently *saturates* on out-of-range values -
// errors instead, like every other numeric conversion here.
pub fn checked_i64_from_f64(f: f64) -> Result<i64, String> {
    // Comparing against the literal power of two, not `i64::MAX as f64`
    // (which rounds up to it), since i64::MAX isn't exactly representable in f64.
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    const I64_MAX_EXCLUSIVE_F64: f64 = 9223372036854775808.0;
    if (I64_MIN_F64..I64_MAX_EXCLUSIVE_F64).contains(&f) {
        Ok(f as i64)
    } else {
        Err(format!("{} is out of range for int conversion", f))
    }
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
