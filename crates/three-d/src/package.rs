//! Spatial documents and image outputs use the same immutable project snapshot.
use crate::{
    PACKAGE, SCHEMA, TYPE,
    editing::{self, Edit},
    model::Scene,
};
use fold_foundation::DocumentId;
use fold_media::Cancel;
use fold_platform::{
    VideoCompile, VideoInfo, VideoProvider, browser,
    packages::{
        Animation, Authoring, BrowserKind, CommandRegistration, Contributions, Creation,
        DocumentProvider, Execution, HOST_API, Manifest, PackageRegistry, PanelDescriptor,
        PanelPlacement, Parameters,
    },
};
use fold_project::{Document, EditBatch, Snapshot};
use fold_render::{ImageOp, RenderGraph};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct Provider;
pub const EDIT: &str = "fold.3d.edit";
pub const PANEL: &str = "fold.3d.editor";
pub const INSPECTOR: &str = "fold.3d.inspector";
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub edits: Vec<Edit>,
}
impl DocumentProvider for Provider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn schema(&self) -> u32 {
        SCHEMA
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Scene::from_document(document).map(|_| ())
    }
    fn creation_capability(&self) -> Option<&dyn Creation> {
        Some(self)
    }
    fn scoped_proposals_capability(
        &self,
    ) -> Option<&dyn fold_platform::proposals::ScopedProposals> {
        Some(&crate::proposals::CAPABILITY)
    }
    fn authoring_capability(&self) -> Option<&dyn Authoring> {
        Some(self)
    }
    fn parameters_capability(&self) -> Option<&dyn Parameters> {
        Some(self)
    }
    fn animation_capability(&self) -> Option<&dyn Animation> {
        Some(self)
    }
    fn browser_kind_capability(&self) -> Option<&dyn BrowserKind> {
        Some(self)
    }
}
impl Authoring for Provider {
    fn authoring_catalog(&self) -> serde_json::Value {
        serde_json::json!({ "schema": SCHEMA,"edits": schemars::schema_for!(Edit),
            "parameter_bindings": schemars::schema_for!(fold_graph::binding::Binding),
            "coordinates": "Meters; local transforms composed through group hierarchy",
            "time": "Exact rational seconds; literal or native keyframe track",
            "color": "Straight scene-linear Rec.709",
            "guards": "Expected values come from inspect. Preserve IDs, protection and extension data." })
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        serde_json::to_value(Scene::from_document(document)?).map_err(|e| e.to_string())
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        self.author_cancellable(document, edits, &Cancel::default())
    }
    fn author_cancellable(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
        cancel: &Cancel,
    ) -> Result<Document, String> {
        cancel.check().map_err(|e| e.to_string())?;
        let edits = edits
            .iter()
            .map(|edit| serde_json::from_value(edit.clone()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<Edit>, _>>()?;
        let before = Scene::from_document(document)?;
        let after = editing::apply(&before, &edits, cancel)?;
        let mut result = after.document(document.id)?;
        result.revision = document.revision;
        result.extensions = document.extensions.clone();
        cancel.check().map_err(|e| e.to_string())?;
        Ok(result)
    }
}
pub fn edit(snapshot: &Snapshot, arguments: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check().map_err(|e| e.to_string())?;
    let arguments: EditArgs = serde_json::from_slice(arguments).map_err(|e| e.to_string())?;
    editing::stage(snapshot, arguments.document, &arguments.edits, cancel)
}
/// Application assembly installs this package once at context cutover.
pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    registry.register(Contributions {
        manifest: Manifest {
            id: PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: &[
                PanelDescriptor {
                    id: PANEL,
                    title: "3D",
                    placement: PanelPlacement::Editor,
                },
                PanelDescriptor {
                    id: INSPECTOR,
                    title: "3D",
                    placement: PanelPlacement::Inspector,
                },
            ],
            build: "3d-context-1",
        },
        documents: vec![Box::new(Provider)],
        video: vec![Box::new(Provider)],
        audio: vec![],
        commands: vec![CommandRegistration {
            id: EDIT,
            title: "Edit 3D",
            execution: Execution::Worker,
            handler: edit,
        }],
    })
}
impl Creation for Provider {
    fn create(&self, id: DocumentId) -> Result<Document, String> {
        Scene::new(VideoInfo {
            width: 1280,
            height: 720,
            rate: [30, 1],
            frames: 300,
        })
        .document(id)
    }
}
impl BrowserKind for Provider {
    fn browser_kind(&self) -> Option<browser::DocumentKind> {
        Some(browser::DocumentKind {
            type_id: TYPE,
            title: "3D",
        })
    }
}
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn video_info(&self, document: &Document) -> Result<VideoInfo, String> {
        Ok(Scene::from_document(document)?.info)
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, fold_foundation::Failure> {
        request.cancel.check()?;
        if request.reference.document != request.document.id || request.reference.output != "video"
        {
            return Err("Unknown 3D output".into());
        }
        let (scene, binding) = Scene::from_document(request.document)?.evaluate(
            request.document.id,
            request.document.revision,
            request.time,
            request.dimensions,
            &|| request.cancel.check().map_err(|e| e.to_string()),
        )?;
        Ok(RenderGraph {
            width: request.dimensions[0],
            height: request.dimensions[1],
            nodes: vec![ImageOp::Spatial {
                scene: Arc::new(scene),
                binding,
            }],
            output: 0,
        })
    }
}
