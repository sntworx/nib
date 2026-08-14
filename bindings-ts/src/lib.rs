mod helpers;

use helpers::{describe_js_error, js_to_value, parse_config, value_to_js};
use js_sys::{Array, Function};
use nib_lang::{Nib as NibCore, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const NIB_OPTIONS_TS: &'static str = r#"
export interface NibOptions {
    maxCallDepth?: number;
    maxParseDepth?: number;
    maxSteps?: number;
    maxStringLength?: number;
    maxArrayLength?: number;
    maxMapSize?: number;
    maxValueDepth?: number;
    maxValueNodes?: number;
}
"#;

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
    // `unchecked_optional_param_type` makes the generated .d.ts match that
    // at the type level too (`options?: NibOptions`), not just at runtime.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_optional_param_type = "NibOptions")] options: JsValue,
    ) -> Result<Nib, JsValue> {
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
    pub fn disable_keywords(&mut self, keywords: Vec<String>) -> Result<(), JsValue> {
        self.nib
            .disable_keywords(keywords.iter().map(|k| k.as_str()).collect())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
