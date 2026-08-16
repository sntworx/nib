use js_sys::{Array, Object, Reflect};
use nib_lang::{Config, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub fn parse_config(options: &JsValue) -> Result<Config, JsValue> {
    let mut config = Config::default();
    if options.is_undefined() || options.is_null() {
        return Ok(config);
    }
    if let Some(v) = get_config_field(options, "maxCallDepth")? {
        config.max_call_depth = v;
    }
    if let Some(v) = get_config_field(options, "maxParseDepth")? {
        config.max_parse_depth = v;
    }
    if let Some(v) = get_config_field(options, "maxSteps")? {
        config.max_steps = v;
    }
    if let Some(v) = get_config_field(options, "maxStringLength")? {
        config.max_string_length = v;
    }
    if let Some(v) = get_config_field(options, "maxArrayLength")? {
        config.max_array_length = v;
    }
    if let Some(v) = get_config_field(options, "maxMapSize")? {
        config.max_map_size = v;
    }
    if let Some(v) = get_config_field(options, "maxValueDepth")? {
        config.max_value_depth = v;
    }
    if let Some(v) = get_config_field(options, "maxValueNodes")? {
        config.max_value_nodes = v;
    }
    Ok(config)
}

fn get_config_field(options: &JsValue, key: &str) -> Result<Option<usize>, JsValue> {
    let value = Reflect::get(options, &JsValue::from_str(key))
        .map_err(|e| JsValue::from_str(&describe_js_error(&e)))?;
    if value.is_undefined() {
        return Ok(None);
    }
    value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n as usize)
        .map(Some)
        .ok_or_else(|| JsValue::from_str(&format!("'{}' must be a non-negative number", key)))
}

pub fn describe_js_error(err: &JsValue) -> String {
    err.dyn_ref::<js_sys::Error>()
        .and_then(|e| e.message().as_string())
        .or_else(|| err.as_string())
        .unwrap_or_else(|| "unknown JS error".to_string())
}

// Both converters walk nested structures recursively. A cyclic JS value
// (`o.self = o`) recurses forever, and a merely deep one overflows the wasm
// stack - either way the unwind skips Rust destructors and poisons the whole
// module, killing every Nib instance in the process, not just this one. 128 is
// far below the ~2000 levels the shallower direction survives, and far above
// any real payload.
const MAX_CONVERSION_DEPTH: usize = 128;

fn too_deep() -> String {
    format!(
        "value nested deeper than {} levels (cyclic value?)",
        MAX_CONVERSION_DEPTH
    )
}

pub fn value_to_js(value: &Value) -> Result<JsValue, String> {
    value_to_js_at(value, 0)
}

fn value_to_js_at(value: &Value, depth: usize) -> Result<JsValue, String> {
    if depth > MAX_CONVERSION_DEPTH {
        return Err(too_deep());
    }
    Ok(match value {
        Value::Int(i) => JsValue::from_f64(*i as f64),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::Str(s) => JsValue::from_str(s),
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Null => JsValue::NULL,
        Value::Array(items) => {
            let arr = Array::new();
            for item in items.iter() {
                arr.push(&value_to_js_at(item, depth + 1)?);
            }
            arr.into()
        }
        Value::Map(pairs) => {
            let obj = Object::new();
            for (k, v) in pairs.iter() {
                Reflect::set(&obj, &JsValue::from_str(k), &value_to_js_at(v, depth + 1)?)
                    .map_err(|e| describe_js_error(&e))?;
            }
            obj.into()
        }
        Value::Function(_) | Value::NativeFunction(_) => {
            return Err("cannot pass a function value to a JS callback".to_string());
        }
    })
}

pub fn js_to_value(js: &JsValue) -> Result<Value, String> {
    js_to_value_at(js, 0)
}

fn js_to_value_at(js: &JsValue, depth: usize) -> Result<Value, String> {
    if depth > MAX_CONVERSION_DEPTH {
        return Err(too_deep());
    }
    if js.is_null() || js.is_undefined() {
        Ok(Value::Null)
    } else if let Some(b) = js.as_bool() {
        Ok(Value::Bool(b))
    } else if let Some(n) = js.as_f64() {
        // JS only has one number type, unlike nib's Int/Float split - treat
        // whole numbers in i64 range as Int, everything else as Float.
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            Ok(Value::Int(n as i64))
        } else {
            Ok(Value::Float(n))
        }
    } else if let Some(s) = js.as_string() {
        Ok(Value::Str(s))
    } else if Array::is_array(js) {
        Array::from(js)
            .iter()
            .map(|item| js_to_value_at(&item, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::array)
    } else if js.is_object() {
        let obj = Object::from(js.clone());
        Object::keys(&obj)
            .iter()
            .map(|key| {
                let value = Reflect::get(&obj, &key).map_err(|e| describe_js_error(&e))?;
                let key = key.as_string().ok_or("expected string object key")?;
                Ok((key, js_to_value_at(&value, depth + 1)?))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::map)
    } else {
        Err("unsupported JS value".to_string())
    }
}

#[cfg(test)]
#[path = "helpers_tests.rs"]
mod tests;
