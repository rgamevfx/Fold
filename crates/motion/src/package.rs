//! Static motion contributions shared by desktop and CLI.
use crate::{MOTION, Motion};
use fold_foundation::DocumentId;
use fold_media::{Cancel, VideoInfo};
use fold_platform::{VideoProvider, packages::*};
use fold_project::{Document, EditBatch, Mutation, Snapshot};
use fold_render::RenderGraph;
use serde::{Deserialize, Serialize};
#[path = "prepared.rs"]
mod prepared;
use std::sync::Arc;
pub const EDIT: &str = "fold.motion.edit";
const BUILD: &str = "motion-schema2-evaluator2-vector-linear8-noto-b85c38ec";
pub const PANEL: &str = "fold.motion.editor";
pub const INSPECTOR: &str = "fold.motion.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: PANEL,
        title: "Motion",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: INSPECTOR,
        // Property contribution hosted inside the shared Node Inspector.
        title: "Motion node properties",
        placement: PanelPlacement::Inspector,
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditArgs {
    pub document: DocumentId,
    pub motion: Motion,
}
fn edit(snapshot: &Snapshot, args: &[u8], _: &Cancel) -> Result<EditBatch, String> {
    let args: EditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let mut document = args.motion.document(args.document)?;
    if let Some(previous) = snapshot.state().documents.get(&args.document) {
        Motion::from_document(previous)?;
        document.extensions = previous.extensions.clone();
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
struct Documents(Arc<prepared::Cache>);
impl DocumentProvider for Documents {
    fn browser_kind(&self) -> Option<fold_platform::browser::DocumentKind> {
        Some(fold_platform::browser::DocumentKind {
            type_id: crate::document::MOTION,
            title: "Motion",
        })
    }
    fn create(&self, id: fold_foundation::DocumentId) -> Result<Document, String> {
        crate::document::Motion::empty().document(id)
    }
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        MOTION
    }
    fn schema(&self) -> u32 {
        2
    }
    fn supports_schema(&self, schema: u32) -> bool {
        [1, 2].contains(&schema)
    }
    fn validate(&self, d: &Document) -> Result<(), String> {
        self.0.get(d).map(|_| ())
    }
    fn evaluation_identity(&self, d: &Document) -> Result<Vec<u8>, String> {
        Ok(self.0.get(d)?.identity.clone())
    }
}
#[derive(Default)]
pub struct Provider(Arc<prepared::Cache>);
impl VideoProvider for Provider {
    fn playback_mode(&self) -> fold_platform::desktop::PlaybackMode {
        fold_platform::desktop::PlaybackMode::EveryFrame
    }
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        MOTION
    }
    fn video_info(&self, d: &Document) -> Result<VideoInfo, String> {
        Ok(self.0.get(d)?.motion.info.clone())
    }
    fn supports_controls(&self) -> bool {
        true
    }
    fn compile(&self, request: fold_platform::VideoCompile<'_>) -> Result<RenderGraph, String> {
        let fold_platform::VideoCompile {
            document: d,
            reference,
            time,
            dimensions,
            cancel,
            ..
        } = request;
        cancel.check()?;
        if reference.output != "video" {
            return Err("unsupported motion output".into());
        }
        let prepared = self.0.get(d)?;
        let mut motion = std::borrow::Cow::Borrowed(&prepared.motion);
        if let Some(overrides) = reference.extensions.get("fold.controls") {
            let overrides: std::collections::BTreeMap<
                fold_foundation::ObjectId,
                crate::fields::Datum,
            > = serde_json::from_value(overrides.clone())
                .map_err(|e| format!("invalid motion control overrides: {e}"))?;
            for (id, value) in overrides {
                cancel.check()?;
                let settings = motion
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| n.kind == "fold.motion.published_control")
                    .map(|n| n.settings::<crate::nodes::interface::ControlSettings>())
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or("override references an unavailable published control")?;
                value.validate()?;
                if value.kind() != settings.value_type {
                    return Err("published control override type mismatch".into());
                }
                motion.to_mut().controls.insert(id, value);
            }
        }
        crate::evaluation::compile_prepared(&motion, time, dimensions[0], dimensions[1], cancel)
    }
}
pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    let prepared = Arc::new(prepared::Cache::default());
    registry.register(Contributions {
        manifest: Manifest {
            id: crate::PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: PANELS,
            build: BUILD,
        },
        documents: vec![
            Box::new(Documents(prepared.clone())),
            Box::new(crate::SolidProvider),
        ],
        commands: vec![CommandRegistration {
            id: EDIT,
            title: "Edit motion",
            execution: Execution::Immediate,
            handler: edit,
        }],
        video: vec![Box::new(Provider(prepared)), Box::new(crate::SolidProvider)],
        audio: vec![],
    })
}
