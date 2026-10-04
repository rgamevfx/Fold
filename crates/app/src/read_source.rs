//! Worker-side Read source replacement, committed with imported assets as one edit.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::Deserialize;
pub const REPLACE: &str = "fold.app.read-source";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    document: DocumentId,
    node: ObjectId,
    #[serde(default)]
    path: Option<std::path::PathBuf>,
}
pub fn replace(
    snapshot: &Snapshot,
    args: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<EditBatch, String> {
    let args: Args = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let previous = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing composition")?;
    let mut composite = fold_compositor::Composite::from_document(previous)?;
    if !matches!(
        composite.node(args.node)?.parameters,
        fold_compositor::Parameters::Read { .. }
    ) {
        return Err("Select a Read node".into());
    }
    let path = match args.path {
        Some(path) => path,
        None => {
            #[cfg(feature = "desktop")]
            {
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Read image or video")
                    .add_filter("Compositing sources", &["exr", "mp4"])
                    .pick_file()
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
                return Err("A file path is required without desktop support".into());
            }
        }
    };
    let proposal = crate::ingest::import(
        snapshot,
        &crate::ingest::ImportRequest {
            base: snapshot.revision(),
            destination: Default::default(),
            paths: vec![path],
        },
        cancel,
    )?;
    let asset_id = match proposal.items.first() {
        Some(fold_project::ItemId::Asset(id)) => *id,
        _ => return Err("No source asset imported".into()),
    };
    let mut batch = proposal.into_batch()?;
    let asset = batch
        .mutations
        .iter()
        .find_map(|mutation| match mutation {
            Mutation::PutAsset(asset) if asset.id == asset_id => Some(asset),
            _ => None,
        })
        .or_else(|| snapshot.state().assets.get(&asset_id).map(AsRef::as_ref))
        .ok_or("Missing imported asset")?;
    let metadata: fold_media::ingest::SourceMetadata = serde_json::from_value(
        asset
            .extensions
            .get(fold_media::ingest::METADATA_KEY)
            .ok_or("Missing source metadata")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let source = fold_compositor::source_from_profile(asset_id, metadata.profile, &composite.info)?;
    let duration = source.info().time(source.info().frames)?;
    let node = composite.node_mut(args.node)?;
    let fold_compositor::Parameters::Read {
        source: old_source,
        source_start,
        duration: old_duration,
        ..
    } = &mut node.parameters
    else {
        unreachable!()
    };
    *old_source = source;
    *source_start = Time::ZERO;
    *old_duration = duration;
    // Per-use input color interpretation is retained deliberately.
    let mut document = composite.document(args.document)?;
    document.extensions = previous.extensions.clone();
    batch.mutations.push(Mutation::PutDocument(document));
    cancel.check()?;
    Ok(batch)
}
