//! Two supervised CPU workers with bounded admission and completion backpressure.
use super::{encode, errors::Failure};
use crate::agent::jobs;
use fold_media::Cancel;
use serde_json::Value;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread::JoinHandle,
};
pub(super) const MAX_IN_FLIGHT: usize = 8;
pub(super) enum Product {
    Reply(jobs::Completion),
    Inspection {
        proof: fold_motion::candidates::InspectionProof,
        bytes: Arc<[u8]>,
    },
    Candidate(Box<fold_motion::editing::Candidate>),
}
pub(super) enum Output {
    Published {
        value: Value,
        revision: fold_project::Revision,
    },
    Value(Value),
    Inspection {
        proof: fold_motion::candidates::InspectionProof,
        bytes: Arc<[u8]>,
    },
    Candidate(Box<fold_motion::editing::Candidate>),
}
type Work = Box<dyn FnOnce(&Cancel) -> Result<Output, Failure> + Send>;
pub(super) struct Completed {
    pub ticket: jobs::Ticket,
    pub result: Product,
}
struct Task {
    ticket: jobs::Ticket,
    work: Work,
    _permit: Permit,
}
pub(super) struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) struct Workers {
    sender: Option<Sender<Task>>,
    results: Receiver<Completed>,
    admitted: Arc<AtomicUsize>,
    threads: Vec<JoinHandle<()>>,
}
impl Workers {
    pub fn new(epoch: fold_foundation::ObjectId) -> Self {
        let (sender, tasks) = mpsc::channel::<Task>();
        let tasks = Arc::new(Mutex::new(tasks));
        let (completed, results) = mpsc::sync_channel(2);
        let threads = (0..2)
            .map(|_| {
                let tasks = tasks.clone();
                let completed = completed.clone();
                std::thread::spawn(move || worker(epoch, tasks, completed))
            })
            .collect();
        Self {
            sender: Some(sender),
            results,
            admitted: Arc::new(AtomicUsize::new(0)),
            threads,
        }
    }
    pub fn reserve(&self) -> Result<Permit, Failure> {
        self.admitted
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_IN_FLIGHT).then_some(count + 1)
            })
            .map(|_| Permit(self.admitted.clone()))
            .map_err(|_| Failure::new("capacity", "CPU worker admission is full"))
    }
    pub fn submit(
        &self,
        permit: Permit,
        ticket: jobs::Ticket,
        work: impl FnOnce(&Cancel) -> Result<Output, Failure> + Send + 'static,
    ) {
        // Receivers only exit after the sender closes. Panics in work are caught.
        assert!(
            self.sender
                .as_ref()
                .expect("live workers")
                .send(Task {
                    ticket,
                    work: Box::new(work),
                    _permit: permit
                })
                .is_ok()
        );
    }
    pub fn drain(&self) -> impl Iterator<Item = Completed> + '_ {
        self.results.try_iter()
    }
}
fn worker(
    epoch: fold_foundation::ObjectId,
    tasks: Arc<Mutex<Receiver<Task>>>,
    results: SyncSender<Completed>,
) {
    loop {
        let task = { tasks.lock().expect("worker queue").recv() };
        let Ok(Task {
            ticket,
            work,
            _permit,
        }) = task
        else {
            return;
        };
        let cancel = ticket.cancellation();
        let result = if cancel.check().is_err() {
            Err(Failure::new("cancelled", "Job cancelled before execution"))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&cancel))).unwrap_or_else(
                |_| {
                    Err(Failure::new(
                        "unsupported",
                        "CPU worker panicked; no project mutation was performed",
                    ))
                },
            )
        };
        let result = match result {
            Ok(Output::Published { value, revision }) => {
                Product::Reply(jobs::Completion::Response {
                    bytes: encode(epoch, Ok(value)).into(),
                    committed: Some(revision),
                })
            }
            Ok(Output::Inspection { proof, bytes }) => Product::Inspection { proof, bytes },
            Ok(Output::Candidate(candidate)) => Product::Candidate(candidate),
            Err(error) if error.code == "cancelled" => Product::Reply(jobs::Completion::Cancelled),
            result => {
                let response = result.map(|value| match value {
                    Output::Value(value) => value,
                    _ => unreachable!(),
                });
                Product::Reply(jobs::Completion::Response {
                    bytes: encode(epoch, response).into(),
                    committed: None,
                })
            }
        };
        if results.send(Completed { ticket, result }).is_err() {
            return;
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.sender.take();
        for thread in self.threads.drain(..) {
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
        // Never block the UI on join. Jobs owns cancellation; dropped receivers
        // prevent detached workers from publishing into a replacement session.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::ObjectId;
    use std::time::{Duration, Instant};
    #[test]
    fn bounded_workers_report_panics_and_acknowledge_cancellation() {
        let epoch = ObjectId::new();
        let now = Instant::now();
        let workers = Workers::new(epoch);
        let mut jobs = jobs::Jobs::new(epoch);
        let permits: Vec<_> = (0..MAX_IN_FLIGHT)
            .map(|_| workers.reserve().unwrap_or_else(|_| panic!("capacity")))
            .collect();
        assert!(workers.reserve().is_err());
        drop(permits);
        let (_, ticket) = jobs.start(Duration::from_secs(30), now).unwrap();
        workers.submit(
            workers.reserve().unwrap_or_else(|_| panic!("capacity")),
            ticket,
            |_| panic!("worker test panic"),
        );
        let (cancelled, ticket) = jobs.start(Duration::from_secs(30), now).unwrap();
        jobs.cancel(cancelled, now).unwrap();
        workers.submit(
            workers.reserve().unwrap_or_else(|_| panic!("capacity")),
            ticket,
            |_| panic!("cancelled work must not execute"),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut results = vec![];
        while results.len() < 2 {
            results.extend(workers.drain());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            results
                .iter()
                .any(|r| matches!(r.result, Product::Reply(jobs::Completion::Cancelled)))
        );
        assert!(results.iter().any(|r| {
            match &r.result {
                Product::Reply(jobs::Completion::Response { bytes, .. }) => {
                    serde_json::from_slice::<Value>(bytes).unwrap()["error"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("panicked")
                }
                _ => false,
            }
        }));
        for result in results {
            jobs.complete(
                &result.ticket,
                match result.result {
                    Product::Reply(reply) => reply,
                    _ => panic!("unexpected candidate"),
                },
                Instant::now(),
            )
            .unwrap();
        }
        assert!(workers.reserve().is_ok());
        assert_eq!(
            jobs.cancel(cancelled, Instant::now()),
            Ok(jobs::Cancellation::Cancelled)
        );
    }
}
