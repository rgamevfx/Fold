//! Output selection and dependency content identity over immutable snapshots.
use fold_media::VideoInfo;
use fold_project::{DocumentRef, EditBatch, Mutation, Snapshot};
/// Hash authoring bytes, ordered asset identities, and implementation versions.
/// Unrelated project revisions and undo/redo do not invalidate viewer textures.
pub fn content(snapshot: &Snapshot) -> Result<String, String> {
    let (reference, _) = output(snapshot)?;
    content_for(snapshot, reference.document)
}

pub fn content_for(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
) -> Result<String, String> {
    let mut data =
        b"fold-video-evaluator-v7;analytic-vectors-all-spaces;gpu-operators-v1;rgba32f;ocio-2.4.2;aces-srgb-view-v1;native709-float;nearest;".to_vec();
    // Persistent playback entries must not survive evaluator implementation
    // changes. The build fingerprint is precomputed, never executable-file I/O.
    data.extend_from_slice(env!("FOLD_SOURCE_SHA256").as_bytes());
    if let Some(color) = fold_platform::color::project(snapshot)? {
        data.extend(serde_json::to_vec(&color).map_err(|e| e.to_string())?);
    }
    let mut pending = vec![document];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let document = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("missing nested document")?;
        let packages = crate::packages::builtins();
        let identity = if packages.supports(document) {
            packages.evaluation_identity(document)?
        } else {
            document.payload.clone()
        };
        for bytes in [
            document.package_id.as_bytes(),
            document.type_id.as_bytes(),
            &identity,
        ] {
            data.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        data.extend_from_slice(&document.schema_version.to_le_bytes());
        let references = serde_json::to_vec(&document.dependencies).map_err(|e| e.to_string())?;
        data.extend_from_slice(&(references.len() as u64).to_le_bytes());
        data.extend_from_slice(&references);
        data.extend_from_slice(&(document.assets.len() as u64).to_le_bytes());
        for id in &document.assets {
            data.extend_from_slice(&serde_json::to_vec(id).map_err(|e| e.to_string())?);
            let asset = snapshot
                .state()
                .assets
                .get(id)
                .ok_or("missing video asset")?;
            if let Some(space) = fold_platform::color::input(&asset.extensions)? {
                data.extend_from_slice(&(space.len() as u64).to_le_bytes());
                data.extend_from_slice(space.as_bytes());
            }
            for bytes in [asset.location.as_bytes(), asset.fingerprint.as_bytes()] {
                data.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                data.extend_from_slice(bytes);
            }
        }
        let mut dependencies: Vec<_> = document.dependencies.iter().map(|r| r.document).collect();
        dependencies.sort();
        dependencies.dedup();
        pending.extend(dependencies);
    }
    Ok(fold_media::content_hash(&data))
}

const OUTPUT_SETTING: &str = "fold.output";

/// Persist an explicit output choice as one undoable project setting change.
pub fn select_output(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
) -> Result<EditBatch, String> {
    let reference = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    output_info(snapshot, &reference)?;
    let mut settings = snapshot.state().settings.clone();
    settings.insert(
        OUTPUT_SETTING.into(),
        serde_json::to_value(reference).map_err(|e| e.to_string())?,
    );
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::SetSettings(settings)],
    })
}

fn output_info(snapshot: &Snapshot, reference: &DocumentRef) -> Result<VideoInfo, String> {
    if reference.output != "video" {
        return Err("unsupported project output port".into());
    }
    crate::packages::builtins().output(snapshot, reference.document)
}

pub fn output(snapshot: &Snapshot) -> Result<(DocumentRef, VideoInfo), String> {
    let reference = reference(snapshot)?;
    let info = output_info(snapshot, &reference)?;
    Ok((reference, info))
}

pub(crate) fn reference(snapshot: &Snapshot) -> Result<DocumentRef, String> {
    if let Some(value) = snapshot.state().settings.get(OUTPUT_SETTING) {
        let reference: DocumentRef = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid project output: {e}"))?;
        if reference.output != "video" {
            return Err("unsupported project output port".into());
        }
        return Ok(reference);
    }
    let packages = crate::packages::builtins();
    let document = [
        fold_timeline::SEQUENCE,
        fold_composition::COMPOSITION,
        fold_compositor::COMPOSITE,
    ]
    .iter()
    .find_map(|kind| {
        snapshot
            .state()
            .documents
            .values()
            .find(|d| d.type_id == *kind && packages.supports(d))
    })
    .ok_or("no supported video document")?;
    Ok(DocumentRef {
        document: document.id,
        output: "video".into(),
        extensions: Default::default(),
    })
}
