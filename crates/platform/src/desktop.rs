//! Small nonblocking desktop interface. Implementations own jobs and projects;
//! the UI owns controls and presentation textures, never decode/evaluation.
use crate::DisplayFrame;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PreviewKey {
    pub content: String,
    pub frame: u32,
    pub dimensions: [u32; 2],
    /// Fixed SDR sRGB output transform version; never a project revision.
    pub view: u32,
}
#[derive(Clone, Debug)]
pub struct DesktopState {
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
            content: None,
            dimensions: [640, 360],
            frames: 1,
            rate: [24, 1],
            foreground_opacity: 0.5,
            status: "Import one or two matching SDR BT.709 H.264 MP4 files. Video only; no audio."
                .into(),
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
            content: self.content.clone()?,
            frame,
            dimensions: self.dimensions.map(|n| (n / divisor).max(1)),
            view: 1,
        })
    }
}
#[derive(Clone, Debug)]
pub enum DesktopCommand {
    Import(Vec<PathBuf>),
    Save(PathBuf),
    Open(PathBuf),
    Undo,
    Redo,
    Opacity(f32),
    Export { path: PathBuf, start: u32, end: u32 },
    Cancel,
}
pub struct PreviewResult {
    pub key: PreviewKey,
    pub frame: Result<DisplayFrame, String>,
}
pub trait DesktopClient {
    fn state(&self) -> &DesktopState;
    fn poll(&mut self);
    fn command(&mut self, command: DesktopCommand);
    fn request_preview(&mut self, key: PreviewKey);
    fn cancel_preview(&mut self);
    fn take_preview(&mut self) -> Option<PreviewResult>;
}
