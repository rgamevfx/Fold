//! Settings transactions; configuration I/O runs through the command worker.
use fold_media::Cancel;
use fold_platform::settings::*;
use fold_project::{EditBatch, Mutation, Snapshot};
pub fn project(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: ProjectEdit = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    args.defaults.format.validate()?;
    let mut settings = snapshot.state().settings.clone();
    if let Some(path) = args.color_config {
        let mut color = fold_platform::color::project(snapshot)?
            .ok_or("Legacy projects retain their original color interpretation")?;
        if path.is_empty() {
            color.config = fold_platform::color::ProjectColor::default().config;
            settings.insert(
                fold_platform::color::PROJECT_KEY.into(),
                serde_json::to_value(color).map_err(|e| e.to_string())?,
            );
        } else {
            let batch = crate::color::external(snapshot, std::path::Path::new(&path))?;
            for mutation in batch.mutations {
                if let Mutation::SetSettings(value) = mutation {
                    settings = value;
                }
            }
        }
        // Validate authored interpretations against the candidate before committing it.
        let color: fold_platform::color::ProjectColor =
            serde_json::from_value(settings[fold_platform::color::PROJECT_KEY].clone())
                .map_err(|e| e.to_string())?;
        let config = color.load(&crate::color::runtime()?)?;
        for asset in snapshot.state().assets.values() {
            cancel.check()?;
            if let Some(space) = fold_platform::color::input(&asset.extensions)? {
                config.conversion(&space, fold_platform::color::WORKING_SPACE)?;
            }
        }
        for document in snapshot.state().documents.values() {
            cancel.check()?;
            let transform = fold_platform::color::output(snapshot, document.id)?;
            config.display(
                fold_platform::color::WORKING_SPACE,
                &transform.display_transform(),
            )?;
        }
    }
    settings.insert(
        PROJECT_KEY.into(),
        serde_json::to_value(args.defaults).map_err(|e| e.to_string())?,
    );
    cancel.check()?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::SetSettings(settings)],
    })
}
pub fn document(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: DocumentEdit = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing settings target")?;
    let document = crate::packages::builtins().set_document_settings(original, &args.settings)?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
