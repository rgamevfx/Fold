//! Nonblocking project coordinator and bounded jobs. UI calls never perform I/O.
use crate::output as workflow;
use fold_platform::desktop::{
    DesktopClient, DesktopCommand, DesktopState, PreviewKey, PreviewResult,
};
use fold_project::{Project, Snapshot};

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

#[path = "session_agent.rs"]
mod agent_transport;
#[path = "session_commands.rs"]
mod commands;
#[path = "session_jobs.rs"]
mod jobs;
use jobs::Completed;

pub struct Session {
    settings_request: Option<Option<fold_foundation::DocumentId>>,
    project: Project,
    agent: agent_transport::Agent,
    saved_project: fold_project::CommittedSnapshot,
    state: DesktopState,
    overlay: Option<fold_project::EditSession>,
    preview: PreviewWorker,
    metadata: std::cell::RefCell<preview_metadata::Metadata>,
    jobs: jobs::Jobs,
    project_epoch: u64,
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
            settings_request: None,
            saved_project: crate::color::new_project(32).snapshot(),
            project,
            agent: Default::default(),
            state: DesktopState::default(),
            overlay: None,
            preview: PreviewWorker::new(),
            metadata: Default::default(),
            jobs: Default::default(),
            project_epoch: 0,
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
    #[cfg(target_os = "linux")]
    pub fn enable_agent_transport(
        &mut self,
        parent: &std::path::Path,
    ) -> std::io::Result<std::path::PathBuf> {
        self.agent.enable(parent)
    }

