use js_sys::{Array, Function, Object, Reflect};
use nib_core::{Config, Nib as NibCore, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Nib {
    nib: NibCore,
}

#[wasm_bindgen]
impl Nib {
    // `options` isn't `Option<T>` because none of wasm-bindgen's numeric ABI
    // conversions (only i8/u8/i16/u16, not usize/f64) support Option - a
    // plain JsValue naturally represents an omitted argument as `undefined`
    // instead, so `new Nib()` and `new Nib({ maxCallDepth: 500 })` both work.
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<Nib, JsValue> {
        let config = parse_config(&options)?;
        Ok(Nib {
            nib: NibCore::with_config(config),
        })
    }

    pub fn parse(&mut self, source: String) -> Result<(), JsValue> {
        self.nib
            .parse(&source)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn include(&mut self, source: String) {
        self.nib.include(source);
    }

    pub fn run(&mut self) -> Result<(), JsValue> {
        self.nib
            .run()
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = registerFunc)]
    pub fn register_func(&mut self, name: String, callback: Function) {
        self.nib
            .register_func(name, move |args: &[Value]| -> Result<Value, String> {
                let js_args = Array::new();
                for arg in args {
                    js_args.push(&value_to_js(arg)?);
                }

                let result = callback
                    .apply(&JsValue::UNDEFINED, &js_args)
                    .map_err(|e| describe_js_error(&e))?;
                js_to_value(&result)
            });
    }

    #[wasm_bindgen(js_name = disableKeywords)]
    pub fn disable_keywords(&mut self, keywords: Vec<String>) {
        self.nib
            .disable_keywords(keywords.iter().map(|k| k.as_str()).collect());
    }
}

fn parse_config(options: &JsValue) -> Result<Config, JsValue> {
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

fn describe_js_error(err: &JsValue) -> String {
    err.dyn_ref::<js_sys::Error>()
        .and_then(|e| e.message().as_string())
        .or_else(|| err.as_string())
        .unwrap_or_else(|| "unknown JS error".to_string())
}

fn value_to_js(value: &Value) -> Result<JsValue, String> {
    Ok(match value {
        Value::Int(i) => JsValue::from_f64(*i as f64),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::Str(s) => JsValue::from_str(s),
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Null => JsValue::NULL,
        Value::Array(items) => {
            let arr = Array::new();
            for item in items {
                arr.push(&value_to_js(item)?);
            }
            arr.into()
        }
        Value::Map(pairs) => {
            let obj = Object::new();
            for (k, v) in pairs {
                Reflect::set(&obj, &JsValue::from_str(k), &value_to_js(v)?)
                    .map_err(|e| describe_js_error(&e))?;
            }
            obj.into()
        }
        Value::Function(_) | Value::NativeFunction(_) => {
            return Err("cannot pass a function value to a JS callback".to_string());
        }
    })
}

fn js_to_value(js: &JsValue) -> Result<Value, String> {
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
            .map(|item| js_to_value(&item))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    } else if js.is_object() {
        let obj = Object::from(js.clone());
        Object::keys(&obj)
            .iter()
            .map(|key| {
                let value = Reflect::get(&obj, &key).map_err(|e| describe_js_error(&e))?;
                let key = key.as_string().ok_or("expected string object key")?;
                Ok((key, js_to_value(&value)?))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Map)
    } else {
        Err("unsupported JS value returned from callback".to_string())
    }
}
