//! One bounded look-ahead worker for the preview service. It only prepares
//! source samples: playback clocks and final-result publication stay unchanged.
use crate::media_workflow as workflow;
use fold_media::Cancel;
use fold_platform::desktop::{PreviewKey, ViewerTransport};
use fold_project::Snapshot;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

#[derive(Clone)]
pub(crate) struct Request {
    pub snapshot: Snapshot,
    pub key: PreviewKey,
    pub transport: ViewerTransport,
}
impl Request {
    fn same_work(&self, other: &Self) -> bool {
        self.key == other.key
            && self.transport.range == other.transport.range
            && self.transport.looping == other.transport.looping
    }
    fn advances_to(&self, other: &Self) -> bool {
        if self.transport.range != other.transport.range
            || self.transport.looping != other.transport.looping
            || self.key.frame.checked_add(1) != Some(other.key.frame)
        {
            return false;
        }
        let mut next = self.key.clone();
        next.frame = other.key.frame;
        // Content identity still guards document edits; only rational time
        // changes during ordinary forward playback.
        if let (Some((document, _)), Some((_, time))) = (next.target, other.key.target) {
            next.target = Some((document, time));
        }
        next == other.key
    }
}
#[derive(Default)]
struct State {
    pending: VecDeque<Request>,
    active: Option<(Request, Cancel)>,
    served: Vec<Request>,
    prepared: Vec<(Request, fold_render::PreparedSources)>,
    stop: bool,
}
pub(crate) struct Worker {
    state: Arc<(Mutex<State>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new() -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = state.clone();
        let thread = std::thread::Builder::new()
            .name("fold-preview-sources".into())
            .spawn(move || {
                loop {
                    let (request, cancel) = {
                        let mut state = worker.0.lock().unwrap();
                        while state.pending.is_empty() && !state.stop {
                            state = worker.1.wait(state).unwrap();
                        }
                        if state.stop {
                            break;
                        }
                        let request = state.pending.pop_front().unwrap();
                        let cancel = Cancel::default();
                        state.active = Some((request.clone(), cancel.clone()));
                        (request, cancel)
                    };
                    // Speculation is dispensable. Current-frame evaluation reports real
                    // errors and retries resource pressure through its existing policy.
                    let prepared = prepare(&request, &cancel);
                    let mut state = worker.0.lock().unwrap();
                    if cancel.check().is_ok() {
                        state.served.push(request.clone());
                        if let Ok(Some(prepared)) = prepared {
                            state.prepared.push((request, prepared));
                        }
                    }
                    state.active = None;
                }
            })
            .expect("source preparation worker creation failed");
        Self {
            state,
            thread: Some(thread),
        }
    }
    pub fn replace(&mut self, requests: Vec<Request>) {
        self.state.0.lock().unwrap().replace(requests);
        self.state.1.notify_one();
    }
    pub fn cancel(&mut self) {
        self.replace(Vec::new());
    }
}
impl State {
    fn replace(&mut self, requests: Vec<Request>) {
        let mut unique = Vec::new();
        for request in requests.into_iter().filter(|r| r.transport.playing) {
            if !unique.iter().any(|r| request.same_work(r)) {
                unique.push(request);
            }
            if unique.len() == 4 {
                break;
            }
        }
        let requests = unique;
        let state = self;
        state
            .prepared
            .retain(|(key, _)| requests.iter().any(|r| r.same_work(key)));
        state
            .served
            .retain(|key| requests.iter().any(|r| r.same_work(key)));
        if let Some((key, cancel)) = &state.active {
            if !requests
                .iter()
                .any(|r| r.same_work(key) || key.advances_to(r))
            {
                cancel.cancel();
            }
        }
        state.pending = requests
            .into_iter()
            .filter(|r| {
                !state.served.iter().any(|done| done.same_work(r))
                    && !state
                        .active
                        .as_ref()
                        .is_some_and(|(key, cancel)| key.same_work(r) && cancel.check().is_ok())
            })
            .collect();
    }
}
fn prepare(
    request: &Request,
    cancel: &Cancel,
) -> Result<Option<fold_render::PreparedSources>, String> {
    let mut key = request.key.clone();
    let scene = workflow::preview_request(&request.snapshot, &key)?;
    let info = crate::packages::builtins().output_ref(&request.snapshot, &scene.source)?;
    let Some(next) = next_key(&key, &request.transport, &info) else {
        return Ok(None);
    };
    key = next;
    let scene = workflow::preview_request(&request.snapshot, &key)?;
    let (graph, _) = workflow::compile_scene(&request.snapshot, &scene, cancel)?;
    fold_render::prepare_sources(&graph, cancel).map(Some)
}
fn next_key(
    key: &PreviewKey,
    transport: &ViewerTransport,
    info: &fold_media::VideoInfo,
) -> Option<PreviewKey> {
    if !transport.playing || info.frames == 0 {
        return None;
    }
    let (start, end) = transport.range.bounds(info.frames);
    let frame = if key.frame >= end {
        if transport.looping {
            start
        } else {
            return None;
        }
    } else {
        key.frame + 1
    };
    let (document, _) = key.target?;
    Some(PreviewKey {
        frame,
        target: Some((document, info.time(frame).ok()?)),
        ..key.clone()
    })
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        self.state.0.lock().unwrap().stop = true;
        self.state.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::{DocumentId, Time};
    use fold_platform::desktop::{PlaybackMode, PlaybackRange};
    fn request() -> Request {
        let id = DocumentId::new();
        Request {
            snapshot: fold_project::Project::new(0).snapshot().evaluation(),
            key: PreviewKey {
                target: Some((id, Time::ZERO)),
                frame: 0,
                output: "video".into(),
                content: "source".into(),
                dimensions: [17, 9],
                region: None,
                view: 1,
                channels: Default::default(),
            },
            transport: ViewerTransport {
                mode: PlaybackMode::EveryFrame,
                output: fold_project::DocumentRef {
                    document: id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                time: Time::ZERO,
                playing: true,
                looping: false,
                range: PlaybackRange::default(),
            },
        }
    }
    #[test]
    fn lookahead_uses_exact_time_and_stops_or_wraps_at_the_review_boundary() {
        let mut r = request();
        let info = fold_media::VideoInfo {
            width: 17,
            height: 9,
            rate: [24000, 1001],
            frames: 120,
        };
        let next = next_key(&r.key, &r.transport, &info).unwrap();
        assert_eq!(next.target.unwrap().1, Time::new(1001, 24000).unwrap());
        r.key.frame = 25;
        r.transport.range = PlaybackRange {
            start: Some(10),
            end: Some(25),
        };
        assert!(next_key(&r.key, &r.transport, &info).is_none());
        r.transport.looping = true;
        assert_eq!(next_key(&r.key, &r.transport, &info).unwrap().frame, 10);
        r.transport.playing = false;
        assert!(next_key(&r.key, &r.transport, &info).is_none());
    }
    #[test]
    fn seek_range_edit_and_pause_cancel_old_work_and_bound_pending_snapshots() {
        let mut state = State::default();
        let r = request();
        let cancel = Cancel::default();
        state.active = Some((r.clone(), cancel.clone()));
        state.replace(vec![r.clone()]);
        assert!(cancel.check().is_ok());
        assert!(state.pending.is_empty());
        let mut forward = r.clone();
        forward.key.frame = 1;
        forward.key.target = Some((r.key.target.unwrap().0, Time::new(1, 24).unwrap()));
        state.replace(vec![forward]);
        assert!(
            cancel.check().is_ok(),
            "playback must preserve the demanded decode"
        );
        let mut changed = r.clone();
        changed.transport.range.end = Some(15);
        state.replace(vec![changed]);
        assert!(cancel.check().is_err());
        let cancel = Cancel::default();
        state.active = Some((r.clone(), cancel.clone()));
        let mut seek = r.clone();
        seek.key.frame = 40;
        state.replace(vec![seek]);
        assert!(cancel.check().is_err());
        state.replace(vec![r.clone(); 100]);
        assert!(state.pending.len() <= 4);
        let cancel = Cancel::default();
        state.active = Some((r.clone(), cancel.clone()));
        let mut paused = r;
        paused.transport.playing = false;
        state.replace(vec![paused]);
        assert!(state.pending.is_empty());
        assert!(cancel.check().is_err());
    }
}
