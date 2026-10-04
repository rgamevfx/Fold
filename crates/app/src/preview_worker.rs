//! Shared bounded preview execution over immutable snapshots.
use crate::media_workflow as workflow;
use fold_media::{Cancel, Decoder};
use fold_platform::desktop::{PreviewDemand, PreviewKey, PreviewResult};
use fold_platform::workspace::PanelInstanceId;
use fold_project::Snapshot;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex},
};

type Request = (Snapshot, PreviewDemand);
struct Active {
    key: PreviewKey,
    consumers: Vec<(PanelInstanceId, u64, bool)>,
    cancel: Cancel,
}
#[derive(Default)]
pub(super) struct Mailbox {
    pending: BTreeMap<(PanelInstanceId, bool), Request>,
    served: Vec<PreviewDemand>,
    active: Option<Active>,
    cursor: Option<(PanelInstanceId, bool)>,
    foreground_run: u8,
    realtime: BTreeSet<PanelInstanceId>,
    realtime_run: u8,
    pressure_since: Option<std::time::Instant>,
    pressure_retries: u64,
    pub result: Option<PreviewResult>,
    #[cfg(feature = "gpu")]
    pub gpu_result: Option<fold_platform::desktop::GpuPreviewResult>,
    #[cfg(feature = "gpu")]
    pub render_host: Option<fold_render::gpu::Host>,
    pub colors: Option<fold_platform::color::Choices>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    updating: bool,
    stop: bool,
}
pub(super) struct PreviewWorker {
    pub shared: Arc<(Mutex<Mailbox>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Mailbox {
    pub fn pressure_retries(&self) -> u64 {
        self.pressure_retries
    }
    pub fn depth(&self) -> usize {
        self.pending.len()
    }
    fn replace(&mut self, snapshot: Snapshot, demands: Vec<PreviewDemand>) {
        // Limit ingress before retaining snapshots or allocating a work queue.
        let demands: Vec<_> = demands.into_iter().take(128).collect();
        self.served.retain(|done| demands.contains(done));
        if let Some(active) = &mut self.active {
            active.consumers.retain(|(id, generation, background)| {
                demands.iter().any(|d| {
                    d.consumer == *id
                        && d.generation == *generation
                        && d.background == *background
                        && d.key.content == active.key.content
                        && d.key.dimensions == active.key.dimensions
                        && d.key.view == active.key.view
                        && d.key.output == active.key.output
                        && d.key.target.map(|t| t.0) == active.key.target.map(|t| t.0)
                })
            });
            for demand in &demands {
                let owner = (demand.consumer, demand.generation, demand.background);
                if demand.key == active.key && active.cancel.check().is_ok() {
                    if !active.consumers.contains(&owner) {
                        active.consumers.push(owner);
                    }
                    if !self.served.contains(demand) {
                        self.served.push(demand.clone());
                    }
                }
            }
            if active.consumers.is_empty() {
                active.cancel.cancel();
            }
        }
        self.pending.clear();
        for demand in demands {
            let active = self
                .active
                .as_ref()
                .is_some_and(|a| a.cancel.check().is_ok() && a.key == demand.key);
            if !active && !self.served.contains(&demand) {
                self.pending.insert(
                    (demand.consumer, demand.background),
                    (snapshot.clone(), demand),
                );
            }
        }
    }
    fn next(&mut self) -> Option<(Snapshot, PreviewKey, Cancel, bool)> {
        let background = self.pending.values().any(|(_, d)| d.background)
            && (self.foreground_run >= 4 || !self.pending.values().any(|(_, d)| !d.background));
        let realtime = !background
            && self
                .pending
                .values()
                .any(|(_, d)| !d.background && self.realtime.contains(&d.consumer))
            && (self.realtime_run < 8
                || !self
                    .pending
                    .values()
                    .any(|(_, d)| !d.background && !self.realtime.contains(&d.consumer)));
        let candidates: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, (_, d))| d.background == background)
            .filter(|(_, (_, d))| background || self.realtime.contains(&d.consumer) == realtime)
            .map(|(&id, _)| id)
            .collect();
        let id = candidates
            .iter()
            .find(|id| self.cursor.is_none_or(|c| **id > c))
            .or(candidates.first())
            .copied()
            .or_else(|| self.pending.keys().next().copied())?;
        self.cursor = Some(id);
        let (snapshot, demand) = self.pending.remove(&id).unwrap();
        if !background {
            self.realtime_run = if realtime {
                self.realtime_run.saturating_add(1)
            } else {
                0
            };
        }
        self.foreground_run = if demand.background {
            0
        } else {
            self.foreground_run.saturating_add(1)
        };
        let mut consumers = vec![(id.0, demand.generation, demand.background)];
        self.served.push(demand.clone());
        self.pending.retain(|&id, (_, other)| {
            if other.key == demand.key {
                consumers.push((id.0, other.generation, other.background));
                self.served.push(other.clone());
                false
            } else {
                true
            }
        });
        let cancel = Cancel::default();
        self.active = Some(Active {
            key: demand.key.clone(),
            consumers,
            cancel: cancel.clone(),
        });
        Some((snapshot, demand.key, cancel, demand.background))
    }
    fn retry_pressure(
        &mut self,
        snapshot: &Snapshot,
        key: &PreviewKey,
        error: Option<&str>,
    ) -> bool {
        if !error.is_some_and(|error| {
            error.contains("budget exhausted") || error.contains("capacity exhausted")
        }) {
            self.pressure_since = None;
            return false;
        }
        let began = self
            .pressure_since
            .get_or_insert_with(std::time::Instant::now);
        if began.elapsed().as_secs() >= 30 {
            return false;
        }
        let Some(active) = self.active.take() else {
            return false;
        };
        if active.cancel.check().is_err() {
            return true;
        }
        self.pressure_retries += 1;
        for (id, generation, background) in active.consumers {
            let demand = PreviewDemand {
                consumer: id,
                generation,
                key: key.clone(),
                background,
            };
            self.served.retain(|done| done != &demand);
            self.pending
                .entry((id, background))
                .or_insert_with(|| (snapshot.clone(), demand));
        }
        true
    }
    fn occupied(&self) -> bool {
        #[cfg(feature = "gpu")]
        if self.gpu_result.is_some() {
            return true;
        }
        self.result.is_some()
    }
    fn finish(&mut self) -> Vec<(PanelInstanceId, u64)> {
        self.active.take().map_or_else(Vec::new, |active| {
            active
                .consumers
                .into_iter()
                .filter(|owner| !owner.2)
                .map(|(id, generation, _)| (id, generation))
                .collect()
        })
    }
}
// Release the mailbox before notifying: a host callback may immediately collect
// the result. At most one published result is waiting, so notifications are bounded.
fn notify(queue: std::sync::MutexGuard<'_, Mailbox>) {
    let wake = queue.occupied().then(|| queue.wake.clone()).flatten();
    drop(queue);
    if let Some(wake) = wake {
        wake();
    }
}
impl PreviewWorker {
    pub fn new() -> Self {
        let shared = Arc::new((
            Mutex::new(Mailbox {
                pending: Default::default(),
                served: vec![],
                active: None,
                cursor: None,
                foreground_run: 0,
                realtime: Default::default(),
                realtime_run: 0,
                pressure_since: None,
                pressure_retries: 0,
                result: None,
                #[cfg(feature = "gpu")]
                gpu_result: None,
                #[cfg(feature = "gpu")]
                render_host: None,
                colors: None,
                wake: None,
                updating: false,
                stop: false,
            }),
            Condvar::new(),
        ));
        let state = shared.clone();
        let thread = std::thread::spawn(move || {
            let mut decoder = Decoder::from_environment();
            let mut color_identity = None;
            #[cfg(feature = "gpu")]
            let mut gpu: Option<(u64, Result<fold_render::gpu::Renderer, String>)> = None;
            loop {
                let (snapshot, key, cancel, background) = {
                    let (lock, ready) = &*state;
                    let mut queue = lock.lock().unwrap();
                    while (queue.pending.is_empty() || queue.occupied() || queue.updating)
                        && !queue.stop
                    {
                        queue = ready.wait(queue).unwrap();
                    }
                    if queue.stop {
                        break;
                    }
                    queue.next().unwrap()
                };
                let class = if background {
                    fold_render::scheduling::Class::Prepare
                } else {
                    fold_render::scheduling::Class::Viewer
                };
                let identity = snapshot
                    .state()
                    .settings
                    .get(fold_platform::color::PROJECT_KEY)
                    .cloned();
                if identity != color_identity {
                    let choices = crate::color::choices(&snapshot);
                    if cancel.check().is_ok() {
                        if choices.error.is_none() {
                            color_identity = identity;
                        }
                        state.0.lock().unwrap().colors = Some(choices);
                    }
                }
                #[cfg(feature = "gpu")]
                let render_host = state.0.lock().unwrap().render_host.clone();
                #[cfg(feature = "gpu")]
                if let Some(host) = render_host {
                    if gpu.as_ref().is_none_or(|(id, _)| *id != host.id()) {
                        gpu = Some((host.id(), fold_render::gpu::Renderer::new(host)));
                    }
                    let frame = (|| {
                        let renderer = gpu.as_mut().unwrap().1.as_mut().map_err(|e| e.clone())?;
                        let request = workflow::preview_request(&snapshot, &key)?;
                        let scene = workflow::evaluate_scene_gpu_admitted(
                            &snapshot,
                            &request,
                            renderer,
                            decoder.as_mut().map_err(|e| e.clone())?,
                            &cancel,
                            Some(class),
                        )?;
                        let _output_permit =
                            fold_render::scheduling::Scheduler::shared().enter(class, &cancel)?;
                        if let fold_render::view::View::Channel { range, .. } = &key.channels {
                            return renderer.output_data(&scene, *range, &cancel);
                        }
                        crate::color::with_config(&snapshot, |config| {
                            let processor = config
                                .map(|c| c.display(fold_color::WORKING_SPACE, &Default::default()))
                                .transpose()?;
                            renderer.output(&scene, processor.as_ref(), &cancel)
                        })
                    })();
                    let mut queue = state.0.lock().unwrap();
                    if queue.retry_pressure(
                        &snapshot,
                        &key,
                        frame.as_ref().err().map(String::as_str),
                    ) {
                        let _ = state
                            .1
                            .wait_timeout(queue, std::time::Duration::from_millis(10))
                            .unwrap();
                        continue;
                    }
                    let consumers = queue.finish();
                    if cancel.check().is_ok() && !queue.stop {
                        queue.gpu_result = Some(fold_platform::desktop::GpuPreviewResult {
                            key,
                            frame,
                            consumers,
                        });
                    }
                    notify(queue);
                    continue;
                }
                let admission = fold_render::scheduling::Scheduler::shared().enter(class, &cancel);
                let _admission = match admission {
                    Ok(permit) => permit,
                    Err(error) => {
                        let mut queue = state.0.lock().unwrap();
                        let consumers = queue.finish();
                        if cancel.check().is_ok() && !queue.stop {
                            queue.result = Some(PreviewResult {
                                key,
                                frame: Err(error),
                                consumers,
                            });
                        }
                        notify(queue);
                        continue;
                    }
                };
                let frame = decoder
                    .as_mut()
                    .map_err(|e| e.clone())
                    .and_then(|decoder| workflow::evaluate(&snapshot, &key, decoder, &cancel))
                    .and_then(|f| match &key.channels {
                        fold_render::view::View::Channel { range, .. } => f.to_data_display(*range),
                        _ => crate::color::preview(&snapshot, &f),
                    });
                let mut queue = state.0.lock().unwrap();
                if queue.retry_pressure(&snapshot, &key, frame.as_ref().err().map(String::as_str)) {
                    let _ = state
                        .1
                        .wait_timeout(queue, std::time::Duration::from_millis(10))
                        .unwrap();
                    continue;
                }
                // Request replacement/cancellation and publication serialize here.
                let consumers = queue.finish();
                if cancel.check().is_ok() && !queue.stop {
                    queue.result = Some(PreviewResult {
                        key,
                        frame,
                        consumers,
                    });
                }
                notify(queue);
            }
        });
        Self {
            shared,
            thread: Some(thread),
        }
    }
    pub fn set_wake(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.shared.0.lock().unwrap().wake = Some(wake);
    }
    pub fn cancel(&mut self) {
        let mut queue = self.shared.0.lock().unwrap();
        if let Some(active) = &queue.active {
            active.cancel.cancel();
        }
        queue.pending.clear();
        queue.served.clear();
        queue.pressure_since = None;
        queue.result = None;
        #[cfg(feature = "gpu")]
        {
            queue.gpu_result = None;
        }
        self.shared.1.notify_one();
    }
    pub fn begin_update(&mut self) {
        self.shared.0.lock().unwrap().updating = true;
    }
    pub fn end_update(&mut self) {
        self.shared.0.lock().unwrap().updating = false;
        self.shared.1.notify_one();
    }
    pub fn demands(&mut self, snapshot: Snapshot, demands: Vec<PreviewDemand>) {
        let mut queue = self.shared.0.lock().unwrap();
        queue.replace(snapshot, demands);
        drop(queue);
        self.shared.1.notify_one();
    }
    pub fn realtime_consumers(&mut self, consumers: BTreeSet<PanelInstanceId>) {
        self.shared.0.lock().unwrap().realtime = consumers;
    }
    pub fn request(&mut self, snapshot: Snapshot, key: PreviewKey) {
        self.cancel();
        self.demands(
            snapshot,
            vec![PreviewDemand {
                consumer: PanelInstanceId(0),
                generation: 0,
                key,
                background: false,
            }],
        );
    }
    pub fn take(&self) -> Option<PreviewResult> {
        let result = self.shared.0.lock().unwrap().result.take();
        self.shared.1.notify_one();
        result
    }
    #[cfg(feature = "gpu")]
    pub fn take_gpu(&self) -> Option<fold_platform::desktop::GpuPreviewResult> {
        let result = self.shared.0.lock().unwrap().gpu_result.take();
        self.shared.1.notify_one();
        result
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

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        fold_project::Project::new(0).snapshot().evaluation()
    }
    fn demand(id: u64, generation: u64, frame: u32, background: bool) -> PreviewDemand {
        PreviewDemand {
            consumer: PanelInstanceId(id),
            generation,
            background,
            key: PreviewKey {
                target: None,
                output: "video".into(),
                content: "scene".into(),
                frame,
                dimensions: [16, 16],
                view: 1,
                channels: Default::default(),
            },
        }
    }
    #[test]
    fn joining_an_active_key_does_not_queue_duplicate_work_after_completion() {
        let mut queue = Mailbox::default();
        let a = demand(1, 1, 0, false);
        let b = demand(2, 4, 0, false);
        queue.replace(snapshot(), vec![a.clone()]);
        queue.next().unwrap();
        queue.replace(snapshot(), vec![a.clone(), b.clone()]);
        assert_eq!(queue.finish().len(), 2);
        queue.replace(snapshot(), vec![a, b]);
        assert!(queue.next().is_none());
    }
    #[test]
    fn shared_work_survives_one_seek_but_cancels_when_all_owners_retire() {
        let mut queue = Mailbox::default();
        queue.replace(
            snapshot(),
            vec![demand(1, 1, 0, false), demand(2, 1, 0, false)],
        );
        let (_, _, cancel, _) = queue.next().unwrap();
        assert_eq!(queue.active.as_ref().unwrap().consumers.len(), 2);
        queue.replace(
            snapshot(),
            vec![demand(1, 2, 12, false), demand(2, 1, 0, false)],
        );
        assert!(cancel.check().is_ok());
        assert_eq!(
            queue.active.as_ref().unwrap().consumers,
            [(PanelInstanceId(2), 1, false)]
        );
        queue.replace(snapshot(), vec![demand(1, 2, 12, false)]);
        assert!(cancel.check().is_err());
        assert!(queue.finish().is_empty());
        assert_eq!(queue.next().unwrap().1.frame, 12);
    }
    #[test]
    fn advancing_viewers_and_background_ranges_receive_fair_service() {
        let mut queue = Mailbox::default();
        let mut order = vec![];
        for step in 0..12 {
            queue.replace(
                snapshot(),
                vec![
                    demand(1, 1, step, false),
                    demand(2, 1, 100 + step, false),
                    demand(1, 1, 200 + step, true),
                ],
            );
            let (_, key, _, background) = queue.next().unwrap();
            order.push((key.frame, background));
            queue.finish();
        }
        assert!(order.iter().filter(|r| r.1).count() >= 2);
        assert!(order.iter().any(|r| (100..200).contains(&r.0)));
    }
    #[test]
    fn realtime_gets_capacity_without_starving_every_frame() {
        let mut queue = Mailbox::default();
        queue.realtime.insert(PanelInstanceId(1));
        let mut order = Vec::new();
        for step in 0..10 {
            queue.replace(
                snapshot(),
                vec![demand(1, 1, step, false), demand(2, 1, 100 + step, false)],
            );
            order.push(queue.next().unwrap().1.frame < 100);
            queue.finish();
        }
        assert_eq!(
            order,
            [true, true, true, true, true, true, true, true, false, true]
        );
    }
    #[test]
    fn range_and_current_frame_deduplicate_without_moving_the_viewer() {
        let mut queue = Mailbox::default();
        queue.replace(
            snapshot(),
            vec![
                demand(1, 1, 7, false),
                demand(1, 1, 7, true),
                demand(2, 9, 7, false),
            ],
        );
        queue.next().unwrap();
        assert!(queue.pending.is_empty());
        assert_eq!(
            queue.finish(),
            [(PanelInstanceId(1), 1), (PanelInstanceId(2), 9)]
        );
        queue.replace(
            snapshot(),
            vec![
                demand(1, 1, 7, false),
                demand(1, 1, 7, true),
                demand(2, 9, 7, false),
            ],
        );
        assert!(
            queue.next().is_none(),
            "never evaluate unacknowledged identical demand twice"
        );
    }
    #[test]
    fn allocation_pressure_yields_and_retries_valid_demand_without_failure_publication() {
        let mut queue = Mailbox::default();
        let snapshot = snapshot();
        let d = demand(1, 1, 7, false);
        queue.replace(snapshot.clone(), vec![d.clone()]);
        queue.next().unwrap();
        assert!(queue.retry_pressure(&snapshot, &d.key, Some("GPU working budget exhausted")));
        assert_eq!(queue.pressure_retries(), 1);
        assert!(!queue.occupied());
        assert_eq!(queue.next().unwrap().1, d.key);
        assert!(!queue.retry_pressure(&snapshot, &d.key, None));
        assert_eq!(queue.finish(), [(PanelInstanceId(1), 1)]);
        queue.replace(snapshot, vec![]);
        assert!(queue.next().is_none());
    }
    #[test]
    fn ingress_and_result_retention_are_bounded() {
        let mut queue = Mailbox::default();
        queue.replace(
            snapshot(),
            (0..1000)
                .map(|id| demand(id, 1, id as u32, false))
                .collect(),
        );
        assert_eq!(queue.pending.len(), 128);
        queue.result = Some(PreviewResult {
            key: demand(0, 1, 0, false).key,
            consumers: vec![],
            frame: Err("fixture".into()),
        });
        assert!(queue.occupied());
        queue.result.take();
        assert!(!queue.occupied());
    }
}
