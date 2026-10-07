//! Shared method schemas for CLI discovery and protocol adapters.
use serde_json::{Value, json};
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn methods() -> Value {
    let string = json!({"type":"string"});
    let integer = json!({"type":"integer","minimum":0});
    let time = object(
        json!({"numerator":{"type":"integer"},"denominator":{"type":"integer","minimum":1}}),
        &["numerator", "denominator"],
    );
    let reference = object(
        json!({"document":string,"output":string}),
        &["document", "output"],
    );
    let specs = vec![
        (
            "sessions",
            "Find running Fold sessions and their project state.",
            json!({}),
            vec![],
        ),
        (
            "state",
            "Inspect live revision, documents, selection and unsaved state. Document/asset lists are paginated.",
            json!({"offset":integer,"limit":{"type":"integer","minimum":1,"maximum":200}}),
            vec![],
        ),
        (
            "catalog",
            "Call without query arguments first for exact package type IDs and workflow guidance. Then discover method schemas, package operations, configured node signatures, shipped guides, or the SDK on demand.",
            json!({"method":string,"type":string,"query":string,"nodes":{"type":"boolean"},"sdk":{"type":"boolean"},"guide":{"type":"string","enum":["workflow","motion","graph","glossary","candidates","live-tools","image-inputs","effects","rust-assets","rust-sdk"]}}),
            vec![],
        ),
        (
            "inspect",
            "Inspect a document through its provider. Returns a job; poll jobs. Arrays support offset/limit and JSON pointer selection.",
            json!({"document":string,"since_revision":integer,"project_epoch":integer,"query":{"type":"object","description":"Package-owned query. Motion modes: nodes, upstream, downstream, handles, signature (kind/settings), validate (edits), evaluated (time and optional object or node/output, fields, limit). Call catalog type for query examples."},"candidate":integer,"generation":integer,"pointer":string,"offset":integer,"limit":{"type":"integer","minimum":1,"maximum":200}}),
            vec!["document"],
        ),
        (
            "batch",
            "Stage validated edits and publish one undo step. Discover operations with catalog. Poll returned job for commitment.",
            json!({"request_id":string,"revision":integer,"project_epoch":integer,"label":string,"ids":{"type":"array","items":string},"operations":{"type":"array","items":fold_platform::authoring::inline_schema(&super::batch::operation_schema()),"minItems":1,"maxItems":128},"validate_only":{"type":"boolean"}}),
            vec![
                "request_id",
                "revision",
                "project_epoch",
                "label",
                "operations",
            ],
        ),
        (
            "author",
            "Execute bounded JavaScript SDK code against a private candidate. A prebound plan is a FoldPlan; use plan.document/existingDocument and catalog sdk=true for the reference. Use a stable namespace; saved handles reload automatically after reconnect/reopen. Constructors apply creation defaults only; setters explicitly update. Jobs return full handles and binding signatures. catalog guide=live-tools explains shipped tools and preview. The code only emits native batch operations, with no filesystem, network or process access. Any exception leaves the candidate unchanged. Poll the job. Limits: 64 KiB source, 32 MiB JS heap, 2 seconds, 1 MiB batch output.",
            json!({"request_id":string,"project_epoch":integer,"candidate":integer,"generation":integer,"label":string,"source":string,"namespace":string,"handles":{"type":"object","additionalProperties":{"type":"string"}}}),
            vec![
                "request_id",
                "project_epoch",
                "candidate",
                "generation",
                "label",
                "source",
            ],
        ),
        (
            "candidate",
            "Private edits: preview with source and time opens a separate native Agent preview viewer with Apply/Discard; it does not move artist playback. begin at live revision; edit using batch at candidate revision; inspect/render at candidate+generation+project_epoch; commit once at base revision or discard. Declare every batch {$ref:name} in batch.ids. Every mutation needs a new request_id; poll edit jobs. Retained for 15 idle minutes, at most four candidates. Conflicts require rebuilding.",
            json!({"action":{"type":"string","enum":["begin","edit","commit","discard","preview"]},"request_id":string,"project_epoch":integer,"revision":integer,"candidate":integer,"generation":integer,"label":string,"source":{"type":"object"},"time":{"type":"object"},"batch":{"type":"object"}}),
            vec!["action", "request_id", "project_epoch"],
        ),
        (
            "undo",
            "Undo one project transaction at the expected revision.",
            json!({"request_id":string,"revision":integer,"project_epoch":integer}),
            vec!["request_id", "revision", "project_epoch"],
        ),
        (
            "redo",
            "Redo one project transaction at the expected revision.",
            json!({"request_id":string,"revision":integer,"project_epoch":integer}),
            vec!["request_id", "revision", "project_epoch"],
        ),
        (
            "asset",
            "Export a portable .foldasset bundle or import it as a new Composition layer. Export requires document and definition; import requires composition, graph, instance and layer UUIDs. Includes native graphs, Rust source and embedded fonts. External scene/media dependencies must be exposed as inputs. Export never overwrites a file. Import is one undoable transaction and does not execute source. Returns a job; await with jobs wait_ms=25000.",
            json!({"action":{"type":"string","enum":["import","export"]},"path":string,"document":string,"definition":string,"composition":string,"graph":string,"instance":string,"layer":string,"request_id":string,"revision":integer,"project_epoch":integer}),
            vec!["action", "path", "request_id", "revision", "project_epoch"],
        ),
        (
            "jobs",
            "Get job status and result. With job and wait_ms (0..25000), wait until completion or deadline without blocking the app; a timeout returns pending and does not cancel work. Prefer wait_ms=25000 to tight polling. Image results are returned as image content. Completed jobs are retained within bounded capacity.",
            json!({"job":integer,"wait_ms":{"type":"integer","minimum":0,"maximum":25000}}),
            vec![],
        ),
        (
            "cancel",
            "Request cancellation of a pending job; already committed work requires undo.",
            json!({"job":integer}),
            vec!["job"],
        ),
        (
            "render",
            "Render a pinned committed output or candidate generation at an exact time without moving a viewer. Returns an image job.",
            json!({"candidate":integer,"generation":integer,"project_epoch":integer,"source":reference,"time":time,"dimensions":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4096},"minItems":2,"maxItems":2}}),
            vec!["source", "time", "dimensions"],
        ),
        (
            "observe",
            "Render 1..12 exact-time frames as a row-major strip with timestamped evaluated queries from the same pinned root. Does not move playback. Queries use the source package; unsupported measurements are explicit. At most 2000000 pixels total. Optional candidate/generation/project_epoch selects private artwork.",
            json!({"source":reference,"times":{"type":"array","items":time,"minItems":1,"maxItems":12},"dimensions":{"type":"array","items":{"type":"integer","minimum":1,"maximum":2048},"minItems":2,"maxItems":2},"queries":{"type":"array","items":{"type":"object"},"maxItems":8},"candidate":integer,"generation":integer,"project_epoch":integer}),
            vec!["source", "times", "dimensions"],
        ),
        (
            "viewers",
            "List viewer IDs, displayed/requested image identities and focused viewer.",
            json!({}),
            vec![],
        ),
        (
            "capture",
            "Capture a viewer presentation image, without chrome/overlays. Returns an image job; check metadata for stale content.",
            json!({"viewer":integer}),
            vec!["viewer"],
        ),
        (
            "navigate",
            "Explicitly seek/retarget only the named viewer and stop its playback.",
            json!({"viewer":integer,"time":time,"source":reference}),
            vec!["viewer", "time"],
        ),
        (
            "save",
            "Save a pinned project revision to an absolute path. Returns a job.",
            json!({"request_id":string,"revision":integer,"project_epoch":integer,"path":string}),
            vec!["request_id", "revision", "project_epoch", "path"],
        ),
        (
            "screenshot",
            "Request a user-selected desktop screenshot through the XDG portal. Requires portal permission; returns an image job.",
            json!({}),
            vec![],
        ),
    ];
    let mut methods=specs.into_iter().map(|(name,description,properties,required)|{
        json!({"name":name,"description":description,"inputSchema":object(properties,&required),"annotations":{"readOnlyHint":matches!(name,"sessions"|"state"|"catalog"|"inspect"|"observe"|"render"|"viewers"|"jobs"),"openWorldHint":false}})
    }).collect::<Vec<_>>();
    let batch = methods.iter().find(|m| m["name"] == "batch").unwrap()["inputSchema"].clone();
    methods
        .iter_mut()
        .find(|m| m["name"] == "candidate")
        .unwrap()["inputSchema"]["properties"]["batch"] = batch;
    json!(methods)
}

