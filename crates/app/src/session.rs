//! Nonblocking project coordinator and bounded jobs. UI calls never perform I/O.
use crate::media_workflow as workflow;
use fold_media::Cancel;
use fold_platform::desktop::{
    DesktopClient, DesktopCommand, DesktopState, PreviewKey, PreviewResult,
};
use fold_project::{EditBatch, Mutation, Project, Snapshot};
use std::sync::mpsc::{self, Receiver};

#[path = "session_transport.rs"]
mod transport;
#[path = "session_view.rs"]
mod view;
#[path = "viewer_transport.rs"]
mod viewer_transport;

#[path = "preview_metadata.rs"]
mod preview_metadata;
#[path = "preview_worker.rs"]
mod preview_worker;
use preview_worker::PreviewWorker;

enum Completed {
    Ingested(Option<crate::ingest::IngestProposal>),
    Imported(EditBatch),
    Opened(Project, fold_project::Revision, Option<String>, String),
    Saved(String),
    Message(String),
}
struct Background {
    export: bool,
    completed: bool,
    cancel: Cancel,
    result: Receiver<Result<Completed, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Background {
    fn drop(&mut self) {
        if !self.completed {
            self.cancel.cancel();
        }
        if let Some(t) = self.thread.take().filter(|t| t.is_finished()) {
            let _ = t.join();
        }
        // A portal dialog may still await the user. Detached workers own only
        // immutable snapshots and cannot publish after their receiver is gone.
    }
}

pub struct Session {
    project: Project,
    state: DesktopState,
    overlay: Option<fold_project::EditSession>,
    preview: PreviewWorker,
    metadata: std::cell::RefCell<preview_metadata::Metadata>,
    background: Vec<Background>,
    imported_items: Vec<fold_project::ItemId>,
    workspace_project: Option<String>,
    workspace_epoch: u64,
    workspace_restore: bool,
    viewers: viewer_transport::Viewers,
    playback_ranges: std::collections::BTreeMap<
        fold_foundation::DocumentId,
        fold_platform::desktop::PlaybackRange,
    >,
    #[cfg(feature = "desktop")]
    playback: crate::playback::Playback,
}
impl Default for Session {
    fn default() -> Self {
        Self::new(crate::color::new_project(32))
    }
}
impl Session {
    pub fn new(project: Project) -> Self {
        let mut session = Self {
            project,
            state: DesktopState::default(),
            overlay: None,
            preview: PreviewWorker::new(),
            metadata: Default::default(),
            background: vec![],
            imported_items: Vec::new(),
            workspace_project: None,
            workspace_epoch: 0,
            workspace_restore: false,
            viewers: Default::default(),
            playback_ranges: Default::default(),
            #[cfg(feature = "desktop")]
            playback: crate::playback::Playback::default(),
        };
        session.refresh();
        session
    }
    fn refresh(&mut self) {
        self.refresh_viewers();
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
            // The shared preview host retires obsolete content demands. A global
            // navigation change must not cancel a different panel's valid request.
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
        let export = matches!(command, DesktopCommand::Export { .. });
        if self.background.iter().any(|job| job.export == export) {
            self.state.status = "A job of this kind is already running; cancel or wait.".into();
            return;
        }
        let snapshot = self.project.snapshot();
        let cancel = Cancel::default();
        let token = cancel.clone();
        let (sender, result) = mpsc::sync_channel(1);
        self.state.busy = true;
        self.state.status = "Background job running…".into();
        #[cfg(feature = "gpu")]
        let render_host = self.preview.shared.0.lock().unwrap().render_host.clone();
        let thread = std::thread::spawn(move || {
            let result = token.check().and_then(|()| match command {
                DesktopCommand::Browser(command) => {
                    crate::browser_ingest::run(&snapshot, command, &token).map(Completed::Ingested)
                }
                DesktopCommand::Extension(request) => crate::packages::builtins()
                    .stage(&snapshot, &request, &token)
                    .map(Completed::Imported),
                DesktopCommand::Import(paths) => {
                    workflow::import(&snapshot, &paths, &token).map(Completed::Imported)
                }
                DesktopCommand::Save(path) => fold_project::save(&snapshot, &path)
                    .map_err(|e| e.to_string())
                    .and_then(|_| std::fs::canonicalize(path).map_err(|e| e.to_string()))
                    .map(|path| Completed::Saved(path.to_string_lossy().into_owned())),
                DesktopCommand::Open(path) => fold_project::load(&path, 32)
                    .map_err(|e| e.to_string())
                    .and_then(|project| {
                        let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
                        Ok(Completed::Opened(
                            project,
                            snapshot.revision(),
                            None,
                            path.to_string_lossy().into_owned(),
                        ))
                    }),
                DesktopCommand::OpenInWorkspace {
                    path,
                    document_type,
                } => fold_project::load(&path, 32)
                    .map_err(|e| e.to_string())
                    .and_then(|project| {
                        let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
                        Ok(Completed::Opened(
                            project,
                            snapshot.revision(),
                            Some(document_type),
                            path.to_string_lossy().into_owned(),
                        ))
                    }),
                DesktopCommand::Export { path, start, end } => {
                    #[cfg(feature = "gpu")]
                    let result = workflow::export_with_host(
                        &snapshot,
                        &path,
                        start,
                        end,
                        &token,
                        render_host,
                    );
                    #[cfg(not(feature = "gpu"))]
                    let result = workflow::export(&snapshot, &path, start, end, &token);
                    result
                        .map(|_| Completed::Message(format!("Export complete: {}", path.display())))
                }
                _ => unreachable!(),
            });
            let _ = sender.send(result);
        });
        self.background.push(Background {
            export,
            completed: false,
            cancel,
            result,
            thread: Some(thread),
        });
    }
}
impl DesktopClient for Session {
    fn set_preview_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.preview.set_wake(wake);
    }
    fn viewer_audio_underruns(
        &self,
        viewer: fold_platform::workspace::PanelInstanceId,
    ) -> Option<u64> {
        #[cfg(feature = "desktop")]
        if self.viewers.monitor == Some(viewer) {
            return Some(self.playback.underruns());
        }
        let _ = viewer;
        None
    }
    fn resource_statistics(&self) -> String {
        let mib = |bytes: u64| bytes as f64 / 1048576.;
        let decoded = fold_media::budget::decoded_usage();
        let pipe = fold_media::budget::pipe_usage();
        let pcm = fold_media::budget::pcm_disk_usage();
        let wave = fold_media::budget::wave_copy_usage();
        let retention = self.project.retention();
        let (depth, peak) = fold_render::scheduling::Scheduler::shared().queue_depth();
        let queue = self.preview.shared.0.lock().unwrap();
        let mut report = format!(
            "Demand {}/128 · execution queue {depth} (peak {peak}) · jobs {}/2 · pressure retries {}\nDecoded CPU {:.1}/256 MiB (peak {:.1}) · pipe {:.1}/64 MiB\nSource copies {:.1}/4096 MiB · PCM disk {:.1}/512 MiB · WAVE scratch {:.1}/512 MiB\nHistory {} roots · {} external handles · {:.1} MiB unique payload (excludes metadata/allocator)",
            queue.depth(),
            self.background.len(),
            queue.pressure_retries(),
            mib(decoded.bytes),
            mib(decoded.peak),
            mib(pipe.bytes),
            mib(fold_media::pinned_source_bytes()),
            mib(pcm.bytes),
            mib(wave.bytes),
            retention.roots,
            retention.external_handles,
            mib(retention.payload_bytes as u64)
        );
        #[cfg(feature = "gpu")]
        if let Some(host) = &queue.render_host {
            let memory = host.memory();
            report.push_str(&format!("\nShared GPU {:.1}/{:.1} MiB (peak {:.1}) · working {:.1} · presentation {:.1} · decoded {:.1} · geometry {:.1} · scratch {:.1}",
                mib(memory.allocated), mib(memory.budget), mib(memory.peak), mib(memory.working),
                mib(memory.presentation), mib(memory.decoded), mib(memory.geometry), mib(memory.scratch)));
        }
        let working = fold_media::budget::working_usage();
        let output = fold_media::budget::output_usage();
        let delivery = fold_media::budget::delivery_disk_usage();
        let delivery_pcm = fold_media::budget::delivery_pcm_usage();
        report.push_str(&format!("\nCPU working {:.1}/512 MiB · output {:.1}/64 MiB · delivery scratch {:.1}/4096 MiB + PCM {:.1}/256 MiB",
            mib(working.bytes), mib(output.bytes), mib(delivery.bytes), mib(delivery_pcm.bytes)));
        report
    }
    fn viewer_request(
        &self,
        viewer: fold_platform::workspace::PanelInstanceId,
    ) -> Option<(u64, fold_foundation::Time)> {
        self.viewer_request_state(viewer)
    }
    fn present_viewer(
        &mut self,
        viewer: fold_platform::workspace::PanelInstanceId,
        generation: u64,
        key: &PreviewKey,
    ) -> bool {
        self.present_viewer_frame(viewer, generation, key)
    }
    fn viewer_audio_error(
        &self,
        viewer: fold_platform::workspace::PanelInstanceId,
    ) -> Option<String> {
        (self.viewers.monitor == Some(viewer))
            .then(|| self.viewers.audio_error.clone())
            .flatten()
    }
    fn viewer_transport(
        &self,
        viewer: fold_platform::workspace::PanelInstanceId,
    ) -> Option<fold_platform::desktop::ViewerTransport> {
        self.viewers
            .clocks
            .get(&viewer)
            .map(|c| c.transport.clone())
    }
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn output_info(&self) -> Result<(fold_foundation::DocumentId, fold_media::VideoInfo), String> {
        workflow::output(&self.project.snapshot()).map(|(source, info)| (source.document, info))
    }
    fn video_info(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Result<fold_media::VideoInfo, String> {
        crate::packages::builtins().output(&self.project.snapshot(), document)
    }
    fn workspace_project(&self) -> Option<String> {
        self.workspace_project.clone()
    }
    fn workspace_epoch(&self) -> u64 {
        self.workspace_epoch
    }
    fn workspace_restore(&self) -> bool {
        self.workspace_restore
    }
    fn outputs(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Vec<fold_platform::workspace::OutputDescriptor> {
        crate::packages::builtins().outputs(&self.project.snapshot(), document)
    }
    fn preview_state(
        &self,
        output: &fold_project::DocumentRef,
        time: fold_foundation::Time,
    ) -> DesktopState {
        let mut state = self.state.clone();
        state.playing = false;
        state.priming = false;
        state.audio_clock = false;
        state.navigation = vec![fold_platform::desktop::ViewLocation {
            document: output.document,
            time,
            label: String::new(),
        }];
        state.selection = fold_platform::desktop::Selection {
            document: Some(output.document),
            objects: vec![],
        };
        state.viewer_document = Some(output.document);
        let snapshot = self.preview_snapshot();
        let info = self.metadata.borrow_mut().output(&snapshot, output);
        state.content = None;
        match info {
            Ok(info) => {
                state.frames = info.frames;
                state.dimensions = [info.width, info.height];
                state.rate = info.rate;
                state.frame = time
                    .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
                    .unwrap_or(0)
                    .max(0) as u32;
                state.frame = state.frame.min(info.frames.saturating_sub(1));
                match self
                    .metadata
                    .borrow_mut()
                    .content(&snapshot, output.document)
                {
                    Ok(content) => state.content = Some(content),
                    Err(error) => state.status = error,
                }
                state.transient = self.overlay.is_some()
                    && state.content
                        != self
                            .metadata
                            .borrow_mut()
                            .content(&self.project.snapshot(), output.document)
                            .ok();
            }
            Err(error) => {
                state.frames = 0;
                state.frame = 0;
                state.transient = false;
                state.status = error;
            }
        }
        state
    }
    fn take_imported_items(&mut self) -> Vec<fold_project::ItemId> {
        std::mem::take(&mut self.imported_items)
    }
    fn document_kinds(&self) -> Vec<fold_platform::browser::DocumentKind> {
        crate::packages::builtins().document_kinds()
    }
    fn supports_document(&self, document: &fold_project::Document) -> bool {
        crate::packages::builtins().supports(document)
    }
    fn poll(&mut self) {
        if let Some(choices) = self.preview.shared.0.lock().unwrap().colors.take() {
            self.state.color_choices = choices;
        }
        self.poll_viewers();
        #[cfg(feature = "desktop")]
        if self.viewers.monitor.is_none() {
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
        let completed = self.background.iter().enumerate().find_map(|(index, job)| {
            match job.result.try_recv() {
                Ok(result) => Some((index, result)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((index, Err("background worker stopped".into())))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            }
        });
        if let Some((index, result)) = completed {
            let mut job = self.background.remove(index);
            let cancelled = job.cancel.check().is_err();
            // Mark completion before dropping: successful ingest proposals retain
            // their token and must not cancel themselves on worker teardown.
            job.completed = true;
            drop(job);
            self.state.busy = !self.background.is_empty();
            // Discard cancelled proposals, but never claim that an already
            // finalized save/export was rolled back by a late cancel click.
            let result = if cancelled
                && matches!(
                    &result,
                    Ok(Completed::Imported(_) | Completed::Ingested(_) | Completed::Opened(..))
                ) {
                Err("job cancelled".into())
            } else {
                result
            };
            match result {
                Ok(Completed::Ingested(proposal)) => {
                    if self.overlay.is_some() {
                        self.state.status =
                            "Finish the current edit, then retry import or relink".into();
                        return;
                    }
                    if let Some(proposal) = proposal {
                        let items = proposal.items.clone();
                        match proposal.commit(&mut self.project) {
                            Ok(_) => {
                                self.refresh();
                                self.state.status.clear();
                                self.imported_items = items;
                            }
                            Err(error) => self.state.status = error,
                        }
                    } else {
                        self.state.status.clear();
                    }
                }
                Ok(Completed::Imported(batch)) => match self.project.commit(batch) {
                    Ok(_) => {
                        self.overlay = None;
                        self.refresh();
                        self.state.status = "Import committed".into();
                    }
                    Err(e) => self.state.status = e.to_string(),
                },
                Ok(Completed::Saved(path)) => {
                    self.workspace_project = Some(path);
                    self.workspace_epoch += 1;
                    self.workspace_restore = false;
                    self.state.status = "Project saved".into();
                }
                Ok(Completed::Opened(project, base, workspace, path)) => {
                    if self.project.snapshot().revision() != base {
                        self.state.status = "Project changed while opening; retry open".into();
                    } else {
                        self.monitor_viewer(None);
                        self.viewers.clocks.clear();
                        self.project = project;
                        self.workspace_project = Some(path);
                        self.workspace_epoch += 1;
                        self.workspace_restore = true;
                        self.imported_items.clear();
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
            DesktopCommand::ViewerTransport { viewer, transport } => {
                self.configure_viewer(viewer, transport);
                return;
            }
            DesktopCommand::MonitorViewer(viewer) => {
                self.monitor_viewer(viewer);
                return;
            }
            DesktopCommand::CloseViewer(viewer) => {
                if self.viewers.monitor == Some(viewer) {
                    self.monitor_viewer(None);
                }
                self.viewers.clocks.remove(&viewer);
                return;
            }
            DesktopCommand::Notify(message) => {
                self.state.status = message;
                return;
            }
            DesktopCommand::Browser(command) => {
                use fold_platform::browser::BrowserCommand;
                if matches!(
                    command,
                    BrowserCommand::Import { .. }
                        | BrowserCommand::ChooseImport(_)
                        | BrowserCommand::Relink(_)
                ) {
                    self.background(DesktopCommand::Browser(command));
                    return;
                }
                let creating = matches!(command, BrowserCommand::Create { .. });
                crate::browser::edit(&self.project.snapshot(), command).and_then(|batch| {
                    let location = if creating {
                        batch.mutations.iter().find_map(|m| match m {
                            Mutation::PutItem(item) => match item.id {
                                fold_project::ItemId::Document(document) => {
                                    Some(fold_platform::desktop::ViewLocation {
                                        document,
                                        time: fold_foundation::Time::ZERO,
                                        label: item.name.clone(),
                                    })
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                    } else {
                        None
                    };
                    self.project.commit(batch).map_err(|e| e.to_string())?;
                    if let Some(location) = location {
                        self.navigate(location);
                    }
                    Ok(())
                })
            }
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
                for job in &self.background {
                    job.cancel.cancel();
                }
                self.state.status = "Cancellation requested".into();
                return;
            }
            DesktopCommand::SetOutputColor {
                document,
                transform,
            } => {
                if !self.state.color_choices.outputs.iter().any(|choice| {
                    choice.display == transform.display
                        && choice.view == transform.view
                        && choice.look == transform.look
                }) {
                    self.state.status = "Unavailable output color transform".into();
                    return;
                }
                crate::color::set_output(&self.project.snapshot(), document, transform).and_then(
                    |batch| {
                        self.project
                            .commit(batch)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    },
                )
            }
            DesktopCommand::SetInputColor { asset, space } => (|| {
                if !self.state.color_choices.inputs.contains(&space) {
                    return Err("Unavailable input color space".into());
                }
                let snapshot = self.project.snapshot();
                if fold_platform::color::project(&snapshot)?.is_none() {
                    return Err("Legacy project retains its original input interpretation".into());
                }
                let mut asset = snapshot
                    .state()
                    .assets
                    .get(&asset)
                    .ok_or("Missing input asset")?
                    .as_ref()
                    .clone();
                asset.extensions.insert(
                    fold_platform::color::INPUT_KEY.into(),
                    serde_json::Value::String(space),
                );
                self.project
                    .commit(EditBatch {
                        base: snapshot.revision(),
                        mutations: vec![Mutation::PutAsset(asset)],
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })(),
            DesktopCommand::SetOutput(document) => {
                workflow::select_output(&self.project.snapshot(), document).and_then(|batch| {
                    self.project
                        .commit(batch)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
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
        let snapshot = self.preview_snapshot();
        let content = key
            .target
            .map(|(id, _)| workflow::content_for(&snapshot, id))
            .unwrap_or_else(|| workflow::content(&snapshot));
        if content.as_ref().ok() == Some(&key.content) {
            self.preview.request(snapshot, key);
        }
    }
    fn preview_demands(&mut self, demands: Vec<fold_platform::desktop::PreviewDemand>) {
        self.preview.realtime_consumers(
            self.viewers
                .clocks
                .iter()
                .filter_map(|(&id, clock)| {
                    (clock.transport.playing
                        && clock.transport.mode == fold_platform::desktop::PlaybackMode::RealTime)
                        .then_some(id)
                })
                .collect(),
        );
        let snapshot = self.preview_snapshot();
        let valid = demands
            .into_iter()
            .take(128)
            .filter(|demand| {
                demand.key.target.is_some_and(|(id, _)| {
                    self.metadata
                        .borrow_mut()
                        .content(&snapshot, id)
                        .ok()
                        .as_ref()
                        == Some(&demand.key.content)
                })
            })
            .collect();
        self.preview.demands(snapshot, valid);
    }
    fn begin_preview_update(&mut self) {
        self.preview.begin_update();
    }
    fn end_preview_update(&mut self) {
        self.preview.end_update();
    }
    fn cancel_preview(&mut self) {
        self.preview.cancel();
    }
    #[cfg(feature = "gpu")]
    fn set_render_host(&mut self, host: fold_render::gpu::Host) {
        self.preview.cancel();
        self.preview.shared.0.lock().unwrap().render_host = Some(host);
    }
    #[cfg(feature = "gpu")]
    fn take_gpu_preview(&mut self) -> Option<fold_platform::desktop::GpuPreviewResult> {
        self.preview.take_gpu()
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        self.preview.take()
    }
}
