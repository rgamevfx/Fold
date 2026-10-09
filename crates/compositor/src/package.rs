use crate::Composite;
use fold_foundation::DocumentId;
use fold_media::Cancel;
use fold_platform::packages::*;
use fold_project::{Document, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const SHADER_EDIT: &str = "fold.compositor.publish-shader";
pub const EDIT: &str = "fold.compositor.edit";
pub const PANEL: &str = "fold.compositor.editor";
pub const INSPECTOR: &str = "fold.compositor.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: PANEL,
        title: "Compositor",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: INSPECTOR,
        title: "Node Inspector",
        placement: PanelPlacement::Inspector,
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub composite: Composite,
}
fn edit(snapshot: &Snapshot, args: &[u8], _: &Cancel) -> Result<EditBatch, String> {
    let args: EditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let mut document = args.composite.document(args.document)?;
    if let Some(previous) = snapshot.state().documents.get(&args.document) {
        let before = Composite::from_document(previous)?;
        before.check_protection(&args.composite)?;
        document.extensions = previous.extensions.clone();
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
fn publish_shader(snapshot: &Snapshot, args: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let candidate: EditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    candidate.composite.validate()?;
    for node in &candidate.composite.nodes {
        cancel.check()?;
        if let crate::OperationConfig::Shader { settings } = &node.configuration() {
            settings
                .program(
                    fold_platform::format::Format::square([
                        candidate.composite.info.width,
                        candidate.composite.info.height,
                    ]),
                    0.,
                    0.,
                )?
                .validate_source()?;
        }
    }
    cancel.check()?;
    edit(snapshot, args, cancel)
}
struct Documents;
impl DocumentProvider for Documents {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITE
    }
    fn schema(&self) -> u32 {
        8
    }
    fn supports_schema(&self, schema: u32) -> bool {
        schema == 8
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Composite::from_document(document).map(|_| ())
    }
    fn parameters_capability(&self) -> Option<&dyn fold_platform::packages::Parameters> {
        Some(self)
    }
    fn animation_capability(&self) -> Option<&dyn fold_platform::packages::Animation> {
        Some(self)
    }
    fn settings_capability(&self) -> Option<&dyn fold_platform::packages::Settings> {
        Some(self)
    }
    fn image_inputs_capability(&self) -> Option<&dyn fold_platform::packages::ImageInputs> {
        Some(self)
    }
    fn authoring_capability(&self) -> Option<&dyn fold_platform::packages::Authoring> {
        Some(self)
    }
    fn query_capability(&self) -> Option<&dyn fold_platform::packages::Query> {
        Some(self)
    }
    fn authoring_receipt_capability(
        &self,
    ) -> Option<&dyn fold_platform::packages::AuthoringReceipt> {
        Some(self)
    }
    fn creation_capability(&self) -> Option<&dyn fold_platform::packages::Creation> {
        Some(self)
    }
    fn placement_capability(&self) -> Option<&dyn fold_platform::packages::Placement> {
        Some(self)
    }
    fn relink_capability(&self) -> Option<&dyn fold_platform::packages::Relink> {
        Some(self)
    }
    fn evaluation_identity_capability(
        &self,
    ) -> Option<&dyn fold_platform::packages::EvaluationIdentity> {
        Some(self)
    }
}
impl fold_platform::packages::Parameters for Documents {
    fn parameter_time(
        &self,
        _document: &Document,
        output_time: fold_foundation::Time,
    ) -> Result<fold_foundation::Time, String> {
        Ok(output_time)
    }

    fn parameters(
        &self,
        document: &Document,
    ) -> Result<Vec<fold_platform::parameters::Descriptor>, String> {
        crate::parameters::catalog(document)
    }
    fn sample_parameter(
        &self,
        document: &Document,
        target: &fold_platform::parameters::Target,
        time: fold_foundation::Time,
        cancel: &Cancel,
    ) -> Result<fold_graph::types::Literal, String> {
        crate::parameters::sample(document, target, time, cancel)
    }
}
impl fold_platform::packages::Animation for Documents {
    fn animation_channels(
        &self,
        document: &Document,
        object: Option<fold_foundation::ObjectId>,
    ) -> Result<Vec<fold_platform::animation::Channel>, String> {
        crate::keyed_parameters::channels(document, object)
    }
    fn edit_animation(
        &self,
        document: &Document,
        channels: &[fold_platform::animation::Channel],
        time: fold_foundation::Time,
    ) -> Result<Document, String> {
        crate::keyed_parameters::edit(document, channels, time)
    }
}
impl fold_platform::packages::Settings for Documents {
    fn settings(
        &self,
        document: &Document,
    ) -> Result<Option<fold_platform::settings::DocumentSettings>, String> {
        Ok(Some(fold_platform::settings::DocumentSettings::fixed(
            crate::Composite::from_document(document)?.info,
        )))
    }
    fn set_settings(
        &self,
        document: &Document,
        settings: &fold_platform::settings::DocumentSettings,
    ) -> Result<Document, String> {
        let mut value = crate::Composite::from_document(document)?;
        value.info = settings.format.clone();
        value.document(document.id)
    }
}
impl fold_platform::packages::ImageInputs for Documents {
    fn image_inputs(
        &self,
        document: &Document,
    ) -> Result<Vec<fold_platform::layer_graph::ImageInput>, String> {
        crate::inputs::describe(document)
    }
    fn bind_image_input(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        port: &str,
        source: Option<fold_platform::image_source::Source>,
        info: fold_media::VideoInfo,
    ) -> Result<Document, String> {
        crate::inputs::bind(snapshot, document, port, source, info)
    }
}
impl fold_platform::packages::Authoring for Documents {
    fn authoring_catalog(&self) -> serde_json::Value {
        crate::automation::catalog()
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        crate::automation::inspect(document)
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        crate::automation::author(document, edits)
    }
}
impl fold_platform::packages::Query for Documents {
    fn query(
        &self,
        document: &Document,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        crate::automation::query(document, query)
    }
}
impl fold_platform::packages::AuthoringReceipt for Documents {
    fn authoring_receipt(
        &self,
        before: &Document,
        after: &Document,
    ) -> Result<serde_json::Value, String> {
        crate::automation::receipt(before, after)
    }
}
impl fold_platform::packages::Creation for Documents {
    fn create(&self, id: fold_foundation::DocumentId) -> Result<Document, String> {
        crate::placement::create(id)
    }
}
impl fold_platform::packages::Placement for Documents {
    fn place(
        &self,
        snapshot: &Snapshot,
        placement: &fold_platform::browser::Placement,
        info: Option<fold_media::VideoInfo>,
    ) -> Result<EditBatch, String> {
        Ok(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(crate::placement::place(
                snapshot, placement, info,
            )?)],
        })
    }
}
impl fold_platform::packages::Relink for Documents {
    fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        for node in Composite::from_document(document)?.nodes {
            if let crate::OperationConfig::Read {
                source:
                    crate::Source::Exr {
                        asset: id,
                        image,
                        sequence,
                        ..
                    },
                ..
            } = &node.configuration()
                && *id == asset
                && !matches!(&metadata.profile, fold_media::ingest::SourceProfile::Exr(new) if image.compatible_pixels(new) && match (sequence, &new.sequence) { (None,None) => true, (Some(expected),Some(actual)) => expected.matches(actual), _ => false })
            {
                return Err("Relink changes the EXR channel catalog or image windows".into());
            }
            if let crate::OperationConfig::Read {
                source: crate::Source::Asset { asset: id, info },
                ..
            } = node.configuration()
                && id == asset
                && !match &metadata.profile {
                    fold_media::ingest::SourceProfile::Video(updated) => *updated == info,
                    fold_media::ingest::SourceProfile::Ppm { dimensions } => {
                        *dimensions == [info.width, info.height]
                    }
                    _ => false,
                }
            {
                return Err("relink changes compositor source duration/streams".into());
            }
        }
        Ok(())
    }
}
impl fold_platform::packages::EvaluationIdentity for Documents {
    fn evaluation_identity(&self, document: &Document) -> Result<Vec<u8>, String> {
        let mut graph = Composite::from_document(document)?;
        for node in &mut graph.nodes {
            node.position = None;
        }
        graph.nodes.sort_by_key(|n| n.id);
        serde_json::to_vec(&graph).map_err(|e| e.to_string())
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
            build: concat!(env!("CARGO_PKG_VERSION"), ":compositor-schema8-evaluator10"),
        },
        documents: vec![Box::new(Documents)],
        commands: vec![
            CommandRegistration {
                id: SHADER_EDIT,
                title: "Validate and publish shader",
                execution: Execution::Worker,
                handler: publish_shader,
            },
            CommandRegistration {
                id: "fold.compositor.create-effect",
                title: "Create reusable effect",
                execution: Execution::Immediate,
                handler: crate::presets::command,
            },
            CommandRegistration {
                id: EDIT,
                title: "Edit composite",
                execution: Execution::Immediate,
                handler: edit,
            },
        ],
        video: vec![Box::new(crate::compile::Provider)],
        audio: vec![],
    })
}
