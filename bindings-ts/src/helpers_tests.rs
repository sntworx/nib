//! Conversion tests for the wasm boundary.
//!
//! These need a real JS environment to build `JsValue`s, so they're
//! `#[wasm_bindgen_test]` rather than plain `#[test]` - run them with
//! `just test-ts` (`wasm-pack test --node`), not `cargo test`.

use super::*;
use js_sys::{Array, Object, Reflect};
use nib_lang::Value;
use wasm_bindgen_test::*;

fn obj(pairs: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (k, v) in pairs {
        Reflect::set(&o, &JsValue::from_str(k), v).unwrap();
    }
    o.into()
}

fn arr(items: &[JsValue]) -> JsValue {
    let a = Array::new();
    for i in items {
        a.push(i);
    }
    a.into()
}

// --- nib -> js ------------------------------------------------------------

#[wasm_bindgen_test]
fn scalars_convert_to_js() {
    assert_eq!(value_to_js(&Value::Int(42)).unwrap().as_f64(), Some(42.0));
    assert_eq!(value_to_js(&Value::Float(2.5)).unwrap().as_f64(), Some(2.5));
    assert_eq!(
        value_to_js(&Value::Str("s".into())).unwrap().as_string(),
        Some("s".to_string())
    );
    assert_eq!(
        value_to_js(&Value::Bool(true)).unwrap().as_bool(),
        Some(true)
    );
    assert!(value_to_js(&Value::Null).unwrap().is_null());
}

#[wasm_bindgen_test]
fn arrays_and_maps_convert_to_js() {
    let js = value_to_js(&Value::array(vec![Value::Int(1), Value::Int(2)])).unwrap();
    let a: Array = js.dyn_into().unwrap();
    assert_eq!(a.length(), 2);
    assert_eq!(a.get(1).as_f64(), Some(2.0));

    let js = value_to_js(&Value::map(vec![
        ("a".into(), Value::Int(1)),
        ("b".into(), Value::array(vec![Value::Int(2)])),
    ]))
    .unwrap();
    assert_eq!(
        Reflect::get(&js, &JsValue::from_str("a")).unwrap().as_f64(),
        Some(1.0)
    );
    let nested: Array = Reflect::get(&js, &JsValue::from_str("b"))
        .unwrap()
        .dyn_into()
        .unwrap();
    assert_eq!(nested.get(0).as_f64(), Some(2.0));
}

// --- js -> nib ------------------------------------------------------------

#[wasm_bindgen_test]
fn scalars_convert_from_js() {
    assert_eq!(js_to_value(&JsValue::from_f64(3.0)).unwrap(), Value::Int(3));
    assert_eq!(
        js_to_value(&JsValue::from_f64(2.5)).unwrap(),
        Value::Float(2.5)
    );
    assert_eq!(
        js_to_value(&JsValue::from_str("s")).unwrap(),
        Value::Str("s".into())
    );
    assert_eq!(js_to_value(&JsValue::TRUE).unwrap(), Value::Bool(true));
    assert_eq!(js_to_value(&JsValue::NULL).unwrap(), Value::Null);
    assert_eq!(js_to_value(&JsValue::UNDEFINED).unwrap(), Value::Null);
}

#[wasm_bindgen_test]
fn structures_convert_from_js() {
    let v = js_to_value(&arr(&[JsValue::from_f64(1.0), JsValue::from_str("x")])).unwrap();
    assert_eq!(v, Value::array(vec![Value::Int(1), Value::Str("x".into())]));

    let v = js_to_value(&obj(&[
        ("a", JsValue::from_f64(1.0)),
        ("b", arr(&[JsValue::from_f64(2.0)])),
    ]))
    .unwrap();
    assert_eq!(
        v,
        Value::map(vec![
            ("a".into(), Value::Int(1)),
            ("b".into(), Value::array(vec![Value::Int(2)])),
        ])
    );
}

#[wasm_bindgen_test]
fn round_trip_preserves_nested_structure() {
    let original = Value::map(vec![
        ("n".into(), Value::Int(1)),
        (
            "list".into(),
            Value::array(vec![Value::Str("a".into()), Value::Bool(false)]),
        ),
        (
            "nested".into(),
            Value::map(vec![("deep".into(), Value::Null)]),
        ),
    ]);
    let back = js_to_value(&value_to_js(&original).unwrap()).unwrap();
    assert_eq!(back, original);
}

