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

// Float arithmetic never panics like integer overflow does - it silently
// produces inf/-inf/NaN instead - so this turns a non-finite result into an
// error message instead of letting it propagate as a bad value. Returns a
// plain message (rather than a RuntimeError) since this helper has no access
// to the interpreter's current source position.
pub fn checked_float(result: f64) -> Result<Value, String> {
    if result.is_finite() {
        Ok(Value::Float(result))
    } else {
        Err("floating-point overflow".to_string())
    }
}
