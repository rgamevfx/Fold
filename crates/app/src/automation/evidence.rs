//! Content and source-build identities for evidence produced on worker threads.
use fold_media::Cancel;
use fold_project::CommittedSnapshot;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(super) fn identity(snapshot: &CommittedSnapshot, cancel: &Cancel) -> Result<Value, String> {
    cancel.check()?;
    let mut documents = Vec::new();
    for (id, document) in &snapshot.state().documents {
        cancel.check()?;
        let hash = Sha256::digest(&document.payload);
        documents.push(json!({"document":id,"revision":document.revision,"payload_sha256":format!("{hash:x}"),"schema_version":document.schema_version,"type":document.type_id,"dependencies":document.dependencies,"assets":document.assets}));
    }
    Ok(
        json!({"source_sha256":env!("FOLD_SOURCE_SHA256"),"profile":env!("FOLD_BUILD_PROFILE"),"target":env!("FOLD_BUILD_TARGET"),"gpu_feature":cfg!(feature="gpu"),"desktop_feature":cfg!(feature="desktop"),"documents":documents,"assets":snapshot.state().assets.values().collect::<Vec<_>>(),"resource_scope":"document payloads include embedded fonts; external asset fingerprints are catalog records, not a new verification of file bytes","os":std::env::consts::OS,"architecture":std::env::consts::ARCH}),
    )
}
