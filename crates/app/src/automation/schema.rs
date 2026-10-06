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
            "Discover package operations and node signatures on demand.",
            json!({"method":string,"type":string,"query":string,"nodes":{"type":"boolean"}}),
            vec![],
        ),
        (
            "inspect",
            "Inspect a document through its provider. Returns a job; poll jobs. Arrays support offset/limit and JSON pointer selection.",
            json!({"document":string,"since_revision":integer,"project_epoch":integer,"query":{"type":"object","description":"Package-owned graph query: mode nodes/upstream/downstream/handles, node, group, depth, limit, offset; handles accepts nodes."},"pointer":string,"offset":integer,"limit":{"type":"integer","minimum":1,"maximum":200}}),
            vec!["document"],
        ),
        (
            "batch",
            "Stage validated edits and publish one undo step. Discover operations with catalog. Poll returned job for commitment.",
            json!({"request_id":string,"revision":integer,"project_epoch":integer,"label":string,"ids":{"type":"array","items":string},"operations":{"type":"array","items":{"type":"object"},"minItems":1,"maxItems":128},"validate_only":{"type":"boolean"}}),
            vec![
                "request_id",
                "revision",
                "project_epoch",
                "label",
                "operations",
            ],
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
            "jobs",
            "Get job status and result. Image results are returned as image content. Completed jobs are retained within bounded capacity.",
            json!({"job":integer}),
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
            "Render a pinned committed output at an exact time without moving a viewer. Returns an image job.",
            json!({"source":reference,"time":time,"dimensions":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4096},"minItems":2,"maxItems":2}}),
            vec!["source", "time", "dimensions"],
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
    json!(specs.into_iter().map(|(name,description,properties,required)|{
        json!({"name":name,"description":description,"inputSchema":object(properties,&required),"annotations":{"readOnlyHint":matches!(name,"sessions"|"state"|"catalog"|"inspect"|"viewers"|"jobs"),"openWorldHint":false}})
    }).collect::<Vec<_>>())
}
