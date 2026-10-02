//! First-party native package. All command handlers produce proposals; they
//! cannot acquire a mutable project or touch the UI/device.
use crate::{ImportArgs, SEQUENCE, SEQUENCE_SCHEMA, Sequence, SequenceEdit};
use fold_foundation::DocumentId;
use fold_media::Cancel;
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
        title: "Selection",
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
    fn browser_kind(&self) -> Option<fold_platform::browser::DocumentKind> {
        Some(fold_platform::browser::DocumentKind {
            type_id: SEQUENCE,
            title: "Sequence",
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
        SEQUENCE
    }
    fn schema(&self) -> u32 {
        SEQUENCE_SCHEMA
    }
    fn supports_schema(&self, schema: u32) -> bool {
        [2, SEQUENCE_SCHEMA].contains(&schema)
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Sequence::from_document(document).map(|_| ())
    }
    fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        use fold_media::ingest::SourceProfile;
        let sequence = Sequence::from_document(document)?;
        for clip in sequence
            .clips
            .iter()
            .filter(|c| c.asset == asset && !matches!(c.info, crate::SourceMedia::Document { .. }))
        {
            let compatible = match (&clip.info, &metadata.profile) {
                (crate::SourceMedia::Video(a), SourceProfile::Video(b)) => a == b,
                (crate::SourceMedia::Audio(a), SourceProfile::Wave(b)) => a == b,
                _ => false,
            };
            let kind = if sequence.track(clip.track)?.kind == crate::TrackKind::Audio {
                "audio"
            } else {
                "video"
            };
            if !compatible || !metadata.streams.iter().any(|stream| stream.kind == kind) {
                return Err("relink changes timeline source duration/streams".into());
            }
        }
        Ok(())
    }
}
/// Read-only compatibility registration for the original two-layer media slice.
/// It uses the same capability path as current sequence documents.
struct LegacyLayers;
impl DocumentProvider for LegacyLayers {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::VIDEO_LAYERS
    }
    fn schema(&self) -> u32 {
        1
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        crate::VideoLayers::from_document(document).map(|_| ())
    }
    fn validate_relink(
        &self,
        document: &Document,
        _: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        let layers = crate::VideoLayers::from_document(document)?;
        if metadata.profile != fold_media::ingest::SourceProfile::Video(layers.info) {
            return Err("relink changes legacy layer source profile".into());
        }
        Ok(())
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
            build: concat!(env!("CARGO_PKG_VERSION"), ":timeline-schema3-evaluator3"),
        },
        documents: vec![
            Box::new(Documents),
            Box::new(LegacyLayers),
            Box::new(crate::ImageSequenceProvider),
        ],
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
        video: vec![
            Box::new(crate::SequenceProvider),
            Box::new(crate::VideoLayersProvider),
            Box::new(crate::ImageSequenceProvider),
        ],
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
