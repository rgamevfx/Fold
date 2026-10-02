//! Adapt the existing trusted editor API to an explicit workspace context.
//! Feature commands already carry document IDs; only navigation/selection/time
//! are scoped here. Project edits and delivery commands still go to the host.
use crate::{desktop::*, workspace::EditorInstance};
use fold_foundation::Time;
use fold_project::DocumentRef;

pub struct PanelContext<'a> {
    host: &'a mut dyn DesktopClient,
    editor: Option<&'a mut EditorInstance>,
    state: DesktopState,
    playback_requested: Option<bool>,
    instance: Option<crate::workspace::PanelInstanceId>,
    seek_request: Option<Time>,
}
impl<'a> PanelContext<'a> {
    pub fn new(host: &'a mut dyn DesktopClient, editor: &'a mut EditorInstance) -> Self {
        let mut state = editor
            .output()
            .map(|output| host.preview_state(&output, editor.navigation.last().unwrap().time))
            .unwrap_or_else(|| {
                let mut state = host.state().clone();
                state.selection = Default::default();
                state.navigation.clear();
                state.viewer_document = None;
                state.content = None;
                state.playing = false;
                state
            });
        if editor.selection.document != editor.document() {
            editor.selection = Selection {
                document: editor.document(),
                objects: vec![],
            };
        }
        state.selection = editor.selection.clone();
        state.navigation = editor.navigation.clone();
        Self {
            host,
            editor: Some(editor),
            state,
            playback_requested: None,
            instance: None,
            seek_request: None,
        }
    }
    /// Non-document panels have an identity without acquiring an editor binding.
    pub fn for_panel(
        host: &'a mut dyn DesktopClient,
        id: crate::workspace::PanelInstanceId,
    ) -> Self {
        let state = host.state().clone();
        Self {
            host,
            editor: None,
            state,
            playback_requested: None,
            instance: Some(id),
            seek_request: None,
        }
    }
    pub fn instance(mut self, id: crate::workspace::PanelInstanceId) -> Self {
        self.instance = Some(id);
        self
    }
    pub fn with_output(mut self, output: &DocumentRef) -> Self {
        if let Some(editor) = &self.editor
            && let Some(location) = editor.navigation.last()
        {
            self.state = self.host.preview_state(output, location.time);
            self.state.selection = editor.selection.clone();
            self.state.navigation = editor.navigation.clone();
        }
        self
    }
    /// Transport intent is returned to the shell and addressed to this viewer only.
    pub fn transport_context(&mut self, playing: bool, range: PlaybackRange) {
        self.state.playback_range = range;
        self.state.playing = playing;
        self.state.priming = false;
    }
    pub fn seek_request(&self) -> Option<Time> {
        self.seek_request
    }
    pub fn playback_requested(&self) -> Option<bool> {
        self.playback_requested
    }
    fn play(&mut self) {
        if let Some(location) = self
            .editor
            .as_ref()
            .and_then(|e| e.navigation.last())
            .cloned()
        {
            let valid = Time::new(
                i64::from(self.state.frames) * i64::from(self.state.rate[1]),
                self.state.rate[0],
            )
            .is_ok_and(|end| {
                self.state.frames > 0 && location.time >= Time::ZERO && location.time < end
            });
            if !valid {
                self.state.status = "No available output at this local time".into();
                self.host
                    .command(DesktopCommand::Notify(self.state.status.clone()));
                return;
            }
            self.state.playing = true;
            self.playback_requested = Some(true);
        }
    }
    fn seek(&mut self, frame: u32) {
        let frame = frame.min(self.state.frames.saturating_sub(1));
        self.playback_requested = Some(false);
        self.state.playing = false;
        self.state.frame = frame;
        if let Some(location) = self.editor.as_mut().and_then(|e| e.navigation.last_mut()) {
            location.time = Time::new(
                i64::from(frame) * i64::from(self.state.rate[1]),
                self.state.rate[0],
            )
            .unwrap_or(Time::ZERO);
        }
        self.seek_request = self
            .editor
            .as_ref()
            .and_then(|e| e.navigation.last())
            .map(|v| v.time);
        self.sync();
    }
    fn sync(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        if self.state.viewer_document != editor.document()
            && let Some(output) = editor.output()
        {
            self.state = self
                .host
                .preview_state(&output, editor.navigation.last().unwrap().time);
        }
        self.state.selection = editor.selection.clone();
        self.state.navigation = editor.navigation.clone();
        self.state.viewer_document = editor.document();
    }
    fn navigate(&mut self, location: ViewLocation) {
        let result = self.host.video_info(location.document).and_then(|info| {
            if location.time < Time::ZERO || location.time >= info.time(info.frames)? {
                Err("Source time is outside the document range".into())
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.state.status = error;
            return;
        }
        self.host.command(DesktopCommand::CancelPreviewEdit);
        self.editor.as_mut().unwrap().navigate(location);
        self.sync();
        // A nested source may be a peer document type. The shell replaces the
        // contribution at this same instance ID, retaining the breadcrumb.
        if let Some(snapshot) = self.host.snapshot()
            && let Some(document) = self
                .editor
                .as_ref()
                .and_then(|e| e.document())
                .and_then(|id| snapshot.state().documents.get(&id))
        {
            self.editor.as_mut().unwrap().document_type = document.type_id.clone();
        }
    }
}
impl DesktopClient for PanelContext<'_> {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn explicit_target(&self) -> bool {
        self.editor.is_some()
    }
    fn panel_instance(&self) -> Option<crate::workspace::PanelInstanceId> {
        self.instance
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        self.host.snapshot()
    }
    fn output_info(&self) -> Result<(fold_foundation::DocumentId, fold_media::VideoInfo), String> {
        self.host.output_info()
    }
    fn video_info(&self, id: fold_foundation::DocumentId) -> Result<fold_media::VideoInfo, String> {
        self.host.video_info(id)
    }
    fn outputs(&self, id: fold_foundation::DocumentId) -> Vec<crate::workspace::OutputDescriptor> {
        self.host.outputs(id)
    }
    fn preview_state(&self, output: &DocumentRef, time: Time) -> DesktopState {
        self.host.preview_state(output, time)
    }
    fn supports_document(&self, document: &fold_project::Document) -> bool {
        self.host.supports_document(document)
    }
    fn document_kinds(&self) -> Vec<crate::browser::DocumentKind> {
        self.host.document_kinds()
    }
    fn workspace_project(&self) -> Option<String> {
        self.host.workspace_project()
    }
    fn workspace_epoch(&self) -> u64 {
        self.host.workspace_epoch()
    }
    fn workspace_restore(&self) -> bool {
        self.host.workspace_restore()
    }
    fn take_imported_items(&mut self) -> Vec<fold_project::ItemId> {
        self.host.take_imported_items()
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        if self.editor.is_none() {
            self.host.command(command);
            self.state = self.host.state().clone();
            return;
        }
        match command {
            DesktopCommand::Select(selection) => {
                if selection.document != self.editor.as_ref().and_then(|e| e.document()) {
                    self.state.status = "Selection does not belong to this editor target".into();
                    return;
                }
                self.editor.as_mut().unwrap().selection = selection;
                self.sync();
            }
            DesktopCommand::Navigate(location) => self.navigate(location),
            DesktopCommand::NavigateBack => {
                self.editor.as_mut().unwrap().back();
                self.sync();
                if let Some(snapshot) = self.host.snapshot()
                    && let Some(document) = self
                        .editor
                        .as_ref()
                        .and_then(|e| e.document())
                        .and_then(|id| snapshot.state().documents.get(&id))
                {
                    self.editor.as_mut().unwrap().document_type = document.type_id.clone();
                }
            }
            DesktopCommand::Seek(frame) => self.seek(frame),
            DesktopCommand::Play => self.play(),
            DesktopCommand::Pause => {
                self.state.playing = false;
                self.playback_requested = Some(false);
            }
            DesktopCommand::Transport(action) => {
                use TransportAction::*;
                let (start, end) = self.state.playback_range.bounds(self.state.frames);
                match action {
                    TogglePlay if self.state.playing => self.command(DesktopCommand::Pause),
                    TogglePlay => self.play(),
                    MarkIn => {
                        self.state.playback_range.start = Some(self.state.frame);
                        if self
                            .state
                            .playback_range
                            .end
                            .is_some_and(|end| end < self.state.frame)
                        {
                            self.state.playback_range.end = Some(self.state.frame);
                        }
                    }
                    MarkOut => {
                        self.state.playback_range.end = Some(self.state.frame);
                        if self
                            .state
                            .playback_range
                            .start
                            .is_some_and(|start| start > self.state.frame)
                        {
                            self.state.playback_range.start = Some(self.state.frame);
                        }
                    }
                    Stop | GoToIn => self.seek(start),
                    GoToOut => self.seek(end),
                    PreviousFrame => self.seek(self.state.frame.saturating_sub(1)),
                    NextFrame => self.seek(self.state.frame.saturating_add(1)),
                    Jump(frame) => self.seek(frame),
                }
            }
            DesktopCommand::ActivateWorkspace(_) => {} // Focus is not navigation.
            other => {
                self.host.command(other);
                self.state.status = self.host.state().status.clone();
            }
        }
    }
    fn request_preview(&mut self, key: PreviewKey) {
        self.host.request_preview(key);
    }
    fn cancel_preview(&mut self) {
        self.host.cancel_preview();
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        self.host.take_preview()
    }
}
