//! Artist conversion uses the same immutable proposal gate as MCP.
use fold_media::Cancel;
use fold_motion_context::package::ConvertArgs;
use fold_platform::proposals::{DocumentInspection, Scope, ScopedRequest};
use fold_project::{EditBatch, Mutation, Snapshot};
pub(crate) fn convert(
    base: &Snapshot,
    arguments: &[u8],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    let args: ConvertArgs = serde_json::from_slice(arguments).map_err(|error| error.to_string())?;
    prepare(
        base,
        args.document,
        serde_json::json!({"op":"capture","document":args.document,"object":args.object,"time":args.time}),
        cancel,
    )
}
pub(crate) fn animate(
    base: &Snapshot,
    arguments: &[u8],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    let args: fold_motion_context::package::AnimateArgs =
        serde_json::from_slice(arguments).map_err(|e| e.to_string())?;
    prepare(
        base,
        args.document,
        serde_json::json!({"op":"animate","document":args.document,"object":args.object,"spec":args.spec}),
        cancel,
    )
}
fn prepare(
    base: &Snapshot,
    document: fold_foundation::DocumentId,
    mut body: serde_json::Value,
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    let original = base
        .state()
        .documents
        .get(&document)
        .ok_or("Missing Motion document")?;
    let packages = crate::packages::builtins();
    let capability = packages
        .scoped_proposals(original)
        .map_err(|error| error.message)?;
    let scope = Scope { document };
    let DocumentInspection::Native(expected) = capability
        .inspect(original, scope, cancel)
        .map_err(|error| error.message)?
    else {
        return Err("Native Motion inspection required".into());
    };
    body["expected"] = expected;
    let prepared = capability
        .prepare(
            base,
            &ScopedRequest {
                scope,
                version: 1,
                body,
            },
            cancel,
        )
        .map_err(|error| error.message)?;
    let changes = prepared
        .publication(base, cancel)
        .map_err(|error| error.message)?;
    Ok(EditBatch {
        base: changes.base,
        mutations: changes
            .documents
            .into_iter()
            .map(Mutation::PutDocument)
            .collect(),
    })
}
