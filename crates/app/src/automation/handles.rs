//! Persistent authoring identity belongs to the project, so deleting a document
//! cannot silently forget its handles and recreate it on the next script run.
use fold_foundation::DocumentId;
use fold_project::{CommittedSnapshot, Document, Metadata};
use serde_json::{Value, json};
pub const KEY: &str = "fold.agent.bindings";
pub fn namespace(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err(
            "namespace must be 1..80 ASCII letters, digits, dot, underscore or dash".into(),
        );
    }
    Ok(())
}
fn contains(value: &Value, id: &str) -> bool {
    match value {
        Value::Object(o) => {
            o.get("id").and_then(Value::as_str) == Some(id)
                || o.contains_key(id)
                || o.values().any(|v| contains(v, id))
        }
        Value::Array(a) => a.iter().any(|v| contains(v, id)),
        _ => false,
    }
}
fn validate(document: &Document, bindings: &Value) -> Result<(), String> {
    let contents = crate::packages::builtins().inspect(document)?;
    for (key, record) in bindings.as_object().ok_or("bindings must be an object")? {
        let id = record["id"].as_str().ok_or("binding requires id")?;
        let signature = record["signature"]
            .as_str()
            .ok_or("binding requires signature")?;
        if key.is_empty() || key.len() > 80 || signature.is_empty() || signature.len() > 200 {
            return Err("invalid binding key/signature length".into());
        }
        if record["id"] != json!(document.id) && !contains(&contents, id) {
            return Err(format!(
                "saved handle {key} targets a deleted object; restore it or explicitly forget the binding"
            ));
        }
    }
    Ok(())
}
pub fn load(snapshot: &CommittedSnapshot, ns: &str) -> Result<Value, String> {
    namespace(ns)?;
    let Some(bindings) = snapshot.state().settings.get(KEY).and_then(|v| v.get(ns)) else {
        return Ok(json!({}));
    };
    let bindings = bindings.as_object().ok_or("invalid saved bindings")?;
    if bindings.len() > 512 {
        return Err("at most 512 saved handles per namespace".into());
    }
    let mut grouped =
        std::collections::BTreeMap::<DocumentId, serde_json::Map<String, Value>>::new();
    let mut result = serde_json::Map::new();
    for (key, record) in bindings {
        let document: DocumentId = serde_json::from_value(record["document"].clone())
            .map_err(|e| format!("binding {key}: {e}"))?;
        if !snapshot.state().documents.contains_key(&document) {
            return Err(format!(
                "saved handle {key} targets a deleted document; restore it or explicitly forget its bindings"
            ));
        }
        let value = json!({"id":record["id"],"signature":record["signature"]});
        grouped
            .entry(document)
            .or_default()
            .insert(key.clone(), value.clone());
        result.insert(key.clone(), value);
    }
    for (document, bindings) in grouped {
        validate(
            &snapshot.state().documents[&document],
            &Value::Object(bindings),
        )?;
    }
    Ok(Value::Object(result))
}
pub fn remember(
    snapshot: &CommittedSnapshot,
    document: DocumentId,
    ns: &str,
    bindings: Value,
) -> Result<Metadata, String> {
    namespace(ns)?;
    let doc = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("missing binding document")?;
    validate(doc, &bindings)?;
    let mut settings = snapshot.state().settings.clone();
    let all = settings
        .entry(KEY.into())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid binding registry")?;
    if !all.contains_key(ns) && all.len() >= 32 {
        return Err("at most 32 author namespaces per project".into());
    }
    let current = all
        .entry(ns)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid saved namespace")?;
    for (key, record) in bindings.as_object().unwrap() {
        let value = json!({"document":document,"id":record["id"],"signature":record["signature"]});
        if current.get(key).is_some_and(|old| old != &value) {
            return Err(format!("binding collision: {ns}:{key}"));
        }
        current.insert(key.clone(), value);
    }
    if current.len() > 512 {
        return Err("at most 512 saved handles per namespace".into());
    }
    Ok(settings)
}
/// Explicit recovery releases identity only; it never deletes artwork.
pub fn forget(snapshot: &CommittedSnapshot, ns: &str, keys: &[String]) -> Result<Metadata, String> {
    namespace(ns)?;
    if keys.is_empty() || keys.len() > 512 {
        return Err("forget requires 1..512 binding keys".into());
    }
    let mut settings = snapshot.state().settings.clone();
    let all = settings
        .get_mut(KEY)
        .and_then(Value::as_object_mut)
        .ok_or("no saved bindings")?;
    let bindings = all
        .get_mut(ns)
        .and_then(Value::as_object_mut)
        .ok_or("missing namespace")?;
    for key in keys {
        if bindings.remove(key).is_none() {
            return Err(format!("missing binding: {ns}:{key}"));
        }
    }
    if bindings.is_empty() {
        all.remove(ns);
    }
    Ok(settings)
}
