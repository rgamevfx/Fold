//! First-party native package. All command handlers produce proposals; they
//! cannot acquire a mutable project or touch the UI/device.
use crate::{ImportArgs, SEQUENCE, SEQUENCE_SCHEMA, Sequence, SequenceEdit};
use fold_foundation::DocumentId;
use fold_media::{Cancel, VideoInfo};
use fold_platform::packages::*;
use fold_project::{Document, EditBatch, Snapshot};
use serde::{Deserialize, Serialize};

pub const EDIT: &str = "fold.timeline.edit";
pub const IMPORT: &str = "fold.timeline.import";
pub const PANEL: &str = "fold.timeline.editor";
pub const INSPECTOR: &str = "fold.timeline.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: PANEL,
        title: "Timeline",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: INSPECTOR,
        title: "Media / Selection",
        placement: PanelPlacement::Inspector,
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditArgs {
    pub document: DocumentId,
    pub edit: SequenceEdit,
}
fn edit(snapshot: &Snapshot, args: &[u8], _: &Cancel) -> Result<EditBatch, String> {
    let args: EditArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    crate::edit_sequence(snapshot, args.document, &args.edit)
}
fn import(snapshot: &Snapshot, args: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let args: ImportArgs = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    crate::import_media(snapshot, &args, cancel)
}
struct Documents;
impl DocumentProvider for Documents {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        SEQUENCE
    }
    fn schema(&self) -> u32 {
        SEQUENCE_SCHEMA
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Sequence::from_document(document).map(|_| ())
    }
    fn video_info(&self, document: &Document) -> Result<VideoInfo, String> {
        Sequence::from_document(document)?.info()
    }
}
struct Audio;
impl AudioProvider for Audio {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        SEQUENCE
    }
    fn compile(
        &self,
        snapshot: &Snapshot,
        document: &Document,
    ) -> Result<fold_render::audio::AudioPlan, String> {
        Sequence::from_document(document)?.audio_plan(snapshot)
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
            build: concat!(env!("CARGO_PKG_VERSION"), ":timeline-schema2-evaluator2"),
        },
        documents: vec![Box::new(Documents)],
        commands: vec![
            CommandRegistration {
                id: EDIT,
                title: "Edit sequence",
                execution: Execution::Immediate,
                handler: edit,
            },
            CommandRegistration {
                id: IMPORT,
                title: "Import timeline media",
                execution: Execution::Worker,
                handler: import,
            },
        ],
        video: vec![Box::new(crate::SequenceProvider)],
        audio: vec![Box::new(Audio)],
    })
}
pub fn request<T: Serialize>(
    snapshot: &Snapshot,
    id: &str,
    args: &T,
) -> Result<CommandRequest, String> {
    Ok(CommandRequest {
        id: id.into(),
        base: snapshot.revision(),
        arguments: serde_json::to_vec(args).map_err(|e| e.to_string())?,
    })
}
