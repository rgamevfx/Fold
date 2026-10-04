//! Small nonblocking desktop interface. Implementations own jobs and projects;
//! the UI owns controls and presentation textures, never decode/evaluation.
use crate::DisplayFrame;
pub use fold_render::view::{Range as DisplayRange, View as ChannelView};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PreviewKey {
    /// Explicit nested output and exact local time; None selects project root.
    pub target: Option<(fold_foundation::DocumentId, fold_foundation::Time)>,
    pub output: String,
    pub content: String,
    pub frame: u32,
    pub dimensions: [u32; 2],
    /// Fixed sRGB viewer-policy version; working/config resource identity is
    /// included in content. Delivery transforms never enter presentation keys.
    pub view: u32,
    pub channels: ChannelView,
}
/// One latest demand per viewer. Background preparation never replaces that
/// viewer's foreground demand; it occupies a separate bounded demand slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewDemand {
    pub consumer: crate::workspace::PanelInstanceId,
    pub generation: u64,
    pub key: PreviewKey,
    pub background: bool,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    pub document: Option<fold_foundation::DocumentId>,
    pub objects: Vec<fold_foundation::ObjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViewLocation {
    pub document: fold_foundation::DocumentId,
    pub time: fold_foundation::Time,
    pub label: String,
}
/// Inclusive review boundaries. These do not trim a document or change export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub color_choices: crate::color::Choices,
    pub selection: Selection,
    pub navigation: Vec<ViewLocation>,
    /// Explicit Project/open navigation, not focus or selection notification.
    pub navigation_event: u64,
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
            color_choices: Default::default(),
            selection: Selection::default(),
            navigation: vec![],
            navigation_event: 0,
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
            output: "video".into(),
            content: self.content.clone()?,
            frame,
            dimensions: self.dimensions.map(|n| (n / divisor).max(1)),
            view: 1,
            channels: Default::default(),
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlaybackMode {
    #[default]
    RealTime,
    EveryFrame,
}

/// Workspace transport intent/status, never authoritative project content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerTransport {
    pub mode: PlaybackMode,
    pub output: fold_project::DocumentRef,
    pub time: fold_foundation::Time,
    pub range: PlaybackRange,
    pub looping: bool,
    pub playing: bool,
}

#[derive(Clone, Debug)]
pub enum DesktopCommand {
    ViewerTransport {
        viewer: crate::workspace::PanelInstanceId,
        transport: ViewerTransport,
    },
    CloseViewer(crate::workspace::PanelInstanceId),
    MonitorViewer(Option<crate::workspace::PanelInstanceId>),
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
    SetOutputColor {
        document: fold_foundation::DocumentId,
        transform: crate::color::OutputTransform,
    },
    SetInputColor {
        asset: fold_foundation::AssetId,
        space: String,
    },
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
    pub consumers: Vec<(crate::workspace::PanelInstanceId, u64)>,
}
#[cfg(feature = "gpu")]
pub struct GpuPreviewResult {
    pub key: PreviewKey,
    pub frame: Result<crate::gpu::Display, String>,
    pub consumers: Vec<(crate::workspace::PanelInstanceId, u64)>,
}
pub trait DesktopClient {
    /// Nonblocking notification after a result is published; no frame ownership crosses this callback.
    fn set_preview_wake(&mut self, _wake: std::sync::Arc<dyn Fn() + Send + Sync>) {}
    #[cfg(feature = "gpu")]
    fn set_render_host(&mut self, _host: crate::gpu::Host) {}
    #[cfg(feature = "gpu")]
    fn take_gpu_preview(&mut self) -> Option<GpuPreviewResult> {
        None
    }
    fn state(&self) -> &DesktopState;
    /// On-demand diagnostics, not permanent artist-facing chrome.
    fn resource_statistics(&self) -> String {
        String::new()
    }
    fn viewer_transport(
        &self,
        _viewer: crate::workspace::PanelInstanceId,
    ) -> Option<ViewerTransport> {
        None
    }
    /// Generation and next requested time; distinct from the presented playhead.
    fn viewer_request(
        &self,
        _viewer: crate::workspace::PanelInstanceId,
    ) -> Option<(u64, fold_foundation::Time)> {
        None
    }
    /// Admit a ready image for this generation. False means retain the old image.
    fn present_viewer(
        &mut self,
        _viewer: crate::workspace::PanelInstanceId,
        _generation: u64,
        _key: &PreviewKey,
    ) -> bool {
        true
    }
    /// Monitoring errors are scoped to the explicitly monitored viewer.
    fn viewer_audio_error(&self, _viewer: crate::workspace::PanelInstanceId) -> Option<String> {
        None
    }
    fn viewer_audio_underruns(&self, _viewer: crate::workspace::PanelInstanceId) -> Option<u64> {
        None
    }
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
    /// Explicit output metadata and identity without retargeting global state.
    fn preview_state(
        &self,
        output: &fold_project::DocumentRef,
        time: fold_foundation::Time,
    ) -> DesktopState {
        let mut state = self.state().clone();
        state.navigation = vec![ViewLocation {
            document: output.document,
            time,
            label: String::new(),
        }];
        state.selection = Selection {
            document: Some(output.document),
            objects: vec![],
        };
        state.viewer_document = Some(output.document);
        state.content = None;
        state.playing = false;
        if let Ok(info) = self.video_info(output.document) {
            state.frames = info.frames;
            state.dimensions = [info.width, info.height];
            state.rate = info.rate;
            state.frame = time
                .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
                .unwrap_or(0)
                .max(0) as u32;
        }
        state
    }
    fn channels(
        &self,
        _output: &fold_project::DocumentRef,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        fold_render::channels::RGBA
            .into_iter()
            .map(|name| name.to_owned().try_into())
            .collect()
    }
    fn outputs(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Vec<crate::workspace::OutputDescriptor> {
        vec![crate::workspace::OutputDescriptor {
            reference: fold_project::DocumentRef {
                document,
                output: "video".into(),
                extensions: Default::default(),
            },
            label: "Video".into(),
            info: self.video_info(document),
            playback_mode: PlaybackMode::RealTime,
        }]
    }
    /// Set only after a successful open/save; Save As gets a distinct association.
    fn workspace_project(&self) -> Option<String> {
        None
    }
    fn workspace_epoch(&self) -> u64 {
        0
    }
    fn workspace_restore(&self) -> bool {
        false
    }
    /// Instance contexts prohibit first-document fallbacks in provider editors.
    fn explicit_target(&self) -> bool {
        false
    }
    fn panel_instance(&self) -> Option<crate::workspace::PanelInstanceId> {
        None
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
    /// Defer new work while collecting results; finish with `end_preview_update`.
    /// Already active work continues. This keeps successor selection atomic.
    fn begin_preview_update(&mut self) {}
    fn end_preview_update(&mut self) {}
    /// Atomically replace bounded per-consumer demand. Empty retires all demand.
    /// The serial default keeps small clients compatible; the application owns
    /// independent cancellation, fair selection and shared completion routing.
    fn preview_demands(&mut self, demands: Vec<PreviewDemand>) {
        if let Some(demand) = demands.first() {
            self.request_preview(demand.key.clone());
        } else {
            self.cancel_preview();
        }
    }
    fn cancel_preview(&mut self);
    fn take_preview(&mut self) -> Option<PreviewResult>;
}
