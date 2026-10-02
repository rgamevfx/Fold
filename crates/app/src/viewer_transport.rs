//! Instance clocks share one monitored audio device. No worker is created for
//! an unmonitored viewer. Clock arithmetic is integer/rational, independent of UI cadence.
#[cfg(feature = "desktop")]
use fold_foundation::Rounding;
use fold_foundation::Time;
use fold_platform::{desktop::ViewerTransport, workspace::PanelInstanceId};
use std::{collections::BTreeMap, time::Instant};

pub(super) struct Clock {
    pub transport: ViewerTransport,
    anchor: Instant,
    origin: Time,
    rate: [u32; 2],
    frames: u32,
    content: Option<String>,
}
impl Clock {
    fn advance(&mut self, now: Instant, sample: Option<u64>) -> bool {
        if !self.transport.playing {
            return false;
        }
        let frame = if let Some(sample) = sample {
            u128::from(sample) * u128::from(self.rate[0]) / (48_000 * u128::from(self.rate[1]))
        } else {
            let denominator = u128::from(self.origin.denominator());
            let numerator = self.origin.numerator().max(0) as u128 * 1_000_000_000
                + now.duration_since(self.anchor).as_nanos() * denominator;
            numerator * u128::from(self.rate[0])
                / (denominator * 1_000_000_000 * u128::from(self.rate[1]))
        };
        let (start, end) = self.transport.range.bounds(self.frames);
        let ended = frame > u128::from(end);
        let frame = if ended && self.transport.looping {
            u128::from(start) + (frame - u128::from(start)) % u128::from(end - start + 1)
        } else {
            frame.min(u128::from(end))
        } as u32;
        self.transport.time =
            Time::new(i64::from(frame) * i64::from(self.rate[1]), self.rate[0]).unwrap();
        if ended {
            self.transport.playing = self.transport.looping;
            if sample.is_some() {
                self.origin = self.transport.time;
                self.anchor = now;
            }
        }
        ended
    }
}
#[derive(Default)]
pub(super) struct Viewers {
    pub clocks: BTreeMap<PanelInstanceId, Clock>,
    pub monitor: Option<PanelInstanceId>,
    pub audio_error: Option<String>,
}
impl super::Session {
    pub(super) fn configure_viewer(&mut self, id: PanelInstanceId, mut transport: ViewerTransport) {
        if self.viewers.clocks.len() >= 64 && !self.viewers.clocks.contains_key(&id) {
            self.state.status = "Viewer limit reached (64)".into();
            return;
        }
        if self
            .viewers
            .clocks
            .get(&id)
            .is_some_and(|clock| clock.transport.output != transport.output)
        {
            transport.playing = false;
            transport.range = Default::default();
        }
        let state = fold_platform::desktop::DesktopClient::preview_state(
            self,
            &transport.output,
            transport.time,
        );
        if state.content.is_none() || self.overlay.is_some() {
            transport.playing = false;
        }
        let (start, end) = transport.range.bounds(state.frames);
        let frame = state.frame.min(state.frames.saturating_sub(1));
        let duration = Time::new(
            i64::from(state.frames) * i64::from(state.rate[1]),
            state.rate[0],
        )
        .unwrap();
        if state.frames > 0 && (transport.time < Time::ZERO || transport.time >= duration) {
            transport.time =
                Time::new(i64::from(frame) * i64::from(state.rate[1]), state.rate[0]).unwrap();
        }
        if transport.playing && (frame < start || frame >= end) {
            transport.time =
                Time::new(i64::from(start) * i64::from(state.rate[1]), state.rate[0]).unwrap();
        }
        let origin = transport.time;
        self.viewers.clocks.insert(
            id,
            Clock {
                transport,
                origin,
                anchor: Instant::now(),
                rate: state.rate,
                frames: state.frames,
                content: state.content,
            },
        );
        if self.viewers.monitor == Some(id) {
            self.restart_monitor();
        }
    }
    pub(super) fn refresh_viewers(&mut self) {
        let snapshot = self.preview_snapshot();
        let changed: Vec<_> = self
            .viewers
            .clocks
            .iter()
            .filter_map(|(&id, clock)| {
                let content =
                    crate::media_workflow::content_for(&snapshot, clock.transport.output.document)
                        .ok();
                (content != clock.content).then(|| {
                    let mut transport = clock.transport.clone();
                    transport.playing = false;
                    (id, transport)
                })
            })
            .collect();
        for (id, transport) in changed {
            self.configure_viewer(id, transport);
        }
    }
    pub(super) fn restart_monitor(&mut self) {
        self.viewers.audio_error = None;
        #[cfg(feature = "desktop")]
        {
            self.playback.pause();
            if let Some(clock) = self
                .viewers
                .monitor
                .and_then(|id| self.viewers.clocks.get(&id))
                && clock.transport.playing
                && clock.transport.output.output == "video"
            {
                let frame = clock
                    .transport
                    .time
                    .to_ticks(clock.rate[0], clock.rate[1], Rounding::Floor)
                    .unwrap_or(0)
                    .max(0) as u32;
                self.playback.play(
                    self.project.snapshot(),
                    clock.transport.output.document,
                    frame,
                    clock
                        .transport
                        .range
                        .bounds(clock.frames)
                        .1
                        .saturating_add(1),
                );
            }
        }
    }
    pub(super) fn monitor_viewer(&mut self, monitor: Option<PanelInstanceId>) {
        if self.viewers.monitor == monitor {
            return;
        }
        // Rebase the outgoing device-clock consumer at its own current position.
        let now = Instant::now();
        if let Some(clock) = self
            .viewers
            .monitor
            .and_then(|id| self.viewers.clocks.get_mut(&id))
        {
            clock.origin = clock.transport.time;
            clock.anchor = now;
        }
        self.viewers.monitor = monitor;
        self.restart_monitor();
    }
    pub(super) fn poll_viewers(&mut self) {
        let now = Instant::now();
        #[cfg(feature = "desktop")]
        if let Some(error) = self.playback.take_error() {
            self.state.status = format!("Audio monitoring unavailable: {error}");
            self.viewers.audio_error = Some(self.state.status.clone());
            self.playback.pause();
            if let Some(clock) = self
                .viewers
                .monitor
                .and_then(|id| self.viewers.clocks.get_mut(&id))
            {
                clock.origin = clock.transport.time;
                clock.anchor = now;
            }
        }
        #[cfg(feature = "desktop")]
        let sample = self.playback.sample();
        #[cfg(not(feature = "desktop"))]
        let sample = None;
        let mut restart = false;
        let snapshot = self.project.snapshot();
        for (&id, clock) in &mut self.viewers.clocks {
            if !snapshot
                .state()
                .documents
                .contains_key(&clock.transport.output.document)
            {
                clock.transport.playing = false;
                restart |= self.viewers.monitor == Some(id);
                continue;
            }
            let monitored =
                self.viewers.monitor == Some(id) && clock.transport.output.output == "video";
            #[cfg(feature = "desktop")]
            if monitored && self.playback.priming() {
                continue;
            }
            let ended = clock.advance(now, if monitored { sample } else { None });
            restart |= monitored && ended && sample.is_some();
        }
        if restart {
            self.restart_monitor();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn clock(now: Instant, start: u32, looping: bool) -> Clock {
        Clock {
            transport: ViewerTransport {
                output: fold_project::DocumentRef {
                    document: fold_foundation::DocumentId::new(),
                    output: "video".into(),
                    extensions: Default::default(),
                },
                time: Time::new(i64::from(start), 24).unwrap(),
                range: Default::default(),
                looping,
                playing: true,
            },
            anchor: now,
            origin: Time::new(i64::from(start), 24).unwrap(),
            rate: [24, 1],
            frames: 48,
            content: None,
        }
    }
    #[test]
    fn independent_clocks_pause_loop_and_device_mapping() {
        let now = Instant::now();
        let mut a = clock(now, 0, true);
        let mut b = clock(now, 12, false);
        a.advance(now + Duration::from_secs(1), None);
        b.advance(now + Duration::from_secs(1), None);
        assert_eq!(a.transport.time, Time::new(24, 24).unwrap());
        assert_eq!(b.transport.time, Time::new(36, 24).unwrap());
        b.transport.playing = false;
        a.advance(now + Duration::from_secs(2), None);
        b.advance(now + Duration::from_secs(2), None);
        assert_eq!(a.transport.time, Time::ZERO);
        assert_eq!(b.transport.time, Time::new(36, 24).unwrap());
        a.advance(now + Duration::from_secs(3), Some(24_000));
        assert_eq!(a.transport.time, Time::new(1, 2).unwrap());
    }
    #[test]
    fn subframe_origin_and_repeated_loops_do_not_accumulate_rounding_drift() {
        let now = Instant::now();
        let mut c = clock(now, 0, true);
        c.origin = Time::new(1, 48).unwrap();
        c.advance(now + Duration::from_millis(21), None);
        assert_eq!(c.transport.time, Time::new(1, 24).unwrap());
        for seconds in 1..=600 {
            c.advance(now + Duration::from_millis(seconds * 1000 + 21), None);
        }
        assert_eq!(c.transport.time, Time::new(1, 24).unwrap());
    }
    #[test]
    fn inclusive_out_and_fractional_rate() {
        let now = Instant::now();
        let mut c = clock(now, 0, false);
        c.rate = [30_000, 1001];
        c.transport.range.end = Some(29);
        c.advance(now + Duration::from_millis(1000), None);
        assert!(c.transport.playing);
        c.advance(now + Duration::from_millis(1001), None);
        assert!(!c.transport.playing);
        assert_eq!(c.transport.time, Time::new(29 * 1001, 30_000).unwrap());
    }
}
