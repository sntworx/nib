mod arity;
mod helpers;

use arity::reflect_arity;
use ext_php_rs::convert::IntoZvalDyn;
use ext_php_rs::prelude::*;
use ext_php_rs::types::{ZendCallable, ZendHashTable, Zval};
use helpers::{describe_call_error, usize_option, value_to_zval, zval_to_value};
use nib_lang::{Config, Nib as NibCore, Value};

#[php_class]
pub struct Nib {
    nib: NibCore,
}

#[php_impl]
impl Nib {
    pub fn __construct(options: Option<&ZendHashTable>) -> PhpResult<Self> {
        let mut config = Config::default();
        if let Some(options) = options {
            if let Some(v) = usize_option(options, "maxCallDepth")? {
                config.max_call_depth = v;
            }
            if let Some(v) = usize_option(options, "maxParseDepth")? {
                config.max_parse_depth = v;
            }
            if let Some(v) = usize_option(options, "maxSteps")? {
                config.max_steps = v;
            }
            if let Some(v) = usize_option(options, "maxStringLength")? {
                config.max_string_length = v;
            }
            if let Some(v) = usize_option(options, "maxArrayLength")? {
                config.max_array_length = v;
            }
            if let Some(v) = usize_option(options, "maxMapSize")? {
                config.max_map_size = v;
            }
            if let Some(v) = usize_option(options, "maxValueDepth")? {
                config.max_value_depth = v;
            }
            if let Some(v) = usize_option(options, "maxValueNodes")? {
                config.max_value_nodes = v;
            }
        }
        Ok(Nib {
            nib: NibCore::with_config(config),
        })
    }

    pub fn parse(&mut self, source: String) -> PhpResult<()> {
        self.nib.parse(&source).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn include(&mut self, source: String) {
        self.nib.include(source);
    }

    pub fn run(&mut self) -> PhpResult<()> {
        self.nib.run().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn disable_keywords(&mut self, keywords: Vec<String>) -> PhpResult<()> {
        self.nib
            .disable_keywords(keywords.iter().map(|k| k.as_str()).collect())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn register_func(&mut self, name: String, callback: &Zval) -> PhpResult<()> {
        // Reflect the callback once, up front, so calls from nib scripts with
        // the wrong arity are rejected the same way nib_lang already rejects
        // wrong-arity calls to its own user-defined functions.
        let arity = reflect_arity(callback).map_err(|e| e.to_string())?;

        // Clone (refcount bump, not a deep copy) so the callable outlives this call.
        let callable =
            ZendCallable::new_owned(callback.shallow_clone()).map_err(|e| e.to_string())?;

        let display_name = name.clone();
        self.nib
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

    pub fn register_var(&mut self, name: String, value: &Zval) -> PhpResult<()> {
        let value = zval_to_value(value)?;
        self.nib.register_var(name, value);
        Ok(())
    }
}

#[php_module]
pub fn module(module: ModuleBuilder) -> ModuleBuilder {
    module.name("nib").class::<Nib>()
}
