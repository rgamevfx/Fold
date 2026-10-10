//! Bounded sidecar I/O worker. No filesystem work on redraw or in feature panels.
use fold_platform::workspace::Workspace;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
};

enum Request {
    Load(u64, String),
    Save(Workspace),
}
pub(crate) struct Completion {
    pub load_ticket: Option<u64>,
    pub project: String,
    pub value: Result<Option<Workspace>, String>,
}
pub(crate) struct WorkspaceStore {
    sender: Option<SyncSender<Request>>,
    result: Receiver<Completion>,
    pending: VecDeque<Request>,
    load_ticket: u64,
    thread: Option<JoinHandle<()>>,
}
fn sidecar(root: &std::path::Path, project: &str) -> PathBuf {
    // Stable filename; the archive also checks the full canonical association.
    let hash = project.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    root.join(format!("{hash:016x}.json"))
}
impl WorkspaceStore {
    pub fn new() -> Self {
        let root = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .map(|root| root.join("fold/workspaces"));
        Self::at(root)
    }
    fn at(root: Option<PathBuf>) -> Self {
        let (sender, requests) = mpsc::sync_channel(1);
        let (publish, result) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let Some(root) = &root else {
                    let (ticket, project) = match request {
                        Request::Load(ticket, project) => (Some(ticket), project),
                        Request::Save(workspace) => (None, workspace.project),
                    };
                    let _ = publish.send(Completion {
                        load_ticket: ticket,
                        project,
                        value: Err("No workspace state directory is configured".into()),
                    });
                    continue;
                };
                match request {
                    Request::Load(ticket, project) => {
                        let path = sidecar(root, &project);
                        let value = match std::fs::metadata(&path) {
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                            Err(e) => Err(e.to_string()),
                            Ok(m) if m.len() > 2 * 1024 * 1024 => {
                                Err("Workspace exceeds size limit".into())
                            }
                            Ok(_) => std::fs::read(&path)
                                .map_err(|e| e.to_string())
                                .and_then(|bytes| Workspace::decode(&bytes, &project).map(Some)),
                        };
                        let _ = publish.send(Completion {
                            load_ticket: Some(ticket),
                            project,
                            value,
                        });
                    }
                    Request::Save(workspace) => {
                        let save = || -> Result<(), String> {
                            std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
                            let bytes =
                                serde_json::to_vec(&workspace).map_err(|e| e.to_string())?;
                            if bytes.len() > 2 * 1024 * 1024 {
                                return Err("Workspace exceeds size limit".into());
                            }
                            let path = sidecar(root, &workspace.project);
                            let temporary =
                                path.with_extension(format!("{}.tmp", std::process::id()));
                            std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
                            std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
                            Ok(())
                        };
                        if let Err(error) = save() {
                            let _ = publish.send(Completion {
                                load_ticket: None,
                                project: workspace.project,
                                value: Err(error),
                            });
                        }
                    }
                }
            }
        });
        Self {
            sender: Some(sender),
            result,
            pending: VecDeque::new(),
            load_ticket: 0,
            thread: Some(thread),
        }
    }
    pub fn load(&mut self, project: String) {
        self.invalidate_load();
        self.pending
            .push_back(Request::Load(self.load_ticket, project));
        self.flush();
    }
    pub fn invalidate_load(&mut self) {
        self.load_ticket += 1;
        self.pending
            .retain(|request| !matches!(request, Request::Load(..)));
    }
    pub fn save(&mut self, workspace: Workspace) {
        self.pending.retain(
            |request| !matches!(request, Request::Save(old) if old.project == workspace.project),
        );
        // Only the latest two project associations need shutdown retention.
        if self.pending.len() >= 4
            && let Some(index) = self
                .pending
                .iter()
                .position(|r| matches!(r, Request::Save(_)))
        {
            self.pending.remove(index);
        }
        self.pending.push_back(Request::Save(workspace));
        self.flush();
    }
    pub fn flush(&mut self) {
        while let Some(request) = self.pending.pop_front() {
            match self.sender.as_ref().unwrap().try_send(request) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(request)) => {
                    self.pending.push_front(request);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {}
            }
        }
    }
    pub fn take(&mut self) -> Option<Completion> {
        self.flush();
        while let Ok(completion) = self.result.try_recv() {
            if completion
                .load_ticket
                .is_none_or(|ticket| ticket == self.load_ticket)
            {
                return Some(completion);
            }
        }
        None
    }
}
#[cfg(test)]
#[path = "workspace_store_tests.rs"]
mod tests;

impl Drop for WorkspaceStore {
    fn drop(&mut self) {
        while let Some(request) = self.pending.pop_front() {
            let _ = self.sender.as_ref().unwrap().send(request);
        }
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
