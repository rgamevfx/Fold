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
    Opened(Project, fold_project::Revision),
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
    preview: PreviewWorker,
    background: Option<Background>,
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
            preview: PreviewWorker::new(),
            background: None,
        };
        session.refresh();
        session
    }
    fn refresh(&mut self) {
        let snapshot = self.project.snapshot();
        let content = workflow::content(&snapshot).ok();
        if self.state.content != content {
            self.preview.cancel();
        }
        self.state.content = content;
        if let Ok((_, layers)) = workflow::active(&snapshot) {
            self.state.frames = layers.info.frames;
            self.state.dimensions = [layers.info.width, layers.info.height];
            self.state.rate = layers.info.rate;
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
        self.state.status = "Background job running (video only)…".into();
        let thread = std::thread::spawn(move || {
            let result = token.check().and_then(|()| match command {
                DesktopCommand::Import(paths) => {
                    workflow::import(&snapshot, &paths, &token).map(Completed::Imported)
                }
                DesktopCommand::Save(path) => fold_project::save(&snapshot, &path)
                    .map(|_| Completed::Message("Project saved".into()))
                    .map_err(|e| e.to_string()),
                DesktopCommand::Open(path) => fold_project::load(&path, 32)
                    .map(|project| Completed::Opened(project, snapshot.revision()))
                    .map_err(|e| e.to_string()),
                DesktopCommand::Export { path, start, end } => {
                    workflow::export(&snapshot, &path, start, end, &token).map(|_| {
                        Completed::Message(format!(
                            "Export complete: {} (video only)",
                            path.display()
                        ))
                    })
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
    fn poll(&mut self) {
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
                        self.refresh();
                        self.state.status =
                            "Imported video layers. SDR BT.709 → sRGB; audio ignored.".into();
                    }
                    Err(e) => self.state.status = e.to_string(),
                },
                Ok(Completed::Opened(project, base)) => {
                    if self.project.snapshot().revision() != base {
                        self.state.status = "Project changed while opening; retry open".into();
                    } else {
                        self.project = project;
                        self.refresh();
                        self.state.status = "Project opened".into();
                    }
                }
                Ok(Completed::Message(message)) => self.state.status = message,
                Err(error) => self.state.status = error,
            }
        }
    }
    fn command(&mut self, command: DesktopCommand) {
        let result = match command {
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
                self.refresh();
                self.state.status = "Edit committed".into();
            }
            Err(error) => self.state.status = error,
        }
    }
    fn request_preview(&mut self, key: PreviewKey) {
        if self.state.content.as_ref() == Some(&key.content) {
            self.preview.request(self.project.snapshot(), key);
        }
    }
    fn cancel_preview(&mut self) {
        self.preview.cancel();
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        self.preview.take()
    }
}
