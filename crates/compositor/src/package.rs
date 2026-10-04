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
    fn browser_kind(&self) -> Option<fold_platform::browser::DocumentKind> {
        Some(fold_platform::browser::DocumentKind {
            type_id: crate::COMPOSITE,
            title: "Composition",
        })
    }
    fn create(&self, id: fold_foundation::DocumentId) -> Result<Document, String> {
        crate::placement::create(id)
    }
    fn place(
        &self,
        snapshot: &Snapshot,
        placement: &fold_platform::browser::Placement,
        info: Option<fold_media::VideoInfo>,
    ) -> Result<Document, String> {
        crate::placement::place(snapshot, placement, info)
    }
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITE
    }
    fn schema(&self) -> u32 {
        3
    }
    fn supports_schema(&self, schema: u32) -> bool {
        [1, 2, 3].contains(&schema)
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
                source: crate::Source::Asset { asset: id, info },
                ..
            } = node.parameters
            {
                if id == asset && metadata.profile != fold_media::ingest::SourceProfile::Video(info)
                {
                    return Err("relink changes compositor source duration/streams".into());
                }
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
            build: concat!(env!("CARGO_PKG_VERSION"), ":compositor-schema3-evaluator3"),
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
