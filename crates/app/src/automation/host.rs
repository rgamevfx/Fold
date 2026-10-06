//! Bounded jobs and replay records owned by the application session.
use super::*;
use fold_media::Cancel;
use fold_project::{CommittedSnapshot, EditBatch};
use std::{collections::BTreeMap, sync::mpsc};
pub enum Outcome {
    Value(Value),
    Candidate {
        id: u64,
        expected: u64,
        candidate: fold_project::Candidate,
        result: Value,
    },
    Edit {
        batch: EditBatch,
        result: Value,
        validate_only: bool,
    },
    Saved {
        snapshot: CommittedSnapshot,
        path: std::path::PathBuf,
    },
}
pub struct Job {
    pub cancel: Cancel,
    pub receiver: Option<mpsc::Receiver<Result<Outcome, String>>>,
    pub result: Option<Value>,
    pub epoch: u64,
}
pub struct CandidateEntry {
    pub value: fold_project::Candidate,
    pub epoch: u64,
    pub touched: std::time::Instant,
}
#[derive(Clone)]
pub struct CandidatePreview {
    pub candidate: u64,
    pub generation: u64,
    pub source: fold_project::DocumentRef,
    pub time: fold_foundation::Time,
    pub original: bool,
}
pub struct Host {
    pub preview: Option<CandidatePreview>,
    pub candidates: BTreeMap<u64, CandidateEntry>,
    next_candidate: u64,
    pub server: Option<wire::Server>,
    pub starting: Option<mpsc::Receiver<Result<wire::Server, String>>>,
    pub jobs: BTreeMap<u64, Job>,
    pub captures: Vec<fold_platform::desktop::AgentCapture>,
    pub viewers: Value,
    pub last_error: Option<String>,
    pub saving: Option<u64>,
    next: u64,
    replay: BTreeMap<String, (String, Value)>,
    replay_bytes: usize,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            preview: None,
            candidates: BTreeMap::new(),
            next_candidate: 0,
            server: None,
            starting: None,
            jobs: BTreeMap::new(),
            captures: Vec::new(),
            viewers: json!([]),
            last_error: None,
            saving: None,
            next: 0,
            replay: BTreeMap::new(),
            replay_bytes: 0,
        }
    }
}
impl Host {
    pub fn expire_candidates(&mut self, epoch: u64) {
        self.candidates
            .retain(|_, c| c.epoch == epoch && c.touched.elapsed().as_secs() < 900);
    }
    pub fn candidate(
        &mut self,
        id: u64,
        generation: u64,
    ) -> Result<fold_project::Candidate, String> {
        let entry = self
            .candidates
            .get_mut(&id)
            .ok_or("candidate expired or missing; begin again")?;
        if entry.value.generation() != generation {
            return Err("candidate generation conflict; inspect current candidate".into());
        }
        entry.touched = std::time::Instant::now();
        Ok(entry.value.clone())
    }
    pub fn begin_candidate(
        &mut self,
        snapshot: CommittedSnapshot,
        epoch: u64,
    ) -> Result<Value, String> {
        if self.candidates.len() >= 4 {
            return Err("four candidates retained; discard or commit one".into());
        }
        self.next_candidate += 1;
        let id = self.next_candidate;
        let base = snapshot.revision();
        self.candidates.insert(
            id,
            CandidateEntry {
                value: fold_project::Candidate::new(snapshot),
                epoch,
                touched: std::time::Instant::now(),
            },
        );
        Ok(
            json!({"candidate":id,"generation":0,"revision":base,"base_revision":base,"project_epoch":epoch,"expires_after_idle_seconds":900}),
        )
    }
    pub fn enable(&mut self) {
        if self.server.is_some() || self.starting.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.starting = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(wire::Server::start());
        });
    }
    pub fn disable(&mut self) {
        self.server = None;
        self.starting = None;
        for job in self.jobs.values() {
            job.cancel.cancel();
        }
        self.jobs.clear();
        self.candidates.clear();
        self.preview = None;
        self.saving = None;
        self.captures.clear();
        self.replay.clear();
        self.replay_bytes = 0;
    }
    pub fn poll_start(&mut self) {
        if let Some(result) = self.starting.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.starting = None;
            match result {
                Ok(server) => {
                    self.server = Some(server);
                    self.last_error = None;
                }
                Err(e) => self.last_error = Some(e),
            }
        }
    }
    pub fn replay(&self, request: &Request) -> Result<Option<Value>, String> {
        let id: String = required(&request.params, "request_id")?;
        if id.is_empty() || id.len() > 128 {
            return Err("request_id must be 1..128 characters".into());
        }
        let signature = serde_json::to_string(request).map_err(|e| e.to_string())?;
        if let Some((old, result)) = self.replay.get(&id) {
            return if old == &signature {
                Ok(Some(result.clone()))
            } else {
                Err("request_id already used with different arguments".into())
            };
        }
        if self.replay.len() >= 512 || self.replay_bytes + signature.len() > 8 * 1024 * 1024 {
            return Err("session replay capacity reached; disable and re-enable local connections to start a new session".into());
        }
        Ok(None)
    }
    pub fn remember(&mut self, request: &Request, result: Value) {
        let id = request.params["request_id"].as_str().unwrap().to_owned();
        let signature = serde_json::to_string(request).unwrap();
        self.replay_bytes += signature.len();
        self.replay.insert(id, (signature, result));
    }
    pub fn spawn(
        &mut self,
        epoch: u64,
        work: impl FnOnce(Cancel) -> Result<Outcome, String> + Send + 'static,
    ) -> Result<Value, String> {
        if self.jobs.values().filter(|j| j.receiver.is_some()).count() >= 4 {
            return Err("four jobs are active; wait or cancel".into());
        }
        while self.jobs.len() >= 16 {
            let id = self
                .jobs
                .iter()
                .find(|(_, j)| j.receiver.is_none())
                .map(|(id, _)| *id)
                .ok_or("job capacity reached")?;
            self.jobs.remove(&id);
        }
        self.next += 1;
        let id = self.next;
        let cancel = Cancel::default();
        let token = cancel.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = work(token);
            let _ = tx.send(result);
        });
        self.jobs.insert(
            id,
            Job {
                cancel,
                receiver: Some(rx),
                result: None,
                epoch,
            },
        );
        Ok(json!({"job":id,"status":"pending"}))
    }
    pub fn file_busy(&self) -> bool {
        self.saving
            .and_then(|id| self.jobs.get(&id))
            .is_some_and(|j| j.receiver.is_some())
    }
    pub fn result(&self, id: u64) -> Result<Value, String> {
        let job = self.jobs.get(&id).ok_or(
            "job expired or does not exist; inspect state before creating another mutation",
        )?;
        Ok(
            json!({"job":id,"status":if job.receiver.is_some(){"pending"}else{"complete"},"outcome":job.result}),
        )
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.disable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_retention_is_bounded_scoped_and_cleared_on_disable() {
        let mut host = Host::default();
        let snapshot = fold_project::Project::new(1).snapshot();
        for _ in 0..4 {
            host.begin_candidate(snapshot.clone(), 7).unwrap();
        }
        assert!(host.begin_candidate(snapshot.clone(), 7).is_err());
        assert!(host.candidate(1, 1).is_err());
        host.candidates.get_mut(&1).unwrap().touched =
            std::time::Instant::now() - std::time::Duration::from_secs(901);
        host.expire_candidates(7);
        assert!(host.candidate(1, 0).is_err());
        host.begin_candidate(snapshot.clone(), 7).unwrap();
        host.expire_candidates(8);
        assert!(host.candidates.is_empty());
        host.begin_candidate(snapshot, 8).unwrap();
        host.disable();
        assert!(host.candidates.is_empty());
    }
}
