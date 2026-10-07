use crate::Composite;
use fold_foundation::DocumentId;
use fold_media::Cancel;
use fold_platform::packages::*;
use fold_project::{Document, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

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
        Composite::from_document(previous)?;
        document.extensions = previous.extensions.clone();
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
struct Documents;
impl DocumentProvider for Documents {
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
        crate::automation::query(document, query)
    }
    fn authoring_receipt(
        &self,
        before: &Document,
        after: &Document,
    ) -> Result<serde_json::Value, String> {
        crate::automation::receipt(before, after)
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        crate::automation::author(document, edits)
    }

    fn create(&self, id: fold_foundation::DocumentId) -> Result<Document, String> {
        crate::placement::create(id)
    }
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
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITE
    }
    fn schema(&self) -> u32 {
        5
    }
    fn supports_schema(&self, schema: u32) -> bool {
        [1, 2, 3, 4, 5].contains(&schema)
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Composite::from_document(document).map(|_| ())
    }
    fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        for node in Composite::from_document(document)?.nodes {
            if let crate::Parameters::Read {
                source:
                    crate::Source::Exr {
                        asset: id,
                        image,
                        sequence,
                        ..
                    },
                ..
            } = &node.parameters
                && *id == asset
                && !matches!(&metadata.profile, fold_media::ingest::SourceProfile::Exr(new) if image.compatible_pixels(new) && match (sequence, &new.sequence) { (None,None) => true, (Some(expected),Some(actual)) => expected.matches(actual), _ => false })
            {
                return Err("Relink changes the EXR channel catalog or image windows".into());
            }
            if let crate::Parameters::Read {
                source: crate::Source::Asset { asset: id, info },
                ..
            } = node.parameters
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
            build: concat!(env!("CARGO_PKG_VERSION"), ":compositor-schema5-evaluator6"),
        },
        documents: vec![Box::new(Documents)],
        commands: vec![CommandRegistration {
            id: EDIT,
            title: "Edit composite",
            execution: Execution::Immediate,
            handler: edit,
        }],
        video: vec![Box::new(crate::compile::Provider)],
        audio: vec![],
    })
}
