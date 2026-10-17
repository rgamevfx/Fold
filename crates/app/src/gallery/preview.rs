//! Bounded Gallery previews use the existing output renderer on background workers.
use super::storage::{Component, Preview, Store};
use fold_media::Cancel;
use fold_platform::gallery::{Kind, Reference};
use fold_project::{EditBatch, Mutation};
#[cfg(feature = "gpu")]
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "gpu")]
static HOST: OnceLock<Mutex<Option<fold_render::gpu::Host>>> = OnceLock::new();
#[cfg(feature = "gpu")]
pub(crate) fn set_host(host: fold_render::gpu::Host) {
    if let Ok(mut slot) = HOST.get_or_init(Default::default).lock() {
        *slot = Some(host);
    }
}
fn path(store: &Store, r: Reference) -> std::path::PathBuf {
    store
        .root
        .join("previews")
        .join(format!("{}-{}.json", r.id, r.version))
}
pub fn cached(store: &Store, r: Reference) -> Result<Option<Preview>, String> {
    let p = path(store, r);
    if !p.exists() {
        return Ok(None);
    }
    let bytes = super::storage::read_bytes(&p, 4 * 1024 * 1024)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| e.to_string())
}
pub fn get(store: &Store, c: &Component, cancel: &Cancel) -> Result<Preview, String> {
    if let Some(p) = &c.preview {
        return Ok(p.clone());
    }
    if let Some(p) = cached(store, c.reference)? {
        let mut check = c.clone();
        check.preview = Some(p.clone());
        check.validate()?;
        return Ok(p);
    }
    let mut document = c.document.clone();
    if matches!(c.kind, Kind::Effect | Kind::Transition) {
        let mut graph = fold_compositor::Composite::from_document(&document)?;
        graph.extensions.remove(fold_compositor::reusable::KEY);
        for node in &mut graph.nodes {
            let input = match node.prepare()?.config() {
                fold_compositor::OperationConfig::Input { id } => Some(id.clone()),
                _ => None,
            };
            if let Some(input) = input {
                node.set_configuration(fold_compositor::OperationConfig::Solid {
                    rgba: if input == "incoming" {
                        [0.9, 0.55, 0.06, 1.]
                    } else {
                        [0.06, 0.2, 0.8, 1.]
                    },
                })?;
            }
        }
        document = graph.document(document.id)?;
    }
    let mut project = crate::color::new_project(0);
    let mut mutations: Vec<_> = store
        .materialize(c, cancel)?
        .into_iter()
        .map(Mutation::PutAsset)
        .collect();
    mutations.push(Mutation::PutDocument(document.clone()));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations,
        })
        .map_err(|e| e.to_string())?;
    let snapshot = project.snapshot();
    let source = crate::packages::builtins().default_output(&snapshot, document.id)?;
    let info = crate::packages::builtins().output_ref(&snapshot, &source)?;
    #[cfg(feature = "gpu")]
    let mut renderer = crate::output::OutputRenderer::with_shared_host(
        HOST.get_or_init(Default::default)
            .lock()
            .map_err(|_| "Preview host unavailable")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    #[cfg(not(feature = "gpu"))]
    let mut renderer =
        crate::output::OutputRenderer::from_environment().map_err(|e| e.to_string())?;
    let duration = info.time(info.frames.saturating_sub(1))?;
    let end = if c.kind == Kind::Motion {
        duration.min(fold_foundation::Time::new(1, 1).unwrap())
    } else {
        duration
    };
    let mut frames = Vec::new();
    for i in 0..8 {
        cancel.check()?;
        let time = fold_foundation::Time::new(end.numerator() * i, end.denominator() * 7)
            .map_err(|e| e.to_string())?;
        let frame = renderer
            .evaluate(
                &snapshot,
                &crate::output::SceneRequest {
                    source: source.clone(),
                    time,
                    dimensions: [160, 90],
                    region: None,
                    preview: None,
                },
                cancel,
            )
            .map_err(|e| e.to_string())?;
        frames.push(frame.rgba().to_vec());
    }
    let preview = Preview {
        size: [160, 90],
        frames,
        fps: 8,
    };
    let p = path(store, c.reference);
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let mut file =
        tempfile::NamedTempFile::new_in(p.parent().unwrap()).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, &preview).map_err(|e| e.to_string())?;
    cancel.check()?;
    file.persist(&p).map_err(|e| e.to_string())?;
    Ok(preview)
}
