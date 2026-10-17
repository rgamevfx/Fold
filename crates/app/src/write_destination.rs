//! Worker-side Write destination picker; publishes one undoable parameter edit.
use fold_foundation::{DocumentId, ObjectId};
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    document: DocumentId,
    node: ObjectId,
    #[serde(default)]
    path: Option<std::path::PathBuf>,
}

pub fn choose(
    snapshot: &Snapshot,
    args: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    let args: Args = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let previous = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing compositor")?;
    let mut composite = fold_compositor::Composite::from_document(previous)?;
    let node = composite.node_mut(args.node)?;
    let mut config = node.prepare()?.config().clone();
    let fold_compositor::OperationConfig::Write { settings } = &mut config else {
        return Err("Select a Write node".into());
    };
    let path = match args.path {
        Some(path) => path,
        None => {
            #[cfg(feature = "desktop")]
            {
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Write destination")
                    .set_file_name(settings.file_type.default_filename())
                    .add_filter(
                        settings.file_type.label(),
                        &[settings.file_type.extension()],
                    )
                    .save_file()
                else {
                    return Ok(EditBatch {
                        base: snapshot.revision(),
                        mutations: vec![],
                    });
                };
                path
            }
            #[cfg(not(feature = "desktop"))]
            {
                return Err("A destination path is required".into());
            }
        }
    };
    cancel.check()?;
    settings.destination = path.to_string_lossy().into_owned();
    node.set_configuration(config)?;
    let mut document = composite.document(args.document)?;
    document.extensions = previous.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
