//! Bounded, cancellable Inspector sampling off the UI thread.
use crate::model::Construction;
use fold_foundation::{DocumentId, ObjectId};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq)]
pub(in crate::ui) struct SampleKey {
    pub document: DocumentId,
    pub object: Option<ObjectId>,
    pub revision: fold_project::Revision,
    pub time: fold_foundation::Time,
    pub occurrence: Vec<ObjectId>,
    pub parameters: Vec<ObjectId>,
}
#[derive(Default)]
pub(in crate::ui) struct Samples {
    key: Option<SampleKey>,
    receiver: Option<
        std::sync::mpsc::Receiver<Result<BTreeMap<ObjectId, crate::types::Literal>, String>>,
    >,
    cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub values: BTreeMap<ObjectId, crate::types::Literal>,
    pub error: String,
}
impl Drop for Samples {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
impl Samples {
    pub fn update(&mut self, key: SampleKey, graph: &Construction) {
        if key.parameters.is_empty() {
            self.key = None;
            if let Some(cancel) = &self.cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.values.clear();
            self.error.clear();
            return;
        }
        if self.key.as_ref() != Some(&key) {
            self.key = None;
            if let Some(cancel) = &self.cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.values.clear();
            self.error.clear();
            // Keep one worker at a time. Cancellation is observed by the common evaluator.
            if let Some(receiver) = &self.receiver {
                match receiver.try_recv() {
                    Err(std::sync::mpsc::TryRecvError::Empty) => return,
                    _ => {
                        self.receiver = None;
                    }
                }
            }
            let (sender, receiver) = std::sync::mpsc::channel();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.cancel = Some(cancel.clone());
            self.receiver = Some(receiver);
            self.key = Some(key.clone());
            let graph = graph.clone();
            std::thread::spawn(move || {
                let check = || {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        Err("Cancelled parameter inspection".into())
                    } else {
                        Ok(())
                    }
                };
                let result = check()
                    .and_then(|()| crate::evaluation::Prepared::new(graph))
                    .and_then(|prepared| {
                        prepared.sample_parameters(
                            &key.occurrence,
                            &key.parameters,
                            key.time,
                            &check,
                        )
                    });
                let _ = sender.send(result);
            });
        }
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(Ok(values)) => {
                    self.values = values;
                    self.receiver = None;
                }
                Ok(Err(error)) => {
                    self.error = error;
                    self.receiver = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.error = "Parameter inspection stopped".into();
                    self.receiver = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reselecting_a_cancelled_sample_starts_a_fresh_request() {
        let document = DocumentId::new();
        let graph =
            Construction::from_document(&crate::package::create(document).unwrap()).unwrap();
        let parameter = graph.definitions[0].nodes[0].parameters["height"].id;
        let key = SampleKey {
            document,
            object: None,
            revision: fold_project::Revision(0),
            time: fold_foundation::Time::ZERO,
            occurrence: vec![],
            parameters: vec![parameter],
        };
        let mut samples = Samples::default();
        samples.update(key.clone(), &graph);
        let mut empty = key.clone();
        empty.parameters.clear();
        samples.update(empty, &graph);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !samples.values.contains_key(&parameter) {
            assert!(std::time::Instant::now() < deadline, "{}", samples.error);
            samples.update(key.clone(), &graph);
            std::thread::yield_now();
        }
        assert!(samples.error.is_empty());
    }
}
