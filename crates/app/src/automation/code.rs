//! Bounded, capability-free JavaScript authoring. Only returned native operations
//! cross into the existing candidate staging/validation path.
use super::{Batch, required};
use fold_media::Cancel;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
pub const SOURCE_LIMIT: usize = 64 * 1024;
pub fn compile(
    params: &Value,
    revision: fold_project::Revision,
    epoch: u64,
    cancel: &Cancel,
) -> Result<Batch, String> {
    let source: String = required(params, "source")?;
    if source.len() > SOURCE_LIMIT {
        return Err("author source exceeds 64 KiB".into());
    }
    let handles = params.get("handles").cloned().unwrap_or_else(|| json!({}));
    let handles: std::collections::BTreeMap<String, fold_foundation::ObjectId> =
        serde_json::from_value(handles).map_err(|e| format!("handles: {e}"))?;
    if handles.len() > 512 {
        return Err("at most 512 existing handles".into());
    }
    let namespace = params["namespace"].as_str().unwrap_or("default");
    super::handles::namespace(namespace)?;
    let bindings = params.get("bindings").cloned().unwrap_or_else(|| json!({}));
    let request_id: String = required(params, "request_id")?;
    let label: String = required(params, "label")?;
    let runtime = rquickjs::Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(32 * 1024 * 1024);
    runtime.set_max_stack_size(256 * 1024);
    let start = Instant::now();
    let token = cancel.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        token.check().is_err() || start.elapsed() > Duration::from_secs(2)
    })));
    let context = rquickjs::Context::full(&runtime).map_err(|e| e.to_string())?;
    let sdk = include_str!("../../../../skills/fold/sdk/authoring.mjs").replace("export ", "");
    let script = format!(
        "'use strict'; const structuredClone=v=>JSON.parse(JSON.stringify(v)); {sdk}\nconst plan=new FoldPlan({state});\n(function(){{\n{source}\n}})();\nJSON.stringify(plan.batch({request_id},{label}));",
        state = json!({"revision":revision,"project_epoch":epoch,"handles":handles,"bindings":bindings,"namespace":namespace}),
        request_id = json!(request_id),
        label = json!(label)
    );
    cancel.check()?;
    let encoded = context.with(|ctx| {
        ctx.eval::<String, _>(script).map_err(|e| {
            let exception = ctx.catch();
            format!("author execution failed: {e}; {exception:?}")
                .chars()
                .take(4096)
                .collect::<String>()
        })
    })?;
    cancel.check()?;
    if encoded.len() > super::MAX_REQUEST {
        return Err("author output exceeds 1 MiB; split the candidate edit".into());
    }
    let mut batch: Batch =
        serde_json::from_str(&encoded).map_err(|e| format!("author output: {e}"))?;
    // Code may alter its own builder but never the host's revision/commit authority.
    batch.revision = revision;
    batch.project_epoch = epoch;
    batch.request_id = request_id;
    batch.label = label;
    batch.validate_only = false;
    Ok(batch)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(source: &str) -> Result<Batch, String> {
        compile(
            &json!({"source":source,"request_id":"code","label":"Code"}),
            fold_project::Revision(0),
            0,
            &Cancel::default(),
        )
    }
    #[test]
    fn code_emits_native_edits_without_host_authority() {
        let b =
            run("plan.document('title').object('text','fold.motion.text').set('text','Hello');")
                .unwrap();
        assert_eq!(b.operations.len(), 3);
        assert!(run("plan.document('x'); throw Error('stop');").is_err());
        assert!(run("require('fs').readFileSync('/etc/passwd');").is_err());
        assert!(run("fetch('https://example.com');").is_err());
    }
    #[test]
    fn infinite_code_is_interrupted() {
        assert!(run("for (;;) {} ").is_err());
    }
}

#[cfg(test)]
#[path = "live_tools_tests.rs"]
mod live_tools_tests;
