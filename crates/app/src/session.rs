//! Nonblocking project coordinator and bounded jobs. UI calls never perform I/O.
use crate::media_workflow as workflow;
use fold_media::{Cancel, Decoder};
use fold_platform::desktop::{
    DesktopClient, DesktopCommand, DesktopState, PreviewKey, PreviewResult,
};
use fold_project::{EditBatch, Mutation, Project, Snapshot};
use std::sync::{
    Arc, Condvar, Mutex,
    mpsc::{self, Receiver},
};

#[path = "session_transport.rs"]
mod transport;
#[path = "session_view.rs"]
mod view;

type Request = (Snapshot, PreviewKey, Cancel);
struct Mailbox {
    pending: Option<Request>,
    result: Option<PreviewResult>,
    stop: bool,
}
struct PreviewWorker {
    shared: Arc<(Mutex<Mailbox>, Condvar)>,
    cancel: Cancel,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl PreviewWorker {
    fn new() -> Self {
        let shared = Arc::new((
            Mutex::new(Mailbox {
                pending: None,
                result: None,
                stop: false,
            }),
            Condvar::new(),
        ));
        let state = shared.clone();
        let thread = std::thread::spawn(move || {
            let mut decoder = Decoder::default();
            loop {
                let (snapshot, key, cancel) = {
                    let (lock, ready) = &*state;
                    let mut queue = lock.lock().unwrap();
                    while queue.pending.is_none() && !queue.stop {
                        queue = ready.wait(queue).unwrap();
                    }
                    if queue.stop {
                        break;
                    }
                    queue.pending.take().unwrap()
                };
                let frame = workflow::evaluate(&snapshot, &key, &mut decoder, &cancel)
                    .and_then(|f| f.to_display().map_err(str::to_owned));
                let mut queue = state.0.lock().unwrap();
                // Request replacement/cancellation and publication serialize here.
                if cancel.check().is_ok() && !queue.stop {
                    queue.result = Some(PreviewResult { key, frame });
                }
            }
        });
        Self {
            shared,
            cancel: Cancel::default(),
            thread: Some(thread),
        }
    }
    fn cancel(&mut self) {
        let mut queue = self.shared.0.lock().unwrap();
        self.cancel.cancel();
        queue.pending = None;
        queue.result = None;
    }
    fn request(&mut self, snapshot: Snapshot, key: PreviewKey) {
        self.cancel();
        self.cancel = Cancel::default();
        self.shared.0.lock().unwrap().pending = Some((snapshot, key, self.cancel.clone()));
        self.shared.1.notify_one();
    }
    fn take(&self) -> Option<PreviewResult> {
        self.shared.0.lock().unwrap().result.take()
    }
}
impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.cancel();
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

enum Completed {
    Imported(EditBatch),
    Opened(Project, fold_project::Revision, Option<String>),
    Message(String),
}
struct Background {
    cancel: Cancel,
    result: Receiver<Result<Completed, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Background {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub struct Session {
    project: Project,
    state: DesktopState,
    overlay: Option<Snapshot>,
    preview: PreviewWorker,
    background: Option<Background>,
    playback_ranges: std::collections::BTreeMap<
        fold_foundation::DocumentId,
        fold_platform::desktop::PlaybackRange,
    >,
    #[cfg(feature = "desktop")]
    playback: crate::playback::Playback,
}
impl Default for Session {
    fn default() -> Self {
        Self::new(Project::new(32))
    }
}
impl Session {
    pub fn new(project: Project) -> Self {
        let mut session = Self {
            project,
            state: DesktopState::default(),
            overlay: None,
            preview: PreviewWorker::new(),
            background: None,
            playback_ranges: Default::default(),
            #[cfg(feature = "desktop")]
            playback: crate::playback::Playback::default(),
        };
        session.refresh();
        session
    }
    fn refresh(&mut self) {
        let committed = self.project.snapshot();
        if self
            .state
            .navigation
            .iter()
            .any(|v| !committed.state().documents.contains_key(&v.document))
        {
            self.state.navigation.clear();
        }
        let snapshot = self.preview_snapshot();
        let content = if let Some(location) = self.state.navigation.last() {
            workflow::content_for(&snapshot, location.document).ok()
        } else {
            workflow::content(&snapshot).ok()
        };
        self.state.transient = self.overlay.is_some();
        if self.state.content != content {
            self.stop_playback();
            self.preview.cancel();
        }
        self.state.content = content;
        if self
            .state
            .selection
            .document
            .is_some_and(|id| !snapshot.state().documents.contains_key(&id))
        {
            self.state.selection = Default::default();
        }
        let output = if let Some(location) = self.state.navigation.last() {
            crate::packages::builtins()
                .output(&snapshot, location.document)
                .map(|info| (location.document, info))
        } else {
            workflow::output(&snapshot).map(|(source, info)| (source.document, info))
        };
        self.playback_ranges
            .retain(|id, _| committed.state().documents.contains_key(id));
        self.state.viewer_document = output.as_ref().ok().map(|(id, _)| *id);
        self.state.playback_range = self
            .state
            .viewer_document
            .and_then(|id| self.playback_ranges.get(&id).copied())
            .unwrap_or_default();
        let info = output.map(|(_, info)| info);
        if let Ok(info) = info {
            self.state.frames = info.frames;
            self.state.dimensions = [info.width, info.height];
            self.state.rate = info.rate;
            self.state.frame = self.state.frame.min(info.frames - 1);
        } else {
            self.state.frames = 1;
            self.state.frame = 0;
        }
        if let Ok((_, layers)) = workflow::active(&snapshot) {
            self.state.foreground_opacity = layers.foreground_opacity;
        }
    }
    fn background(&mut self, command: DesktopCommand) {
        if self.background.is_some() {
            self.state.status = "A background job is already running; cancel or wait.".into();
            return;
        }
        let snapshot = self.project.snapshot();
        let cancel = Cancel::default();
        let token = cancel.clone();
        let (sender, result) = mpsc::sync_channel(1);
        self.state.busy = true;
        self.state.status = "Background job running…".into();
        let thread = std::thread::spawn(move || {
            let result = token.check().and_then(|()| match command {
                DesktopCommand::Extension(request) => crate::packages::builtins()
                    .stage(&snapshot, &request, &token)
                    .map(Completed::Imported),
                DesktopCommand::Import(paths) => {
                    workflow::import(&snapshot, &paths, &token).map(Completed::Imported)
                }
                DesktopCommand::Save(path) => fold_project::save(&snapshot, &path)
                    .map(|_| Completed::Message("Project saved".into()))
                    .map_err(|e| e.to_string()),
                DesktopCommand::Open(path) => fold_project::load(&path, 32)
                    .map(|project| Completed::Opened(project, snapshot.revision(), None))
                    .map_err(|e| e.to_string()),
                DesktopCommand::OpenInWorkspace {
                    path,
                    document_type,
                } => fold_project::load(&path, 32)
                    .map(|project| {
                        Completed::Opened(project, snapshot.revision(), Some(document_type))
                    })
                    .map_err(|e| e.to_string()),
                DesktopCommand::Export { path, start, end } => {
                    workflow::export(&snapshot, &path, start, end, &token)
                        .map(|_| Completed::Message(format!("Export complete: {}", path.display())))
                }
                _ => unreachable!(),
            });
            let _ = sender.send(result);
        });
        self.background = Some(Background {
            cancel,
            result,
            thread: Some(thread),
        });
    }
}
impl DesktopClient for Session {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<Snapshot> {
        Some(self.project.snapshot())
    }
    fn video_info(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Result<fold_media::VideoInfo, String> {
        crate::packages::builtins().output(&self.project.snapshot(), document)
    }
    fn poll(&mut self) {
        #[cfg(feature = "desktop")]
        {
            use fold_foundation::{Rounding, Time};
            if let Some(sample) = self.playback.sample()
                && self.state.playing
            {
                let frame = Time::new(sample as i64, fold_media::audio::AUDIO_RATE)
                    .unwrap()
                    .to_ticks(self.state.rate[0], self.state.rate[1], Rounding::Floor)
                    .unwrap_or(0)
                    .max(0) as u32;
                self.seek(frame.min(self.state.playback_range.bounds(self.state.frames).1));
            }
            self.state.playing = self.playback.playing();
            self.state.priming = self.playback.priming();
            self.state.audio_clock = self.playback.audio_clock();
            self.state.underruns = self.playback.underruns();
            if let Some(error) = self.playback.take_error() {
                self.state.status = error;
            }
        }
        let result = self
            .background
            .as_ref()
            .and_then(|job| match job.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("background worker stopped".into()))
                }
            });
        if let Some(result) = result {
            let cancelled = self.background.as_ref().unwrap().cancel.check().is_err();
            // Worker has sent its final result, so joining performs no media work.
            self.background.take();
            self.state.busy = false;
            // Discard cancelled proposals, but never claim that an already
            // finalized save/export was rolled back by a late cancel click.
            let result = if cancelled
                && matches!(&result, Ok(Completed::Imported(_) | Completed::Opened(..)))
            {
                Err("job cancelled".into())
            } else {
                result
            };
            match result {
                Ok(Completed::Imported(batch)) => match self.project.commit(batch) {
                    Ok(_) => {
                        self.overlay = None;
                        self.refresh();
                        self.state.status = "Import committed".into();
                    }
                    Err(e) => self.state.status = e.to_string(),
                },
                Ok(Completed::Opened(project, base, workspace)) => {
                    if self.project.snapshot().revision() != base {
                        self.state.status = "Project changed while opening; retry open".into();
                    } else {
                        self.project = project;
                        self.playback_ranges.clear();
                        self.overlay = None;
                        self.state.navigation.clear();
                        self.refresh();
                        self.state.status = "Project opened".into();
                        if let Some(kind) = workspace {
                            let snapshot = self.project.snapshot();
                            if let Some(document) = snapshot
                                .state()
                                .documents
                                .values()
                                .find(|d| d.type_id == kind)
                            {
                                self.navigate(fold_platform::desktop::ViewLocation {
                                    document: document.id,
                                    time: fold_foundation::Time::ZERO,
                                    label: kind.rsplit('.').next().unwrap_or("Document").into(),
                                });
                            } else {
                                self.state.status =
                                    format!("Project opened; no document for workspace {kind}");
                            }
                        }
                    }
                }
                Ok(Completed::Message(message)) => self.state.status = message,
                Err(error) => self.state.status = error,
            }
        }
    }
    fn command(&mut self, command: DesktopCommand) {
        let result = match command {
            DesktopCommand::PreviewExtension(request) => {
                self.preview_edit(request);
                return;
            }
            DesktopCommand::CancelPreviewEdit => {
                self.overlay = None;
                self.refresh();
                return;
            }
            DesktopCommand::Navigate(location) => {
                self.navigate(location);
                return;
            }
            DesktopCommand::NavigateBack => {
                self.back();
                return;
            }
            DesktopCommand::ActivateWorkspace(kind) => {
                self.activate_workspace(&kind);
                return;
            }
            DesktopCommand::Transport(action) => {
                self.transport(action);
                return;
            }
            DesktopCommand::Play => {
                self.start_playback();
                return;
            }
            DesktopCommand::Pause => {
                self.stop_playback();
                return;
            }
            DesktopCommand::Seek(frame) => {
                self.seek(frame);
                if self.state.playing {
                    self.start_playback();
                }
                return;
            }
            DesktopCommand::Select(selection) => {
                self.state.selection = selection;
                return;
            }
            DesktopCommand::Extension(request) => {
                let registry = crate::packages::builtins();
                match registry.command(&request.id) {
                    Ok(command)
                        if command.execution == fold_platform::packages::Execution::Worker =>
                    {
                        self.background(DesktopCommand::Extension(request));
                        return;
                    }
                    Ok(_) => registry
                        .stage(&self.project.snapshot(), &request, &Cancel::default())
                        .and_then(|batch| {
                            self.project
                                .commit(batch)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }),
                    Err(error) => Err(error),
                }
            }
            DesktopCommand::Cancel => {
                if let Some(job) = &self.background {
                    job.cancel.cancel();
                }
                self.state.status = "Cancellation requested".into();
                return;
            }
            DesktopCommand::Undo => self.project.undo().map(|_| ()).map_err(|e| e.to_string()),
            DesktopCommand::Redo => self.project.redo().map(|_| ()).map_err(|e| e.to_string()),
            DesktopCommand::Opacity(opacity) => (|| {
                let snapshot = self.project.snapshot();
                if crate::timeline_workflow::has_sequence(&snapshot) {
                    return Err("Opacity is a legacy layer control, not a sequence edit".into());
                }
                let (reference, mut layers) = workflow::active(&snapshot)?;
                layers.foreground_opacity = opacity;
                let mut document = layers.document(reference.document)?;
                document.extensions = snapshot.state().documents[&reference.document]
                    .extensions
                    .clone();
                self.project
                    .commit(EditBatch {
                        base: snapshot.revision(),
                        mutations: vec![Mutation::PutDocument(document)],
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })(),
            other => {
                self.background(other);
                return;
            }
        };
        match result {
            Ok(()) => {
                self.overlay = None;
                self.refresh();
                self.state.status = "Edit committed".into();
            }
            Err(error) => {
                self.overlay = None;
                self.refresh();
                self.state.status = error;
            }
        }
    }
    fn request_preview(&mut self, key: PreviewKey) {
        if self.state.content.as_ref() == Some(&key.content) {
            self.preview.request(self.preview_snapshot(), key);
        }
    }
    fn cancel_preview(&mut self) {
        self.preview.cancel();
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        self.preview.take()
    }
}
