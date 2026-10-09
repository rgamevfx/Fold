//! Shared JSON mechanics only. Creative models and validation stay in providers.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
pub fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}
pub fn encode(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}
/// Object merge retains unmentioned fields, including unknown extensions.
/// Null is a value, not an implicit deletion operation.
pub fn merge(target: &mut Value, patch: &Value) {
    if let (Some(a), Some(b)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in b {
            merge(a.entry(key).or_insert(Value::Null), value);
        }
    } else {
        *target = patch.clone();
    }
}
pub fn patch<T: Serialize + DeserializeOwned>(target: &mut T, patch: &Value) -> Result<(), String> {
    let mut value = encode(&*target)?;
    merge(&mut value, patch);
    *target = decode(&value)?;
    Ok(())
}
