//! Versioned local authoring contract. The session remains the only live writer.
mod batch;
pub mod cli;
pub mod client;
pub(crate) mod host;
pub(crate) mod images;
pub mod mcp;
mod schema;
pub mod wire;
pub use batch::{Batch, stage};
use fold_platform::authoring::decode;
use fold_project::CommittedSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub const VERSION: u32 = 1;
pub const MAX_REQUEST: usize = 1024 * 1024;
pub const MAX_RESPONSE: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}
pub fn error(message: impl ToString) -> Value {
    let message = message.to_string();
    let mut result = json!({"ok":false,"error":message});
    if let Some(start) = message.find('{') {
        if let Ok(detail) = serde_json::from_str::<Value>(&message[start..]) {
            if detail["kind"] == "rust_compile" {
                result["diagnostics"] = detail["diagnostics"].clone();
                result["kind"] = detail["kind"].clone();
            }
        }
    }
    result
}
pub fn success(value: Value) -> Value {
    json!({"ok":true,"result":value})
}
pub fn required<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, String> {
    decode(v.get(key).ok_or_else(|| format!("missing {key}"))?)
}
pub fn catalog(params: &Value) -> Result<Value, String> {
    if let Some(method) = params.get("method").and_then(Value::as_str) {
        let tool = schema::methods()
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == method)
            .cloned()
            .ok_or("unknown method")?;
        return Ok(tool);
    }
    let registry = crate::packages::builtins();
    let catalogs = registry.authoring_catalog();
    if let Some(kind) = params.get("type").and_then(Value::as_str) {
        let mut catalog = catalogs
            .into_iter()
            .find(|c| c["type"] == kind)
            .ok_or("unknown authoring type")?;
        if let Some(query) = params.get("query").and_then(Value::as_str)
            && let Some(nodes) = catalog["catalog"]["nodes"].as_array_mut()
        {
            let query = query.to_lowercase();
            nodes.retain(|n| n.to_string().to_lowercase().contains(&query));
        }
        if params.get("nodes") == Some(&Value::Bool(false)) {
            catalog["catalog"].as_object_mut().unwrap().remove("nodes");
        }
        return Ok(catalog);
    }
    Ok(
        json!({"version":VERSION,"types":catalogs.iter().map(|c|c["type"].clone()).collect::<Vec<_>>(),
       "methods":["state","catalog","inspect","batch","undo","redo","jobs","cancel","render","viewers","capture","navigate","save","screenshot"],
       "commands":registry.commands().map(|c|json!({"id":c.id,"title":c.title})).collect::<Vec<_>>(),
       "batch":batch::examples(),"limits":{"request_bytes":MAX_REQUEST,"operations":128,"active_jobs":4,"retained_jobs":16},
       "time":{"numerator":1,"denominator":24},
       "notes":"Call catalog with method for request JSON schemas, or type for package operation examples and node signatures. inspect supports JSON pointer, offset, limit. Job acceptance is not completion: poll jobs and check ok. Image jobs return PNG base64. request_id is required for mutations; reuse it only with identical arguments. Revision conflicts require reinspection."}),
    )
}
pub fn inspect(snapshot: &CommittedSnapshot, params: &Value) -> Result<Value, String> {
    inspect_cancellable(snapshot, params, &fold_media::Cancel::default())
}
pub fn inspect_cancellable(
    snapshot: &CommittedSnapshot,
    params: &Value,
    cancel: &fold_media::Cancel,
) -> Result<Value, String> {
    let id = required(params, "document")?;
    let d = snapshot
        .state()
        .documents
        .get(&id)
        .ok_or("missing document")?;
    let value = if let Some(query) = params.get("query") {
        crate::packages::builtins().query_cancellable(d, query, cancel)?
    } else {
        crate::packages::builtins().inspect(d)?
    };
    let pointer = params.get("pointer").and_then(Value::as_str).unwrap_or("");
    let value = value
        .pointer(pointer)
        .ok_or("inspection pointer not found")?;
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .min(200) as usize;
    let (data, total) = if let Some(a) = value.as_array() {
        (
            Value::Array(a.iter().skip(offset).take(limit).cloned().collect()),
            Some(a.len()),
        )
    } else {
        (value.clone(), None)
    };
    let result = json!({"revision":snapshot.revision(),"document":id,"pointer":pointer,"offset":offset,"total":total,"data":data});
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > 256 * 1024
    {
        return Err(
            "inspection exceeds 256 KiB; select a narrower pointer or smaller limit".into(),
        );
    }
    Ok(result)
}

pub fn inspect_changes(
    before: &CommittedSnapshot,
    after: &CommittedSnapshot,
    params: &Value,
) -> Result<Value, String> {
    let id = required(params, "document")?;
    let new = after.state().documents.get(&id);
    let old = before.state().documents.get(&id);
    let changes = match (old, new) {
        (Some(a), Some(b)) => crate::packages::builtins().authoring_receipt(a, b)?,
        (None, Some(_)) => json!({"created":true}),
        (Some(_), None) => json!({"removed":true}),
        (None, None) => return Err("missing document in both revisions".into()),
    };
    let result = json!({"document":id,"since_revision":before.revision(),"revision":after.revision(),"changes":changes});
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > 256 * 1024
    {
        return Err("change summary exceeds 256 KiB; inspect narrower graph queries".into());
    }
    Ok(result)
}
