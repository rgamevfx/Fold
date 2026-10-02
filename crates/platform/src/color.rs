//! Feature-neutral project and published-output color metadata.
pub use fold_color::authored::{from_picker, to_picker};
pub use fold_color::settings::*;
pub use fold_color::{DisplayTransform, WORKING_SPACE};
use fold_foundation::DocumentId;
use fold_project::Snapshot;

pub fn project(snapshot: &Snapshot) -> Result<Option<ProjectColor>, String> {
    snapshot
        .state()
        .settings
        .get(PROJECT_KEY)
        .map(|value| {
            let color: ProjectColor = serde_json::from_value(value.clone())
                .map_err(|e| format!("Invalid project color settings: {e}"))?;
            color.validate()?;
            Ok(color)
        })
        .transpose()
}
pub fn output(snapshot: &Snapshot, document: DocumentId) -> Result<OutputTransform, String> {
    let doc = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or("Missing color output document")?;
    doc.extensions
        .get(OUTPUT_KEY)
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|e| format!("Invalid output transform: {e}"))
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}
pub fn input(metadata: &fold_project::Metadata) -> Result<Option<String>, String> {
    metadata
        .get(INPUT_KEY)
        .map(|value| {
            value
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "Invalid input color space".into())
        })
        .transpose()
}
