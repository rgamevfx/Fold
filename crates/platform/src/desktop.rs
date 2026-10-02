//! Small nonblocking desktop interface. Implementations own jobs and projects;
//! the UI owns controls and presentation textures, never decode/evaluation.
use crate::DisplayFrame;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PreviewKey {
    /// Explicit nested output and exact local time; None selects project root.
    pub target: Option<(fold_foundation::DocumentId, fold_foundation::Time)>,
    pub content: String,
    pub frame: u32,
    pub dimensions: [u32; 2],
    /// Fixed SDR sRGB output transform version; never a project revision.
    pub view: u32,
}
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub document: Option<fold_foundation::DocumentId>,
    pub objects: Vec<fold_foundation::ObjectId>,
}
#[derive(Clone, Debug)]
pub struct ViewLocation {
    pub document: fold_foundation::DocumentId,
    pub time: fold_foundation::Time,
    pub label: String,
}
/// Inclusive review boundaries. These do not trim a document or change export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaybackRange {
    pub start: Option<u32>,
    pub end: Option<u32>,
}
impl PlaybackRange {
    pub fn bounds(self, frames: u32) -> (u32, u32) {
        let last = frames.saturating_sub(1);
        let start = self.start.unwrap_or(0).min(last);
        (start, self.end.unwrap_or(last).min(last).max(start))
    }
}
#[derive(Clone, Copy, Debug)]
pub enum TransportAction {
    TogglePlay,
    Stop,
    PreviousFrame,
    NextFrame,
    GoToIn,
    GoToOut,
    MarkIn,
    MarkOut,
    Jump(u32),
}
#[derive(Clone, Debug)]
pub struct DesktopState {
    pub selection: Selection,
    pub navigation: Vec<ViewLocation>,
    pub transient: bool,
    pub frame: u32,
    pub viewer_document: Option<fold_foundation::DocumentId>,
    pub playback_range: PlaybackRange,
    pub playing: bool,
    pub priming: bool,
    pub audio_clock: bool,
    pub underruns: u64,
    pub content: Option<String>,
    pub dimensions: [u32; 2],
    pub frames: u32,
    pub rate: [u32; 2],
    pub foreground_opacity: f32,
    pub status: String,
    pub busy: bool,
}
impl Default for DesktopState {
    fn default() -> Self {
        Self {
            selection: Selection::default(),
            navigation: vec![],
            transient: false,
            frame: 0,
            viewer_document: None,
            playback_range: PlaybackRange::default(),
            playing: false,
            priming: false,
            audio_clock: false,
            underruns: 0,
            content: None,
            dimensions: [640, 360],
            frames: 1,
            rate: [24, 1],
            foreground_opacity: 0.5,
            status: String::new(),
            busy: false,
        }
    }
}
impl DesktopState {
    pub fn preview_key(&self, frame: u32, divisor: u32) -> Option<PreviewKey> {
        if frame >= self.frames || ![1, 2, 4].contains(&divisor) {
            return None;
        }
        Some(PreviewKey {
            target: self.navigation.last().map(|location| {
                (
                    location.document,
                    if frame == self.frame {
                        location.time
                    } else {
                        fold_foundation::Time::new(
                            i64::from(frame) * i64::from(self.rate[1]),
                            self.rate[0],
                        )
                        .unwrap_or(fold_foundation::Time::ZERO)
                    },
                )
            }),
            content: self.content.clone()?,
            frame,
            dimensions: self.dimensions.map(|n| (n / divisor).max(1)),
            view: 1,
        })
    }
}
#[derive(Clone, Debug)]
pub enum DesktopCommand {
    Notify(String),
    Browser(crate::browser::BrowserCommand),
    Import(Vec<PathBuf>),
    Extension(crate::packages::CommandRequest),
    PreviewExtension(crate::packages::CommandRequest),
    CancelPreviewEdit,
    Navigate(ViewLocation),
    NavigateBack,
    /// Restore the viewer context for the selected editor workspace.
    ActivateWorkspace(String),
    Select(Selection),
    /// Undoable persisted delivery output; independent of source navigation.
    SetOutput(fold_foundation::DocumentId),
    Transport(TransportAction),
    Play,
    Pause,
    Seek(u32),
    Save(PathBuf),
    Open(PathBuf),
    /// Load on a worker and navigate to the first document of a workspace type.
    OpenInWorkspace {
        path: PathBuf,
        document_type: String,
    },
    Undo,
    Redo,
    Opacity(f32),
    Export {
        path: PathBuf,
        start: u32,
        end: u32,
    },
    Cancel,
}
pub struct PreviewResult {
    pub key: PreviewKey,
    pub frame: Result<DisplayFrame, String>,
}
pub trait DesktopClient {
    fn state(&self) -> &DesktopState;
    /// Immutable committed state, never a mutable project or device handle.
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        None
    }
    /// Current committed delivery output and its range metadata.
    fn output_info(&self) -> Result<(fold_foundation::DocumentId, fold_media::VideoInfo), String> {
        Err("no project output".into())
    }
    /// Lightweight capability metadata for document browsers (no media I/O).
    fn video_info(
        &self,
        _document: fold_foundation::DocumentId,
    ) -> Result<fold_media::VideoInfo, String> {
        Err("document metadata unavailable".into())
    }
    fn take_imported_items(&mut self) -> Vec<fold_project::ItemId> {
        vec![]
    }
    fn document_kinds(&self) -> Vec<crate::browser::DocumentKind> {
        vec![]
    }
    fn supports_document(&self, _document: &fold_project::Document) -> bool {
        false
    }
    fn poll(&mut self);
    fn command(&mut self, command: DesktopCommand);
    fn request_preview(&mut self, key: PreviewKey);
    fn cancel_preview(&mut self);
    fn take_preview(&mut self) -> Option<PreviewResult>;
}