    /// Native owner entry point shared by local protocol adapters. Workers retain
    /// committed snapshots; only this owner can apply their inspected proposals.
    pub fn dispatch_agent(&mut self, bytes: &[u8]) -> Vec<u8> {
        let outcome = self.agent.dispatch(
            &mut self.project,
            bytes,
            self.overlay.is_some(),
            #[cfg(feature = "gpu")]
            self.preview.shared.0.lock().unwrap().render_host.clone(),
        );
        if outcome.changed {
            self.refresh();
        }
        outcome.response
    }
    fn refresh_history(&mut self) {
        self.state.dirty = !self.project.snapshot().same_content(&self.saved_project);
        self.state.can_undo = self.project.can_undo();
        self.state.can_redo = self.project.can_redo();
    }
    fn refresh(&mut self) {
        self.refresh_history();
        self.refresh_output();
    }
    fn refresh_output(&mut self) {
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
        let output = if let Some(location) = self.state.navigation.last() {
            let registry = crate::packages::builtins();
            registry
                .default_output(&snapshot, location.document)
                .and_then(|source| {
                    self.metadata
                        .borrow_mut()
                        .output(&snapshot, &source)
                        .map(|info| (source, info))
                })
        } else {
            workflow::reference(&snapshot).and_then(|source| {
                self.metadata
                    .borrow_mut()
                    .output(&snapshot, &source)
                    .map(|info| (source, info))
            })
        };
        let content = if let Some(location) = self.state.navigation.last() {
            self.metadata
                .borrow_mut()
                .content(&snapshot, location.document)
                .ok()
        } else {
            output.as_ref().ok().and_then(|(source, _)| {
                self.metadata
                    .borrow_mut()
                    .content(&snapshot, source.document)
                    .ok()
            })
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
        self.playback_ranges
            .retain(|id, _| committed.state().documents.contains_key(id));
        self.state.viewer_document = output.as_ref().ok().map(|(source, _)| source.document);
        self.state.viewer_output = output
            .as_ref()
            .ok()
            .map(|(source, _)| source.output.clone());
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
    }
    fn replace_project(&mut self, project: Project) {
        self.project_epoch += 1;
        self.jobs.replace_project();
        self.monitor_viewer(None);
        self.stop_playback();
        self.viewers.clocks.clear();
        self.cancel_preview();
        self.metadata = Default::default();
        self.saved_project = project.snapshot();
        self.project = project;
        self.agent.replace_project();
        self.imported_items.clear();
        self.playback_ranges.clear();
        self.overlay = None;
        self.state.navigation.clear();
        self.refresh_job_state();
    }
    fn background(&mut self, command: DesktopCommand) {
        let result = self.jobs.start(
            command,
            self.project.snapshot(),
            self.state.project_path.clone(),
            self.project_epoch,
            #[cfg(feature = "gpu")]
            self.preview.shared.0.lock().unwrap().render_host.clone(),
        );
        match result {
            Ok(kind) => {
                if kind.project_file() {
                    self.state.project_error = None;
                }
                self.refresh_job_state();
                self.state.status = "Background job running…".into();
            }
            Err(error) => self.state.status = error,
        }
    }
    fn refresh_job_state(&mut self) {
        self.state.busy = self.jobs.busy();
        self.state.file_busy = self.jobs.file_busy();
    }

    fn accept_completion(&mut self, completion: jobs::Completion) {
        self.refresh_job_state();
        if completion.epoch != self.project_epoch {
            return;
        }
        let project_file = completion.kind.project_file();
        let result = completion.result;
        if project_file {
            self.state.file_result_serial += 1;
            self.state.project_error = result.as_ref().err().cloned();
        }
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
            Ok(Completed::Edited(batch)) if batch.mutations.is_empty() => self.state.status.clear(),
            Ok(Completed::Edited(_)) if self.overlay.is_some() => {
                self.state.status = "Finish the current edit, then retry import or relink".into();
            }
            Ok(Completed::Edited(batch)) => match self.project.commit(batch) {
                Ok(_) => {
                    self.overlay = None;
                    self.refresh();
                    self.state.status.clear();
                }
                Err(e) => self.state.status = e.to_string(),
            },
            Ok(Completed::Cancelled) => self.state.status.clear(),
            Ok(Completed::Saved(path, snapshot)) => {
                self.saved_project = snapshot;
                self.state.project_path = Some(path.clone().into());
                self.state.save_serial += 1;
                self.refresh_history();
                self.workspace_project = Some(path);
                self.workspace_epoch += 1;
                self.workspace_restore = false;
                self.state.status = "Project saved".into();
            }
            Ok(Completed::Opened(project, base, workspace, path)) => {
                if self.project.snapshot().revision() != base || self.overlay.is_some() {
                    self.state.status = "Project changed while opening; retry open".into();
                    self.state.project_error = Some(self.state.status.clone());
                } else {
                    self.replace_project(project);
                    self.state.project_path = Some(path.clone().into());
                    self.workspace_project = Some(path);
                    self.workspace_epoch += 1;
                    self.workspace_restore = true;
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
            Ok(Completed::Exported(message)) => self.state.status = message,
            Err(error) => self.state.status = error,
        }
    }
}
impl DesktopClient for Session {
    fn take_settings_request(&mut self) -> Option<Option<fold_foundation::DocumentId>> {
        self.settings_request.take()
    }
    fn document_settings(
        &self,
        id: fold_foundation::DocumentId,
    ) -> Result<Option<fold_platform::settings::DocumentSettings>, String> {
        let snapshot = self.project.snapshot();
        let document = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("Missing document")?;
        let registry = crate::packages::builtins();
        if !registry.has_capability(document, fold_platform::packages::Capability::Settings) {
            return Ok(None);
        }
        registry.document_settings(document)
    }

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
            "Demand {}/128 · execution queue {depth} (peak {peak}) · jobs {}/2 · pressure retries {}\nDecoded CPU {:.1}/{:.1} MiB (peak {:.1}) · pipe {:.1}/64 MiB\nSource copies {:.1}/4096 MiB · PCM disk {:.1}/512 MiB · WAVE scratch {:.1}/512 MiB\nHistory {} roots · {} external handles · {:.1} MiB unique payload (excludes metadata/allocator)",
            queue.depth(),
            self.jobs.len(),
            queue.pressure_retries(),
            mib(decoded.bytes),
            mib(decoded.budget),
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
        report.push_str(&format!("\nCPU working {:.1}/{:.1} MiB · output {:.1}/{:.1} MiB · delivery scratch {:.1}/4096 MiB + PCM {:.1}/256 MiB",
            mib(working.bytes), mib(working.budget), mib(output.bytes), mib(output.budget), mib(delivery.bytes), mib(delivery_pcm.bytes)));
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
    fn extension_pending(&self, request: &fold_platform::packages::CommandRequest) -> bool {
        self.jobs.extension_pending(request)
    }

    fn effect_interface(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Result<fold_platform::effects::Interface, String> {
        crate::packages::builtins().effect_interface(&self.project.snapshot(), document)
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
    fn channels(
        &self,
        output: &fold_project::DocumentRef,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        crate::packages::builtins().channels(&self.project.snapshot(), output)
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
        state.viewer_output = Some(output.output.clone());
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
    fn hierarchy(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Result<fold_platform::hierarchy::Hierarchy, String> {
        let snapshot = self.project.snapshot();
        let document = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("Missing hierarchy document")?;
        let registry = crate::packages::builtins();
        if !registry.has_capability(document, fold_platform::packages::Capability::Hierarchy) {
            return Ok(Default::default());
        }
        registry.hierarchy(document)
    }
    fn animation_channels(
        &self,
        document: fold_foundation::DocumentId,
        object: Option<fold_foundation::ObjectId>,
    ) -> Result<Vec<fold_platform::animation::Channel>, String> {
        let snapshot = self.project.snapshot();
        let document = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("Missing animation document")?;
        let registry = crate::packages::builtins();
        if !registry.has_capability(document, fold_platform::packages::Capability::Animation) {
            return Ok(vec![]);
        }
        registry.animation_channels(document, object)
    }
    fn image_inputs(
        &self,
        document: fold_foundation::DocumentId,
    ) -> Result<Vec<fold_platform::layer_graph::ImageInput>, String> {
        let snapshot = self.project.snapshot();
        let document = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("Missing graph")?;
        let registry = crate::packages::builtins();
        if !registry.has_capability(document, fold_platform::packages::Capability::ImageInputs) {
            return Ok(vec![]);
        }
        registry.document_image_inputs(document)
    }
    fn timeline_cache(&self, key: &fold_platform::desktop::PreviewKey) -> bool {
        let snapshot = self.project.snapshot();
        key.target
            .map(|t| t.0)
            .or(self.state.viewer_document)
            .and_then(|id| snapshot.state().documents.get(&id))
            .is_some_and(|document| document.type_id == fold_timeline::SEQUENCE)
    }
    fn document_kinds(&self) -> Vec<fold_platform::browser::DocumentKind> {
        crate::packages::builtins().document_kinds()
    }
    fn has_network(&self, document: &fold_project::Document) -> bool {
        self.supports_document(document)
            && matches!(
                document.type_id.as_str(),
                fold_compositor::COMPOSITE | fold_motion::CONSTRUCTION | fold_motion::scene::TYPE
            )
    }
    fn supports_document(&self, document: &fold_project::Document) -> bool {
        crate::packages::builtins().supports(document)
    }
    fn poll(&mut self) {
        self.agent.poll();
        #[cfg(target_os = "linux")]
        for _ in 0..8 {
            let request = match self.agent.next() {
                agent_transport::Incoming::Idle => break,
                agent_transport::Incoming::Deferred => continue,
                agent_transport::Incoming::Request(request) => request,
            };
            let response = self.dispatch_agent(&request.bytes);
            let _ = request.response.try_send(response);
        }
        if let Some(choices) = self.preview.shared.0.lock().unwrap().colors.take() {
            self.state.color_choices = choices;
        }
        self.poll_viewers();
        self.poll_playback();
        if let Some(completion) = self.jobs.poll() {
            self.accept_completion(completion);
        }
    }

    fn command(&mut self, command: DesktopCommand) {
        if let commands::Outcome::Edit(result) = self.execute_command(command) {
            self.overlay = None;
            self.refresh();
            self.state.status = result.err().unwrap_or_else(|| "Edit committed".into());
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
        let valid: Vec<_> = demands
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
        self.preview.preparation.replace(
            valid
                .iter()
                .filter_map(|demand| {
                    let clock = self.viewers.clocks.get(&demand.consumer)?;
                    (!demand.background && clock.transport.playing).then(|| {
                        preview_worker::source_preparation::Request {
                            snapshot: snapshot.clone(),
                            key: demand.key.clone(),
                            transport: clock.transport.clone(),
                        }
                    })
                })
                .collect(),
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_worker_outcomes_are_rejected_after_replacement_even_at_the_same_revision() {
        let mut session = Session::default();
        let old = session.project.snapshot();
        let epoch = session.project_epoch;
        session.command(DesktopCommand::NewProject);
        assert_eq!(old.revision(), session.project.snapshot().revision());
        let status = session.state.status.clone();
        let outcomes = [
            (
                jobs::Kind::Save,
                Completed::Saved("old.fold".into(), old.clone()),
            ),
            (
                jobs::Kind::Open,
                Completed::Opened(Project::new(8), old.revision(), None, "old.fold".into()),
            ),
            (
                jobs::Kind::Ingest,
                Completed::Edited(fold_project::EditBatch {
                    base: old.revision(),
                    mutations: vec![fold_project::Mutation::SetSettings(Default::default())],
                }),
            ),
            (
                jobs::Kind::Export,
                Completed::Exported("old export completed".into()),
            ),
        ];
        for (kind, result) in outcomes {
            session.accept_completion(jobs::Completion {
                kind,
                epoch,
                result: Ok(result),
            });
            assert_eq!(session.state.status, status);
            assert!(session.state.project_path.is_none());
            assert_eq!(session.state.save_serial, 0);
            assert_eq!(session.state.file_result_serial, 0);
            assert!(!session.state.dirty && !session.state.can_undo);
        }
    }
}
