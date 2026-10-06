//! Versioned local authoring contract. The session remains the only live writer.
mod batch;
pub mod cli;
pub mod client;
pub(crate) mod code;
mod evidence;
pub(crate) mod host;
pub(crate) mod images;
pub mod mcp;
pub(crate) mod observation;
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
pub fn validate_request(request: &Request) -> Result<(), String> {
    if request.version != VERSION {
        return Err(format!(
            "unsupported contract version {}; expected {VERSION}",
            request.version
        ));
    }
    let methods = schema::methods();
    let method = methods
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == request.method)
        .ok_or("unknown method; call catalog")?;
    schema::validate(&method["inputSchema"], &request.params, "params")
}
pub fn catalog(params: &Value) -> Result<Value, String> {
    validate_request(&Request {
        version: VERSION,
        method: "catalog".into(),
        params: params.clone(),
    })?;
    if let Some(topic) = params.get("guide").and_then(Value::as_str) {
        if params.as_object().unwrap().len() != 1 {
            return Err("guide cannot be combined with other catalog arguments".into());
        }
        let text = match topic {
            "workflow" => include_str!("../../../../skills/fold/SKILL.md"),
            "motion" => include_str!("../../../../skills/fold/references/motion.md"),
            "graph" => include_str!("../../../../skills/fold/references/graph-authoring.md"),
            "glossary" => include_str!("../../../../skills/fold/references/glossary.md"),
            "live-tools" => include_str!("../../../../skills/fold/references/live-tools.md"),
            "candidates" => include_str!("../../../../skills/fold/references/candidates.md"),
            _ => {
                return Err(
                    "unknown guide; supported: workflow, motion, graph, glossary, candidates"
                        .into(),
                );
            }
        };
        return Ok(
            json!({"topic":topic,"build":env!("CARGO_PKG_VERSION"),"text":text,"related_topics":["workflow","live-tools","motion","graph","glossary","candidates"],"lookup":"Fetch another topic using catalog guide; repository access is unnecessary."}),
        );
    }
    if params.get("sdk") == Some(&Value::Bool(true)) {
        let mut generated =
            fold_platform::authoring::typescript("ProjectOperation", &batch::operation_schema());
        for package in crate::packages::builtins().authoring_catalog() {
            let name = package["type"]
                .as_str()
                .unwrap_or("package")
                .replace('.', "_");
            if let Some(schemas) = package["catalog"]["operation_schemas"].as_array() {
                for (index, schema) in schemas.iter().enumerate() {
                    generated.push_str(&fold_platform::authoring::typescript(
                        &format!("{name}_{index}"),
                        schema,
                    ));
                }
            }
        }

        if params.as_object().unwrap().len() != 1 {
            return Err("sdk cannot be combined with other catalog arguments".into());
        }
        return Ok(
            json!({"version":2,"status":"native candidate code authoring and client SDK","language":"JavaScript ES module with TypeScript declarations","module":include_str!("../../../../skills/fold/sdk/authoring.mjs"),"declarations":include_str!("../../../../skills/fold/sdk/authoring.d.ts"),"operation_declarations":generated,"usage":"Build a FoldPlan using candidate revision/project_epoch. Submit plan.batch(request_id,label) through candidate edit. Use author with a stable namespace for automatically persisted handles. Constructors initialize new identities only; explicit setters update existing artwork. Client-side plans receive returned bindings and namespace. Document.edit and FoldPlan.operation expose package/transaction operations from catalog. Use author for bounded in-app JavaScript with prebound plan, or run the same module in your client.","unsupported":["automatic conflict merging"]}),
        );
    }
    if params.get("sdk").is_some() {
        return Err("sdk must be true when supplied".into());
    }
    if params.get("method").is_some() && params.as_object().unwrap().len() != 1 {
        return Err("method cannot be combined with type/query/nodes".into());
    }
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
    let mut catalogs = registry.authoring_catalog();
    for catalog in &mut catalogs {
        if let Some(query) = params.get("query").and_then(Value::as_str)
            && let Some(nodes) = catalog["catalog"]["nodes"].as_array_mut()
        {
            let query = query.to_lowercase();
            nodes.retain(|n| n.to_string().to_lowercase().contains(&query));
        }
        if params.get("nodes") == Some(&Value::Bool(false))
            && let Some(c) = catalog["catalog"].as_object_mut()
        {
            c.remove("nodes");
        }
    }
    if params.get("type").is_none()
        && (params.get("query").is_some() || params.get("nodes").is_some())
    {
        return Ok(json!({"version":VERSION,"catalogs":catalogs}));
    }
    if let Some(kind) = params.get("type").and_then(Value::as_str) {
        return catalogs
            .into_iter()
            .find(|c| c["type"] == kind)
            .ok_or_else(|| "unknown authoring type".into());
    }

    Ok(
        json!({"version":VERSION,"authoring_contract":{"version":2,"status":"incremental; see unsupported capabilities","wire_version":VERSION},"types":catalogs.iter().map(|c|c["type"].clone()).collect::<Vec<_>>(),
       "methods":schema::methods().as_array().unwrap().iter().map(|m|m["name"].clone()).collect::<Vec<_>>(),
       "guides":["workflow","live-tools","motion","graph","glossary","candidates"],
       "workflow":{"guidance":"catalog guide=workflow; fetch referenced topic names through catalog guide", "discover":"catalog method for exact request schemas; catalog type for package operations","sdk":"catalog sdk=true returns the native fluent authoring SDK and generated operation declarations",
       "author":"candidate begin, then author with bounded SDK JavaScript or candidate edit with a typed batch; use the returned generation and revision for the next edit","evaluate":"inspect/render with candidate, generation and project_epoch; evidence is only valid for that generation","commit":"candidate commit with base revision, generation and label; one undo step","limits":"four candidates; 900 idle seconds; 4096 mutations and 64 MiB document payload per candidate","unsupported":["automatic rebase","TypeScript type stripping; author executes JavaScript","analytical motion verification"]},
       "commands":registry.commands().map(|c|json!({"id":c.id,"title":c.title})).collect::<Vec<_>>(),
       "batch":batch::examples(),"operation_schema":batch::operation_schema(),"limits":{"request_bytes":MAX_REQUEST,"operations":128,"active_jobs":4,"retained_jobs":16},
       "time":{"numerator":1,"denominator":24},
       "notes":"Call catalog with method for request JSON schemas, or type for package operation examples and node signatures. inspect supports JSON pointer, offset, limit. Job acceptance is not completion: poll jobs and check ok. CLI image jobs return image.data (PNG base64) and image.mimeType; MCP returns image content blocks. request_id is required for mutations; reuse it only with identical arguments. Revision conflicts require reinspection."}),
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
    let outputs: Vec<_> = crate::packages::builtins()
        .outputs(snapshot, id)
        .into_iter()
        .map(|o| match o.info {
            Ok(info) => json!({"source":o.reference,"label":o.label,"info":info,"available":true}),
            Err(error) => {
                json!({"source":o.reference,"label":o.label,"available":false,"error":error})
            }
        })
        .collect();
    let result = json!({"revision":snapshot.revision(),"document":id,"pointer":pointer,"offset":offset,"total":total,"data":data,"outputs":outputs});
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

pub mod handles;
