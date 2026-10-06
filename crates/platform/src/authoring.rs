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

/// Catalog examples use the same explicit reference form as authoring batches.
pub fn examples(mut value: Value) -> Value {
    fn visit(v: &mut Value) {
        match v {
            Value::String(s) if s.starts_with('$') => {
                *v = serde_json::json!({"$ref": &s[1..]});
            }
            Value::Array(a) => {
                for v in a {
                    visit(v);
                }
            }
            Value::Object(o) => {
                for v in o.values_mut() {
                    visit(v);
                }
            }
            _ => {}
        }
    }
    for key in ["operations", "parameter_examples"] {
        if let Some(v) = value.get_mut(key) {
            visit(v);
        }
    }
    value
}

/// Public batches resolve explicit identity references before typed deserialization.
pub fn reference_schema(mut schema: Value) -> Value {
    fn visit(value: &mut Value) {
        match value {
            Value::Object(object)
                if object.get("format").and_then(Value::as_str) == Some("uuid") =>
            {
                let literal = value.clone();
                *value = serde_json::json!({"anyOf":[literal,{"type":"object","properties":{"$ref":{"type":"string"}},"required":["$ref"],"additionalProperties":false}]});
            }
            Value::Object(object) => {
                for value in object.values_mut() {
                    visit(value)
                }
            }
            Value::Array(array) => {
                for value in array {
                    visit(value)
                }
            }
            _ => {}
        }
    }
    visit(&mut schema);
    schema
}
/// Embed an acyclic schema without changing the root of its local references.
pub fn inline_schema(schema: &Value) -> Value {
    fn expand(value: &Value, root: &Value, stack: &mut Vec<String>) -> Value {
        if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
            if let Some(pointer) = reference.strip_prefix('#') {
                assert!(
                    !stack.iter().any(|v| v == reference),
                    "recursive schema requires its own root"
                );
                let target = root.pointer(pointer).expect("schema reference exists");
                stack.push(reference.to_owned());
                let result = expand(target, root, stack);
                stack.pop();
                return result;
            }
        }
        match value {
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "$defs" && key.as_str() != "$schema")
                    .map(|(key, value)| (key.clone(), expand(value, root, stack)))
                    .collect(),
            ),
            Value::Array(values) => {
                Value::Array(values.iter().map(|v| expand(v, root, stack)).collect())
            }
            _ => value.clone(),
        }
    }
    expand(schema, schema, &mut Vec::new())
}

/// Derive client types from the same public operation schemas as discovery.
/// Unsupported schema vocabulary is conservatively `unknown`, never invented.
pub fn typescript(name: &str, schema: &Value) -> String {
    fn ty(s: &Value, prefix: &str) -> String {
        if let Some(reference) = s["$ref"].as_str() {
            return reference
                .strip_prefix("#/$defs/")
                .map(|name| format!("{prefix}_{name}"))
                .unwrap_or_else(|| "unknown".into());
        }
        if let Some(value) = s.get("const") {
            return value.to_string();
        }
        if let Some(values) = s["enum"].as_array() {
            return values
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(" | ");
        }
        for (key, join) in [("oneOf", " | "), ("anyOf", " | "), ("allOf", " & ")] {
            if let Some(values) = s[key].as_array() {
                return values
                    .iter()
                    .map(|v| format!("({})", ty(v, prefix)))
                    .collect::<Vec<_>>()
                    .join(join);
            }
        }
        if let Some(kinds) = s["type"].as_array() {
            return kinds
                .iter()
                .map(|kind| {
                    let mut one = s.clone();
                    one["type"] = kind.clone();
                    ty(&one, prefix)
                })
                .collect::<Vec<_>>()
                .join(" | ");
        }
        match s["type"].as_str().unwrap_or("") {
            "string" => "string".into(),
            "integer" | "number" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => {
                if let Some(items) = s["prefixItems"].as_array() {
                    format!(
                        "[{}]",
                        items
                            .iter()
                            .map(|v| ty(v, prefix))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                } else {
                    format!("Array<{}>", ty(&s["items"], prefix))
                }
            }
            "object" => {
                let mut fields = vec![];
                if let Some(properties) = s["properties"].as_object() {
                    for (key, value) in properties {
                        let required = s["required"]
                            .as_array()
                            .is_some_and(|r| r.contains(&Value::String(key.clone())));
                        fields.push(format!(
                            "{}{}: {}",
                            serde_json::to_string(key).unwrap(),
                            if required { "" } else { "?" },
                            ty(value, prefix)
                        ));
                    }
                }
                let object = format!("{{ {} }}", fields.join("; "));
                if s["additionalProperties"].is_object() {
                    format!(
                        "{object} & Record<string,{}>",
                        ty(&s["additionalProperties"], prefix)
                    )
                } else {
                    object
                }
            }
            _ => {
                if s == &Value::Bool(false) {
                    "never".into()
                } else {
                    "unknown".into()
                }
            }
        }
    }
    let mut result = format!("export type {name} = {};\n", ty(schema, name));
    if let Some(defs) = schema["$defs"].as_object() {
        for (key, value) in defs {
            result.push_str(&format!(
                "export type {name}_{key} = {};\n",
                ty(value, name)
            ));
        }
    }
    result
}
