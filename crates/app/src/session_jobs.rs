//! File/ingest/export worker admission and lifecycle. Workers own immutable roots;
//! completion is data, and only Session may publish it.
use crate::output as workflow;
use fold_media::Cancel;
use fold_platform::desktop::DesktopCommand;
use fold_project::{EditBatch, Project};
use std::sync::mpsc::{self, Receiver};

pub(super) enum Completed {
    Ingested(Option<crate::ingest::IngestProposal>),
    Edited(EditBatch),
    Opened(Project, fold_project::Revision, Option<String>, String),
    Saved(String, fold_project::CommittedSnapshot),
    Cancelled,
    Exported(String),
}

#[derive(Clone)]
pub(super) enum Kind {
    Open,
    Save,
    Ingest,
    Extension(fold_platform::packages::CommandRequest),
    Export,
}
impl Kind {
    fn for_command(command: &DesktopCommand) -> Result<Self, String> {
        Ok(match command {
            DesktopCommand::ChooseOpen
            | DesktopCommand::Open(_)
            | DesktopCommand::OpenInWorkspace { .. } => Self::Open,
            DesktopCommand::ChooseSave | DesktopCommand::Save(_) => Self::Save,
            DesktopCommand::Browser(_) => Self::Ingest,
            DesktopCommand::Extension(request) => Self::Extension(request.clone()),
            DesktopCommand::Export { .. } => Self::Export,
            _ => return Err("command is not a background operation".into()),
        })
    }
    pub fn project_file(&self) -> bool {
        matches!(self, Self::Open | Self::Save)
    }
    fn export(&self) -> bool {
        matches!(self, Self::Export)
    }
}
#[derive(PartialEq)]
enum Lifecycle {
    Running,
    Cancelling,
    Completed,
}
struct Worker {
    kind: Kind,
    epoch: u64,
    lifecycle: Lifecycle,
    cancel: Cancel,
    result: Receiver<Result<Completed, String>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    fn cancel(&mut self) {
        self.lifecycle = Lifecycle::Cancelling;
        self.cancel.cancel();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if self.lifecycle != Lifecycle::Completed {
            self.cancel.cancel();
        }
        if let Some(thread) = self.thread.take().filter(|thread| thread.is_finished()) {
            let _ = thread.join();
        }
        // Portal dialogs cannot be interrupted. Detached workers have no writer
        // and their receiver is discarded on Session shutdown.
    }
}
pub(super) struct Completion {
    pub kind: Kind,
    pub epoch: u64,
    pub result: Result<Completed, String>,
}
#[derive(Default)]
pub(super) struct Jobs {
    workers: Vec<Worker>,
}
impl Jobs {
    pub fn len(&self) -> usize {
        self.workers.len()
    }
    pub fn busy(&self) -> bool {
        !self.workers.is_empty()
    }
    pub fn file_busy(&self) -> bool {
        self.workers.iter().any(|job| !job.kind.export())
    }
    pub fn cancel(&mut self) {
        for job in &mut self.workers {
            job.cancel();
        }
    }
    pub fn extension_pending(&self, request: &fold_platform::packages::CommandRequest) -> bool {
        self.workers.iter().any(|job| matches!(&job.kind, Kind::Extension(r) if r.id == request.id && r.base == request.base && r.arguments == request.arguments))
    }
    pub fn cancel_extension(&mut self, request: &fold_platform::packages::CommandRequest) {
        for job in &mut self.workers {
            if matches!(&job.kind, Kind::Extension(r) if r.id == request.id && r.base == request.base && r.arguments == request.arguments)
            {
                job.cancel();
            }
        }
    }
    pub fn replace_project(&mut self) {
        // Retain admission until workers acknowledge cancellation. In particular,
        // a save already at atomic publication must finish before another save
        // can target the same path. Portal dialogs may still await dismissal.
        self.cancel();
    }
    pub fn poll(&mut self) -> Option<Completion> {
        let (index, result) =
            self.workers.iter().enumerate().find_map(|(index, job)| {
                match job.result.try_recv() {
                    Ok(result) => Some((index, result)),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some((index, Err("background worker stopped".into())))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            })?;
        let mut job = self.workers.remove(index);
        let result = if job.lifecycle == Lifecycle::Cancelling
            && matches!(
                &result,
                Ok(Completed::Edited(_) | Completed::Ingested(_) | Completed::Opened(..))
            ) {
            Err("job cancelled".into())
        } else {
            result
        };
        // A finalized save/export remains successful after a late cancel. Ingest
        // proposals retain the token: successful teardown must not cancel them.
        job.lifecycle = Lifecycle::Completed;
        Some(Completion {
            kind: job.kind.clone(),
            epoch: job.epoch,
            result,
        })
    }
    pub fn start(
        &mut self,
        command: DesktopCommand,
        snapshot: fold_project::CommittedSnapshot,
        suggested_path: Option<std::path::PathBuf>,
        epoch: u64,
        #[cfg(feature = "gpu")] render_host: Option<fold_render::gpu::Host>,
    ) -> Result<Kind, String> {
        let kind = Kind::for_command(&command)?;
        if self
            .workers
            .iter()
            .any(|job| job.kind.export() == kind.export())
        {
            return Err("A job of this kind is already running; cancel or wait.".into());
        }
        let cancel = Cancel::default();
        let token = cancel.clone();
        let (sender, result) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let result = execute(
                command,
                &snapshot,
                suggested_path.as_deref(),
                &token,
                #[cfg(feature = "gpu")]
                render_host,
            );
            let _ = sender.send(result);
        });
        self.workers.push(Worker {
            kind: kind.clone(),
            epoch,
            lifecycle: Lifecycle::Running,
            cancel,
            result,
            thread: Some(thread),
        });
        Ok(kind)
    }
}
fn execute(
    command: DesktopCommand,
    snapshot: &fold_project::CommittedSnapshot,
    suggested_path: Option<&std::path::Path>,
    token: &Cancel,
    #[cfg(feature = "gpu")] render_host: Option<fold_render::gpu::Host>,
) -> Result<Completed, String> {
    token.check().map_err(String::from).and_then(|()| {
        let command = match command {
            DesktopCommand::ChooseOpen | DesktopCommand::ChooseSave => {
                let save = matches!(command, DesktopCommand::ChooseSave);
                let Some(path) = choose_project(save, suggested_path)? else {
                    return Ok(Completed::Cancelled);
                };
                token.check()?;
                if save {
                    DesktopCommand::Save(path)
                } else {
                    DesktopCommand::Open(path)
                }
            }
            other => other,
        };
        match command {
            DesktopCommand::Browser(command) => {
                crate::browser_ingest::run(snapshot, command, token).map(Completed::Ingested)
            }
            DesktopCommand::Extension(request) => crate::packages::builtins()
                .stage(snapshot, &request, token)
                .map(Completed::Edited),
            DesktopCommand::Save(path) => fold_project::save_cancellable(snapshot, &path, &|| {
                token.check().map_err(String::from)
            })
            .map_err(|e| e.to_string())
            .and_then(|_| std::fs::canonicalize(path).map_err(|e| e.to_string()))
            .map(|path| Completed::Saved(path.to_string_lossy().into_owned(), snapshot.clone())),
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
                let result =
                    workflow::export_with_host(snapshot, &path, start, end, token, render_host);
                #[cfg(not(feature = "gpu"))]
                let result = workflow::export(snapshot, &path, start, end, token);
                result.map(|_| Completed::Exported(format!("Export complete: {}", path.display())))
            }
            _ => Err("command is not a background operation".into()),
        }
    })
}