/// Validate the subset used by the public request definitions at the transport seam.
/// Package-owned payloads are decoded by their owners, not guessed here.
pub fn validate(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let kind = schema["type"].as_str().unwrap_or("");
    let matches = match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "" => true,
        _ => return Err(format!("unsupported schema type {kind}")),
    };
    if !matches {
        return Err(format!("{path}: expected {kind}"));
    }
    if let Some(choices) = schema["enum"].as_array()
        && !choices.contains(value)
    {
        return Err(format!("{path}: expected one of {}", schema["enum"]));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key}: required"));
                }
            }
        }
        for (key, v) in object {
            if let Some(def) = schema["properties"].get(key) {
                validate(def, v, &format!("{path}.{key}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(format!(
                    "{path}.{key}: unknown argument; discover supported arguments with catalog"
                ));
            }
        }
    }
    if let Some(array) = value.as_array() {
        for (key, test) in [("minItems", true), ("maxItems", false)] {
            if let Some(bound) = schema[key].as_u64()
                && ((test && array.len() < (bound as usize))
                    || (!test && array.len() > (bound as usize)))
            {
                return Err(format!("{path}: {key}={bound}"));
            }
        }
        if let Some(item) = schema.get("items") {
            for (i, v) in array.iter().enumerate() {
                validate(item, v, &format!("{path}[{i}]"))?;
            }
        }
    }
    if let Some(n) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|b| n < b)
            || schema["maximum"].as_f64().is_some_and(|b| n > b))
    {
        return Err(format!("{path}: outside supported range"));
    }
    Ok(())
}
