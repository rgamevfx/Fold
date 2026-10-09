//! Worker-side project-copy publication. The live project and editor file route
//! are untouched; successful publication cannot be relabelled as cancellation.
use super::{Failure, workers};
use fold_project::{CommittedSnapshot, SaveError};
use serde_json::json;

pub(super) fn validate_path(path: &str) -> Result<(), Failure> {
    let parsed = std::path::Path::new(path);
    if path.len() > 4096
        || path.contains('\0')
        || !parsed.is_absolute()
        || parsed.file_name().is_none()
    {
        return Err(Failure::new(
            "unsupported",
            "Project copy requires an absolute file path of at most 4096 UTF-8 bytes",
        ));
    }
    Ok(())
}
pub(super) fn save(
    snapshot: &CommittedSnapshot,
    path: &str,
    cancel: &fold_media::Cancel,
) -> Result<workers::Output, Failure> {
    let warning = match fold_project::save_new(snapshot, path, &|| Ok(cancel.check()?)) {
        Ok(()) => None,
        Err(SaveError::DirectorySync(error)) => Some(error.to_string()),
        Err(SaveError::Cancelled(reason)) => return Err(Failure::new("cancelled", reason)),
        Err(SaveError::AlreadyExists) => {
            return Err(Failure::new(
                "conflict",
                "Destination already exists; no file was replaced",
            ));
        }
        Err(error) => return Err(Failure::new("io", error.to_string())),
    };
    Ok(workers::Output::Published {
        revision: snapshot.revision(),
        value: json!({"state":"saved","path":path,"revision":snapshot.revision(),"durable":warning.is_none(),"warning":warning,"session_file_changed":false}),
    })
}