fn choose_project(
    save: bool,
    current: Option<&std::path::Path>,
) -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(feature = "desktop")]
    {
        let mut dialog = rfd::FileDialog::new().add_filter("Fold project", &["fold"]);
        if let Some(parent) = current.and_then(std::path::Path::parent) {
            dialog = dialog.set_directory(parent);
        }
        Ok(if save {
            dialog
                .set_file_name(
                    current
                        .and_then(std::path::Path::file_name)
                        .map(|name| name.to_string_lossy())
                        .unwrap_or_else(|| "Untitled.fold".into()),
                )
                .save_file()
        } else {
            dialog.pick_file()
        })
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (save, current);
        Err("Native project dialogs require desktop support".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(kind: Kind) -> (Jobs, mpsc::SyncSender<Result<Completed, String>>, Cancel) {
        let (sender, result) = mpsc::sync_channel(1);
        let cancel = Cancel::default();
        let jobs = Jobs {
            workers: vec![Worker {
                kind,
                epoch: 7,
                lifecycle: Lifecycle::Running,
                cancel: cancel.clone(),
                result,
                thread: None,
            }],
        };
        (jobs, sender, cancel)
    }

    #[test]
    fn cancelled_open_is_discarded_but_finalized_save_and_export_are_reported() {
        let project = Project::new(8);
        let snapshot = project.snapshot();
        let (mut jobs, sender, _) = held(Kind::Open);
        sender
            .send(Ok(Completed::Opened(
                project,
                snapshot.revision(),
                None,
                "old.fold".into(),
            )))
            .ok()
            .unwrap();
        jobs.cancel();
        assert!(jobs.poll().unwrap().result.is_err());
        assert!(!jobs.busy());
        for (kind, result) in [
            (
                Kind::Save,
                Completed::Saved("saved.fold".into(), snapshot.clone()),
            ),
            (Kind::Export, Completed::Exported("finalized".into())),
        ] {
            let (mut jobs, sender, _) = held(kind);
            sender.send(Ok(result)).ok().unwrap();
            jobs.cancel();
            assert!(jobs.poll().unwrap().result.is_ok());
        }
    }

    #[test]
    fn cancellation_prevents_open_save_export_and_dialog_execution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.fold");
        let snapshot = Project::new(8).snapshot();
        let token = Cancel::default();
        token.cancel();
        for command in [
            DesktopCommand::Open(path.clone()),
            DesktopCommand::Save(path.clone()),
            DesktopCommand::Export {
                path: path.clone(),
                start: 0,
                end: 1,
            },
            DesktopCommand::ChooseOpen,
            DesktopCommand::ChooseSave,
        ] {
            let result = execute(
                command,
                &snapshot,
                None,
                &token,
                #[cfg(feature = "gpu")]
                None,
            );
            assert!(matches!(result, Err(error) if error == "job cancelled"));
        }
        assert!(!path.exists());
    }

    #[test]
    fn replacement_cancels_running_worker_but_holds_admission_until_completion() {
        let (mut jobs, sender, token) = held(Kind::Export);
        let (release, wait) = mpsc::sync_channel(0);
        let thread = std::thread::spawn(move || {
            wait.recv().unwrap();
            assert!(token.check().is_err());
            assert!(
                sender
                    .send(Ok(Completed::Exported("old export".into())))
                    .is_ok()
            );
        });
        jobs.replace_project();
        assert!(jobs.busy());
        release.send(()).unwrap();
        thread.join().unwrap();
        assert_eq!(jobs.poll().unwrap().epoch, 7);
        assert!(!jobs.busy());
    }

    #[test]
    fn successful_proposal_teardown_keeps_its_token_and_slots_remain_bounded() {
        let (mut jobs, sender, token) = held(Kind::Ingest);
        let snapshot = Project::new(8).snapshot();
        assert!(
            jobs.start(
                DesktopCommand::Save("unused.fold".into()),
                snapshot,
                None,
                7,
                #[cfg(feature = "gpu")]
                None,
            )
            .is_err()
        );
        sender.send(Ok(Completed::Ingested(None))).ok().unwrap();
        assert!(jobs.poll().unwrap().result.is_ok());
        assert!(token.check().is_ok());
    }
}
