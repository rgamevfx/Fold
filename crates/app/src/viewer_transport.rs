//! Instance clocks share one monitored audio device. No worker is created for
//! an unmonitored viewer. Clock arithmetic is integer/rational, independent of UI cadence.
use fold_foundation::Rounding;
use fold_foundation::Time;
use fold_platform::desktop::{PlaybackMode, PreviewKey};
use fold_platform::{desktop::ViewerTransport, workspace::PanelInstanceId};
use std::{collections::BTreeMap, time::Instant};

pub(super) struct Clock {
    pub transport: ViewerTransport,
    anchor: Instant,
    origin: Time,
    rate: [u32; 2],
    frames: u32,
    content: Option<String>,
    generation: u64,
    presented: Option<u32>,
    /// Rational deadlines avoid rounding a fractional frame period on each tick.
    pacing: Option<(Instant, u64)>,
}
impl Clock {
    fn frame(&self) -> u32 {
        self.transport
            .time
            .to_ticks(self.rate[0], self.rate[1], Rounding::Floor)
            .unwrap_or(0)
            .max(0) as u32
    }
    fn request_frame(&self) -> u32 {
        let current = self.frame();
        if self.transport.mode != PlaybackMode::EveryFrame
            || !self.transport.playing
            || self.presented.is_none()
        {
            return current;
        }
        let (start, end) = self.transport.range.bounds(self.frames);
        if current >= end {
            if self.transport.looping { start } else { end }
        } else {
            current + 1
        }
    }
    fn interval_elapsed(&self, now: Instant) -> bool {
        self.pacing.is_none_or(|(anchor, intervals)| {
            now.duration_since(anchor).as_nanos() * u128::from(self.rate[0])
                >= u128::from(intervals) * 1_000_000_000 * u128::from(self.rate[1])
        })
    }
    fn present(&mut self, generation: u64, key: &PreviewKey, now: Instant) -> bool {
        if generation != self.generation
            || key.target.map(|target| target.0) != Some(self.transport.output.document)
            || key.output != self.transport.output.output
            || Some(&key.content) != self.content.as_ref()
        {
            return false;
        }
        if self.transport.mode == PlaybackMode::EveryFrame && self.transport.playing {
            if key.frame != self.request_frame()
                || !self.interval_elapsed(now)
                || (!self.transport.looping
                    && self.presented.is_some()
                    && self.frame() == self.transport.range.bounds(self.frames).1)
            {
                return false;
            }
            self.transport.time =
                Time::new(i64::from(key.frame) * i64::from(self.rate[1]), self.rate[0]).unwrap();
        } else if key.frame != self.frame()
            && (!self.transport.playing
                || key.frame > self.frame()
                || self.presented.is_some_and(|frame| key.frame < frame))
        {
            return false;
        }
        if !self.transport.playing && key.target.map(|target| target.1) != Some(self.transport.time)
        {
            return false;
        }
        if self.transport.mode == PlaybackMode::EveryFrame && self.transport.playing {
            self.pacing = Some(match self.pacing {
                Some((anchor, intervals))
                    if now.duration_since(anchor).as_nanos() * u128::from(self.rate[0])
                        < u128::from(intervals + 1) * 1_000_000_000 * u128::from(self.rate[1]) =>
                {
                    (anchor, intervals + 1)
                }
                _ => (now, 1), // Slow render: discard clock debt, never race to catch up.
            });
        }
        self.presented = Some(key.frame);
        true
    }
    fn advance(&mut self, now: Instant, sample: Option<u64>) -> bool {
        if !self.transport.playing {
            return false;
        }
        if self.transport.mode == PlaybackMode::EveryFrame {
            let ended = !self.transport.looping
                && self.frame() == self.transport.range.bounds(self.frames).1
                && self.presented.is_some()
                && self.interval_elapsed(now);
            if ended {
                self.transport.playing = false;
            }
            return ended;
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
        if frame < self.frame() {
            self.presented = None;
        }
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
    next_generation: u64,
}
impl super::Session {
    pub(super) fn viewer_request_state(&self, id: PanelInstanceId) -> Option<(u64, Time)> {
        let clock = self.viewers.clocks.get(&id)?;
        let time = if clock.transport.mode == PlaybackMode::EveryFrame && clock.transport.playing {
            Time::new(
                i64::from(clock.request_frame()) * i64::from(clock.rate[1]),
                clock.rate[0],
            )
            .ok()?
        } else {
            clock.transport.time
        };
        Some((clock.generation, time))
    }
    pub(super) fn present_viewer_frame(
        &mut self,
        id: PanelInstanceId,
        generation: u64,
        key: &PreviewKey,
    ) -> bool {
        self.viewers
            .clocks
            .get_mut(&id)
            .is_some_and(|clock| clock.present(generation, key, Instant::now()))
    }
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
        self.viewers.next_generation += 1;
        self.viewers.clocks.insert(
            id,
            Clock {
                transport,
                origin,
                anchor: Instant::now(),
                rate: state.rate,
                frames: state.frames,
                content: state.content,
                generation: self.viewers.next_generation,
                presented: None,
                pacing: None,
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
                && clock.transport.mode == PlaybackMode::RealTime
                && clock.transport.output.output == "video"
            {
                let frame = clock
                    .transport
                    .time
                    .to_ticks(clock.rate[0], clock.rate[1], Rounding::Floor)
                    .unwrap_or(0)
                    .max(0) as u32;
                let (start, end) = clock.transport.range.bounds(clock.frames);
                if clock.transport.looping {
                    self.playback.play_looping(
                        self.project.snapshot(),
                        clock.transport.output.document,
                        frame,
                        start,
                        end.saturating_add(1),
                    );
                } else {
                    self.playback.play(
                        self.project.snapshot(),
                        clock.transport.output.document,
                        frame,
                        end.saturating_add(1),
                    );
                }
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
            let monitored = self.viewers.monitor == Some(id)
                && clock.transport.output.output == "video"
                && clock.transport.mode == PlaybackMode::RealTime;
            #[cfg(feature = "desktop")]
            if monitored && self.playback.priming() {
                continue;
            }
            let ended = clock.advance(now, if monitored { sample } else { None });
            restart |= monitored && ended && sample.is_some() && !clock.transport.looping;
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
                mode: PlaybackMode::RealTime,
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
            generation: 1,
            presented: None,
            pacing: None,
        }
    }
    fn key(c: &Clock, frame: u32) -> PreviewKey {
        PreviewKey {
            region: None,
            target: Some((
                c.transport.output.document,
                Time::new(i64::from(frame) * i64::from(c.rate[1]), c.rate[0]).unwrap(),
            )),
            output: "video".into(),
            content: "test".into(),
            frame,
            dimensions: [16, 16],
            view: 1,
            channels: Default::default(),
        }
    }
    fn every(now: Instant, looping: bool) -> Clock {
        let mut c = clock(now, 0, looping);
        c.transport.mode = PlaybackMode::EveryFrame;
        c.content = Some("test".into());
        c
    }
    #[test]
    fn realtime_lookahead_waits_for_clock_and_rejects_retired_generation() {
        let now = Instant::now();
        let mut c = clock(now, 0, false);
        c.content = Some("test".into());
        let next = key(&c, 1);
        assert!(!c.present(1, &next, now));
        assert_eq!(c.frame(), 0);
        c.advance(now + Duration::from_millis(42), None);
        assert!(c.present(1, &next, now + Duration::from_millis(42)));
        c.generation += 1;
        assert!(!c.present(1, &next, now + Duration::from_millis(43)));
    }
    #[cfg(feature = "desktop")]
    #[test]
    fn every_frame_monitor_never_starts_or_primes_the_audio_device() {
        let mut session = crate::session::Session::default();
        let id = PanelInstanceId(1);
        session
            .viewers
            .clocks
            .insert(id, every(Instant::now(), true));
        session.monitor_viewer(Some(id));
        assert!(!session.playback.playing());
        assert!(!session.playback.priming());
        assert!(session.playback.sample().is_none());
        assert!(session.viewers.clocks[&id].transport.playing);
        assert_eq!(session.viewers.clocks[&id].transport.time, Time::ZERO);
    }
    #[test]
    fn slow_every_frame_advances_only_on_presentation_and_does_not_catch_up() {
        let now = Instant::now();
        let mut c = every(now, false);
        let mut realtime = clock(now, 0, false);
        for frame in 0..10 {
            let ready = now + Duration::from_millis(u64::from(frame + 1) * 200);
            let before = c.transport.time;
            c.advance(ready, Some(48_000)); // Audio cannot advance this policy.
            assert_eq!(c.transport.time, before);
            assert_eq!(c.request_frame(), frame);
            assert!(c.present(1, &key(&c, frame), ready));
            assert_eq!(c.frame(), frame);
            assert!(!c.present(1, &key(&c, frame + 1), ready));
            realtime.advance(ready, None);
        }
        assert_eq!(realtime.frame(), 47);
        assert_eq!(c.frame(), 9);
    }
    #[test]
    fn fractional_deadlines_and_loops_remain_exact_with_fast_frames() {
        let now = Instant::now();
        let mut c = every(now, true);
        c.rate = [30_000, 1001];
        c.transport.range.start = Some(0);
        c.transport.range.end = Some(2);
        assert!(c.present(1, &key(&c, 0), now));
        for n in 1..=18_000u64 {
            let nanos = (u128::from(n) * 1001 * 1_000_000_000).div_ceil(30_000) as u64;
            let due = now + Duration::from_nanos(nanos);
            let next = c.request_frame();
            assert_eq!(next, (n % 3) as u32);
            assert!(!c.present(1, &key(&c, next), due - Duration::from_nanos(1)));
            assert!(c.present(1, &key(&c, next), due));
        }
        assert_eq!(c.frame(), 0);
    }
    #[test]
    fn every_frame_last_frame_dwells_then_stops_and_stale_results_are_rejected() {
        let now = Instant::now();
        let mut c = every(now, false);
        c.transport.range.end = Some(1);
        assert!(!c.present(0, &key(&c, 0), now));
        let mut wrong = key(&c, 0);
        wrong.content = "retired edit".into();
        assert!(!c.present(1, &wrong, now));
        assert!(c.present(1, &key(&c, 0), now));
        assert!(!c.present(1, &key(&c, 2), now + Duration::from_secs(1)));
        assert!(c.present(1, &key(&c, 1), now + Duration::from_secs(1)));
        assert!(!c.advance(now + Duration::from_millis(1020), None));
        assert!(c.advance(now + Duration::from_millis(1050), None));
        assert!(!c.transport.playing);
        assert_eq!(c.frame(), 1);
        c.generation = 2;
        assert!(!c.present(1, &key(&c, 1), now + Duration::from_secs(2)));
    }
    #[test]
    fn realtime_accepts_late_frames_without_moving_playhead_or_regressing_picture() {
        let now = Instant::now();
        let mut c = clock(now, 0, false);
        c.content = Some("test".into());
        c.advance(now + Duration::from_millis(500), None);
        assert!(c.present(1, &key(&c, 4), now));
        assert_eq!(c.frame(), 12);
        assert!(!c.present(1, &key(&c, 3), now));
        assert!(!c.present(1, &key(&c, 13), now));
        c.transport.playing = false;
        assert!(!c.present(1, &key(&c, 5), now));
        assert!(c.present(1, &key(&c, 12), now));
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