// --- the depth cap --------------------------------------------------------
//
// Without it a cyclic or merely deep value overflows the wasm stack, and that
// unwind skips Rust destructors - poisoning the entire module, not just the
// Nib instance that hit it.

#[wasm_bindgen_test]
fn deep_js_values_are_rejected_rather_than_overflowing() {
    let mut deep = JsValue::from_f64(1.0);
    for _ in 0..200 {
        deep = arr(&[deep]);
    }
    let err = js_to_value(&deep).unwrap_err();
    assert!(err.contains("nested deeper than 128"), "{}", err);
}

#[wasm_bindgen_test]
fn cyclic_js_values_are_rejected() {
    let o = Object::new();
    Reflect::set(&o, &JsValue::from_str("self"), &o).unwrap();
    let err = js_to_value(&o.into()).unwrap_err();
    assert!(err.contains("cyclic value?"), "{}", err);
}

#[wasm_bindgen_test]
fn deep_nib_values_are_rejected_on_the_way_out() {
    let mut deep = Value::Int(1);
    for _ in 0..200 {
        deep = Value::array(vec![deep]);
    }
    assert!(value_to_js(&deep).unwrap_err().contains("nested deeper"));
}

/// Just under the cap must still convert - the limit is a ceiling, not a
/// blanket rejection of nesting.
#[wasm_bindgen_test]
fn values_just_under_the_cap_still_convert() {
    let mut deep = Value::Int(1);
    for _ in 0..100 {
        deep = Value::array(vec![deep]);
    }
    let js = value_to_js(&deep).unwrap();
    assert_eq!(js_to_value(&js).unwrap(), deep);
}

// --- config ---------------------------------------------------------------

#[wasm_bindgen_test]
fn missing_options_give_the_defaults() {
    let d = nib_lang::Config::default();
    assert_eq!(parse_config(&JsValue::UNDEFINED).unwrap(), d);
    assert_eq!(parse_config(&JsValue::NULL).unwrap(), d);
    assert_eq!(parse_config(&Object::new().into()).unwrap(), d);
}

#[wasm_bindgen_test]
fn every_config_field_is_read() {
    let opts = obj(&[
        ("maxCallDepth", JsValue::from_f64(1.0)),
        ("maxParseDepth", JsValue::from_f64(2.0)),
        ("maxSteps", JsValue::from_f64(3.0)),
        ("maxStringLength", JsValue::from_f64(4.0)),
        ("maxArrayLength", JsValue::from_f64(5.0)),
        ("maxMapSize", JsValue::from_f64(6.0)),
        ("maxValueDepth", JsValue::from_f64(7.0)),
        ("maxValueNodes", JsValue::from_f64(8.0)),
    ]);
    let c = parse_config(&opts).unwrap();
    assert_eq!(
        (
            c.max_call_depth,
            c.max_parse_depth,
            c.max_steps,
            c.max_string_length,
            c.max_array_length,
            c.max_map_size,
            c.max_value_depth,
            c.max_value_nodes
        ),
        (1, 2, 3, 4, 5, 6, 7, 8)
    );
}

/// A partial options object overrides only what it names.
#[wasm_bindgen_test]
fn unspecified_config_fields_keep_their_defaults() {
    let c = parse_config(&obj(&[("maxSteps", JsValue::from_f64(9.0))])).unwrap();
    assert_eq!(c.max_steps, 9);
    assert_eq!(c.max_call_depth, nib_lang::Config::default().max_call_depth);
}

#[wasm_bindgen_test]
fn invalid_config_values_are_rejected_by_name() {
    for bad in [
        JsValue::from_str("lots"),
        JsValue::from_f64(-1.0),
        JsValue::TRUE,
    ] {
        let err = parse_config(&obj(&[("maxSteps", bad)])).unwrap_err();
        assert!(
            describe_js_error(&err).contains("'maxSteps' must be a non-negative number"),
            "{:?}",
            err
        );
    }
}

// --- error description ----------------------------------------------------

#[wasm_bindgen_test]
fn js_errors_are_described_from_several_shapes() {
    assert_eq!(
        describe_js_error(&js_sys::Error::new("boom").into()),
        "boom"
    );
    assert_eq!(
        describe_js_error(&JsValue::from_str("plain string")),
        "plain string"
    );
    assert_eq!(
        describe_js_error(&JsValue::from_f64(1.0)),
        "unknown JS error"
    );
}
