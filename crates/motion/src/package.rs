//! Static motion contributions shared by desktop and CLI.
use crate::{MOTION, Motion};
use fold_foundation::{DocumentId, Time};
use fold_media::{Cancel, VideoInfo};
use fold_platform::{VideoProvider, packages::*};
use fold_project::{Document, EditBatch, Mutation, Snapshot};
use fold_render::RenderGraph;
use serde::{Deserialize, Serialize};
pub const EDIT: &str = "fold.motion.edit";
const BUILD: &str = "motion-schema1-evaluator1-vector-linear8-noto-b85c38ec";
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
struct Documents;
impl DocumentProvider for Documents {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        MOTION
    }
    fn schema(&self) -> u32 {
        1
    }
    fn validate(&self, d: &Document) -> Result<(), String> {
        Motion::from_document(d).map(|_| ())
    }
    fn video_info(&self, d: &Document) -> Result<VideoInfo, String> {
        Ok(Motion::from_document(d)?.info)
    }
    fn evaluation_identity(&self, d: &Document) -> Result<Vec<u8>, String> {
        let mut m = Motion::from_document(d)?;
        for n in &mut m.graph.nodes {
            n.position = [0.; 2];
        }
        m.graph.nodes.sort_by_key(|n| n.id);
        for group in m.groups.values_mut() {
            for n in &mut group.graph.nodes {
                n.position = [0.; 2];
            }
            group.graph.nodes.sort_by_key(|n| n.id);
        }
        let mut identity = BUILD.as_bytes().to_vec();
        identity.extend(serde_json::to_vec(&m).map_err(|e| e.to_string())?);
        Ok(identity)
    }
}
pub struct Provider;
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        MOTION
    }
    fn compile_reference(
        &self,
        _snapshot: &Snapshot,
        d: &Document,
        reference: &fold_project::DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        _resolve: &fold_platform::VideoResolver<'_>,
    ) -> Result<RenderGraph, String> {
        if reference.output != "video" {
            return Err("unsupported motion output".into());
        }
        let mut motion = Motion::from_document(d)?;
        if let Some(overrides) = reference.extensions.get("fold.controls") {
            let overrides: std::collections::BTreeMap<
                fold_foundation::ObjectId,
                crate::fields::Datum,
            > = serde_json::from_value(overrides.clone())
                .map_err(|e| format!("invalid motion control overrides: {e}"))?;
            for (id, value) in overrides {
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
                motion.controls.insert(id, value);
            }
        }
        crate::evaluation::compile(&motion, time, dimensions[0], dimensions[1])
    }
    fn compile(
        &self,
        d: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        if output != "video" {
            return Err("unsupported motion output".into());
        }
        crate::evaluation::compile(&Motion::from_document(d)?, time, width, height)
    }
}
pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    registry.register(Contributions {
        manifest: Manifest {
            id: crate::PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: PANELS,
            build: BUILD,
        },
        documents: vec![Box::new(Documents)],
        commands: vec![CommandRegistration {
            id: EDIT,
            title: "Edit motion",
            execution: Execution::Immediate,
            handler: edit,
        }],
        video: vec![Box::new(Provider)],
        audio: vec![],
    })
}
