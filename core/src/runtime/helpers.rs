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

// Rust's `f64 as i64` cast *saturates* on out-of-range values rather than
// erroring, unlike every other numeric conversion in this interpreter
// (`checked_add`, `checked_div`, `checked_float`) - used by `Float`'s
// `floor`/`ceil`/`round` pseudo-methods so an out-of-range float errors
// instead of silently handing back a wrong-but-plausible `i64`.
pub fn checked_i64_from_f64(f: f64) -> Result<i64, String> {
    // 2^63 is the exclusive upper edge of i64's range and is exactly
    // representable in f64; `i64::MAX as f64` rounds *up* to this same
    // value (f64 can't represent i64::MAX exactly), so comparing against
    // the literal power of two avoids relying on that rounding coincidence.
    const I64_MIN_F64: f64 = -9223372036854775808.0;
    const I64_MAX_EXCLUSIVE_F64: f64 = 9223372036854775808.0;
    if (I64_MIN_F64..I64_MAX_EXCLUSIVE_F64).contains(&f) {
        Ok(f as i64)
    } else {
        Err(format!("{} is out of range for int conversion", f))
    }
}
