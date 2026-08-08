use ext_php_rs::convert::IntoZval;
use ext_php_rs::error::Error as PhpRsError;
use ext_php_rs::types::{ZendHashTable, Zval};
use nib_core::Value;

pub fn usize_option(options: &ZendHashTable, key: &str) -> Result<Option<usize>, String> {
    match options.get(key) {
        None => Ok(None),
        Some(zval) => zval
            .long()
            .filter(|n| *n >= 0)
            .map(|n| n as usize)
            .map(Some)
            .ok_or_else(|| format!("'{}' must be a non-negative int", key)),
    }
}

/// `ZendCallable::try_call`'s `Error::Exception` variant carries the raw
/// thrown object; its `Display` dumps the whole thing Zval-by-Zval (message,
/// trace, file, line, ...), which is both unreadable and has been observed to
/// embed a NUL byte that then fails a *second*, unrelated conversion when
/// ext-php-rs turns our returned `String` into a thrown PHP exception -
/// surfacing as a "contains NUL-bytes" error that buries the real one. Pull
/// out just `getMessage()` instead, which is what a caller actually wants.
pub fn describe_call_error(err: PhpRsError) -> String {
    match err {
        PhpRsError::Exception(exception) => exception
            .try_call_method("getMessage", vec![])
            .ok()
            .and_then(|message| message.string())
            .unwrap_or_else(|| "PHP callback threw an exception".to_string()),
        other => other.to_string(),
    }
}

/// Both converters walk nested structures recursively. `Zval::array()`
/// dereferences, so a self-referential PHP array (`$a["self"] = &$a`) recurses
/// forever, and a merely deep one overflows the native stack - neither is
/// something a host can defend against at the call site. 128 is far above any
/// real payload.
const MAX_CONVERSION_DEPTH: usize = 128;

fn too_deep() -> String {
    format!(
        "value nested deeper than {} levels (recursive array?)",
        MAX_CONVERSION_DEPTH
    )
}

pub fn value_to_zval(value: &Value) -> Result<Zval, String> {
    value_to_zval_at(value, 0)
}

fn value_to_zval_at(value: &Value, depth: usize) -> Result<Zval, String> {
    if depth > MAX_CONVERSION_DEPTH {
        return Err(too_deep());
    }
    let zval = match value {
        Value::Int(i) => (*i).into_zval(false).map_err(|e| e.to_string())?,
        Value::Float(f) => (*f).into_zval(false).map_err(|e| e.to_string())?,
        Value::Str(s) => s.clone().into_zval(false).map_err(|e| e.to_string())?,
        Value::Bool(b) => (*b).into_zval(false).map_err(|e| e.to_string())?,
        Value::Null => Zval::null(),
        Value::Array(items) => items
            .iter()
            .map(|v| value_to_zval_at(v, depth + 1))
            .collect::<Result<Vec<_>, _>>()?
            .into_zval(false)
            .map_err(|e| e.to_string())?,
        Value::Map(pairs) => {
            let mut ht = ZendHashTable::new();
            for (k, v) in pairs.iter() {
                ht.insert(k.as_str(), value_to_zval_at(v, depth + 1)?)
                    .map_err(|e| e.to_string())?;
            }
            ht.into_zval(false).map_err(|e| e.to_string())?
        }
        Value::Function(_) | Value::NativeFunction(_) => {
            return Err("cannot pass a function value to a PHP callback".to_string());
        }
    };
    Ok(zval)
}

pub fn zval_to_value(zval: &Zval) -> Result<Value, String> {
    zval_to_value_at(zval, 0)
}

fn zval_to_value_at(zval: &Zval, depth: usize) -> Result<Value, String> {
    if depth > MAX_CONVERSION_DEPTH {
        return Err(too_deep());
    }
    if let Some(i) = zval.long() {
        Ok(Value::Int(i))
    } else if let Some(f) = zval.double() {
        Ok(Value::Float(f))
    } else if let Some(b) = zval.bool() {
        Ok(Value::Bool(b))
    } else if let Some(s) = zval.string() {
        Ok(Value::Str(s))
    } else if zval.is_null() {
        Ok(Value::Null)
    } else if let Some(arr) = zval.array() {
        if arr.has_sequential_keys() {
            arr.values()
                .map(|v| zval_to_value_at(v, depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::array)
        } else {
            arr.iter()
                .map(|(k, v)| zval_to_value_at(v, depth + 1).map(|v| (k.to_string(), v)))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::map)
        }
    } else {
        Err("unsupported PHP value returned from callback".to_string())
    }
}
