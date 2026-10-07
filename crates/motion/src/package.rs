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
pub const CHANGE: &str = "fold.motion.change.v1";
const BUILD: &str = "motion-schema7-evaluator9-layer-context1-solver1-noto-b85c38ec";
pub const PANEL: &str = "fold.motion.editor";
pub const INSPECTOR: &str = "fold.motion.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: PANEL,
        title: "Composition",
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
    let mut mutations = vec![];
    if let Some(previous) = snapshot.state().documents.get(&args.document) {
        let before = Motion::from_document(previous)?;
        crate::layer_edits::append(
            snapshot,
            args.document,
            before.scene.as_ref(),
            &args.motion,
            &mut mutations,
        )?;
        document.extensions = previous.extensions.clone();
    }
    mutations.push(Mutation::PutDocument(document));
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations,
    })
}
struct Documents(Arc<prepared::Cache>);
impl DocumentProvider for Documents {
    fn image_inputs(
        &self,
        document: &Document,
    ) -> Result<Vec<fold_platform::layer_graph::ImageInput>, String> {
        crate::inputs::describe(&self.0.get(document)?.motion)
    }
    fn bind_image_input(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        port: &str,
        source: Option<fold_platform::image_source::Source>,
        info: fold_media::VideoInfo,
    ) -> Result<Document, String> {
        let _ = snapshot;
        crate::inputs::bind(document, port, source, info)
    }

    fn authoring_catalog(&self) -> serde_json::Value {
        crate::automation::catalog()
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        crate::automation::inspect(document)
    }
    fn query(
        &self,
        document: &Document,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        crate::automation::discovery::query(document, query)
    }
    fn query_cancellable(
        &self,
        document: &fold_project::Document,
        query: &serde_json::Value,
        cancel: &fold_media::Cancel,
    ) -> Result<serde_json::Value, String> {
        crate::automation::discovery::query_cancellable(document, query, cancel)
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        crate::automation::author(document, edits)
    }

    fn author_cancellable(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
        cancel: &Cancel,
    ) -> Result<Document, String> {
        crate::automation::author_cancellable(document, edits, cancel)
    }
    fn authoring_receipt(
        &self,
        before: &Document,
        after: &Document,
    ) -> Result<serde_json::Value, String> {
        crate::automation::discovery::receipt(before, after)
    }
    fn create(&self, id: fold_foundation::DocumentId) -> Result<Document, String> {
        crate::document::Motion::new_scene().document(id)
    }
    fn place(
        &self,
        snapshot: &fold_project::Snapshot,
        placement: &fold_platform::browser::Placement,
        info: Option<fold_media::VideoInfo>,
    ) -> Result<Document, String> {
        crate::nodes::image::place(snapshot, placement, info)
    }
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        MOTION
    }
    fn schema(&self) -> u32 {
        7
    }
    fn supports_schema(&self, schema: u32) -> bool {
        [1, 2, 3, 4, 5, 6, 7].contains(&schema)
    }
    fn validate(&self, d: &Document) -> Result<(), String> {
        self.0.get(d).map(|_| ())
    }
    fn evaluation_identity(&self, d: &Document) -> Result<Vec<u8>, String> {
        Ok(self.0.get(d)?.identity.as_ref().clone())
    }
}
#[derive(Default)]
pub struct Provider(Arc<prepared::Cache>, Arc<crate::simulation::Runtime>);
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
    fn supports_graph_context(&self, kind: &str) -> bool {
        kind == fold_composition::graph::CONTEXT
    }
    fn supports_controls(&self) -> bool {
        true
    }
    fn compile(&self, request: fold_platform::VideoCompile<'_>) -> Result<RenderGraph, String> {
        let image_request = |source: &fold_platform::image_source::Request| {
            fold_platform::image_source::compile(source, &request)
        };
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
        crate::nodes::composition::refresh_metadata(&mut motion, request.describe_video)?;
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
        let scene_output = reference
            .extensions
            .get(fold_composition::graph::OBJECT)
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|e| format!("Invalid object output: {e}"))
            })
            .transpose()?;
        if let Some(id) = scene_output {
            if !motion
                .scene
                .as_ref()
                .is_some_and(|scene| scene.objects.iter().any(|object| object.id == id))
            {
                return Err("Missing object output".into());
            }
        }
        let layer_times = if let Some(value) = request.graph_context {
            let context: fold_composition::graph::Context =
                serde_json::from_value(value.data.clone()).map_err(|e| e.to_string())?;
            if scene_output != Some(context.selected) {
                return Err("Mismatched layer graph context".into());
            }
            crate::evaluation::layers::bind(motion.to_mut(), &context)?
        } else {
            Default::default()
        };
        let mut evaluator = crate::evaluation::Evaluator::for_graph(&motion, &motion.graph, time)?;
        evaluator.layer_times = Arc::new(layer_times);
        evaluator.scene_output = scene_output;
        evaluator.runtime = self.1.clone();
        if !reference.extensions.contains_key("fold.controls")
            && matches!(motion, std::borrow::Cow::Borrowed(_))
        {
            evaluator.identity = Some(prepared.identity.clone());
        }
        crate::evaluation::compile_evaluator_images(
            evaluator,
            dimensions[0],
            dimensions[1],
            cancel,
            &image_request,
        )
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
        commands: vec![
            CommandRegistration {
                id: crate::authoring::composition::NEST,
                title: "Precompose layers",
                execution: Execution::Immediate,
                handler: crate::authoring::composition::stage,
            },
            CommandRegistration {
                id: EDIT,
                title: "Edit motion",
                execution: Execution::Immediate,
                handler: edit,
            },
            CommandRegistration {
                id: CHANGE,
                title: "Edit motion",
                execution: Execution::Immediate,
                handler: crate::edits::stage,
            },
        ],
        video: vec![
            Box::new(Provider(
                prepared,
                Arc::new(crate::simulation::Runtime::default()),
            )),
            Box::new(crate::SolidProvider),
        ],
        audio: vec![],
    })
}
