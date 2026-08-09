use super::*;

// Falsy: false, null, zero, empty string, empty array, empty map. Everything
// else is truthy - including "0" (unlike PHP) and [0].
#[test]
fn truthiness_table() {
    let falsy = [
        Value::Bool(false),
        Value::Null,
        Value::Int(0),
        Value::Float(0.0),
        Value::Float(-0.0),
        Value::Str(String::new()),
        Value::array(vec![]),
        Value::map(vec![]),
    ];
    for v in &falsy {
        assert!(!is_truthy(v), "{} should be falsy", v);
    }

    let truthy = [
        Value::Bool(true),
        Value::Int(1),
        Value::Int(-1),
        Value::Float(0.5),
        Value::Str("0".into()),
        Value::Str(" ".into()),
        Value::array(vec![Value::Int(0)]),
        Value::map(vec![("k".into(), Value::Null)]),
    ];
    for v in &truthy {
        assert!(is_truthy(v), "{} should be truthy", v);
    }
}

#[test]
fn equality_coerces_between_int_and_float_only() {
    assert!(values_equal(&Value::Int(1), &Value::Float(1.0)));
    assert!(values_equal(&Value::Float(2.0), &Value::Int(2)));
    assert!(!values_equal(&Value::Int(1), &Value::Float(1.5)));

    // no cross-type coercion anywhere else - truthiness must not leak in here
    assert!(!values_equal(&Value::Int(0), &Value::Bool(false)));
    assert!(!values_equal(
        &Value::Str(String::new()),
        &Value::Bool(false)
    ));
    assert!(!values_equal(&Value::Str("1".into()), &Value::Int(1)));
    assert!(!values_equal(&Value::Null, &Value::Bool(false)));
}

#[test]
fn maps_compare_regardless_of_insertion_order() {
    let a = Value::map(vec![
        ("x".into(), Value::Int(1)),
        ("y".into(), Value::Int(2)),
    ]);
    let b = Value::map(vec![
        ("y".into(), Value::Int(2)),
        ("x".into(), Value::Int(1)),
    ]);
    assert!(values_equal(&a, &b));
}

#[test]
fn checked_float_rejects_non_finite_results() {
    assert_eq!(checked_float(1.5), Ok(Value::Float(1.5)));
    assert!(checked_float(f64::INFINITY).is_err());
    assert!(checked_float(f64::NEG_INFINITY).is_err());
    assert!(checked_float(f64::NAN).is_err());
}

// Rust's `f64 as i64` silently *saturates* out of range instead of erroring,
// unlike every other numeric conversion here.
#[test]
fn checked_i64_from_f64_guards_the_range() {
    assert_eq!(checked_i64_from_f64(3.7), Ok(3)); // truncates toward zero
    assert_eq!(checked_i64_from_f64(-3.7), Ok(-3));
    assert_eq!(checked_i64_from_f64(0.0), Ok(0));

    // i64::MIN is exactly representable in f64; i64::MAX is not - it rounds up
    // to 2^63, which is out of range, so the bound is compared as exclusive.
    assert_eq!(checked_i64_from_f64(-9223372036854775808.0), Ok(i64::MIN));
    assert!(checked_i64_from_f64(9223372036854775808.0).is_err());
    assert!(checked_i64_from_f64(1e19).is_err());
    assert!(checked_i64_from_f64(-1e19).is_err());
}

#[test]
fn as_f64_widens_numbers_only() {
    assert_eq!(as_f64(&Value::Int(3)), Some(3.0));
    assert_eq!(as_f64(&Value::Float(3.5)), Some(3.5));
    assert_eq!(as_f64(&Value::Str("3".into())), None);
    assert_eq!(as_f64(&Value::Bool(true)), None);
    assert_eq!(as_f64(&Value::Null), None);
}
