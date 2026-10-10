//! One lifetime for all presentation state belonging to a stable viewer identity.
//! Persistent bindings live in Workspace; Session alone owns playback workers.
use super::*;
pub(super) struct RuntimeViewer {
    output: Option<workspace::DocumentRef>,
    pub(super) key: WindowKey,
    pub(super) transport: crate::transport::Transport,
    pub(super) navigation: crate::viewer_navigation::Navigation,
    pub(super) viewport_scale: [f32; 2],
    pub(super) viewport_pixels: Option<[f32; 2]>,
    pub(super) presentation: (Option<PreviewKey>, Option<String>),
    pub(super) review: (Option<String>, bool),
}
impl RuntimeViewer {
    pub(super) fn bind(&mut self, output: Option<workspace::DocumentRef>) {
        if self.output != output {
            self.output = output;
            self.presentation = Default::default();
            self.review = Default::default();
            self.transport = Default::default();
        }
    }

    pub(super) fn new(id: PanelInstanceId) -> Self {
        Self {
            output: None,
            key: WindowKey::new(format!("fold.viewer.{}", id.0), format!("Viewer {}", id.0))
                .unwrap(),
            transport: Default::default(),
            navigation: Default::default(),
            viewport_scale: [1.; 2],
            viewport_pixels: None,
            presentation: Default::default(),
            review: Default::default(),
        }
    }
}
