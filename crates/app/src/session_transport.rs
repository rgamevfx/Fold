//! Legacy non-instance command adapter for headless clients and fixtures.
//! Native panels use `viewer_transport`; this adapter cannot take their audio device.
use super::Session;
use fold_foundation::Time;
use fold_platform::desktop::TransportAction;

impl Session {
    pub(super) fn stop_playback(&mut self) {
        #[cfg(feature = "desktop")]
        if self.viewers.monitor.is_none() {
            self.playback.pause();
        }
        self.state.playing = false;
        self.state.priming = false;
    }
    pub(super) fn start_playback(&mut self) {
        if !self.viewers.clocks.is_empty() {
            self.state.status = "Choose a viewer for playback".into();
            return;
        }
        if self.overlay.is_some() {
            self.state.status = "Finish the current edit before playing.".into();
            return;
        }
        let Some(document) = self.state.viewer_document else {
            return;
        };
        let (start, end) = self.state.playback_range.bounds(self.state.frames);
        if self.state.frame < start || self.state.frame >= end {
            self.seek(start);
        }
        #[cfg(feature = "desktop")]
        {
            self.playback.play(
                self.project.snapshot(),
                document,
                self.state.frame,
                end.saturating_add(1),
            );
            self.state.playing = true;
            self.state.priming = true;
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = document;
            self.state.status = "Playback requires the desktop feature".into();
        }
    }
    pub(super) fn seek(&mut self, frame: u32) {
        self.state.frame = frame.min(self.state.frames.saturating_sub(1));
        if let Some(location) = self.state.navigation.last_mut() {
            location.time = Time::new(
                i64::from(self.state.frame) * i64::from(self.state.rate[1]),
                self.state.rate[0],
            )
            .unwrap_or(Time::ZERO);
        }
    }
    pub(super) fn transport(&mut self, action: TransportAction) {
        let Some(document) = self.state.viewer_document else {
            return;
        };
        use TransportAction::*;
        let (start, end) = self.state.playback_range.bounds(self.state.frames);
        let frame = match action {
            TogglePlay => {
                if self.state.playing {
                    self.stop_playback();
                } else {
                    self.start_playback();
                }
                return;
            }
            MarkIn | MarkOut => {
                let frame = self.state.frame;
                let range = &mut self.state.playback_range;
                if matches!(action, MarkIn) {
                    range.start = Some(frame);
                    if range.end.is_some_and(|end| end < frame) {
                        range.end = Some(frame);
                    }
                } else {
                    range.end = Some(frame);
                    if range.start.is_some_and(|start| start > frame) {
                        range.start = Some(frame);
                    }
                }
                self.playback_ranges.insert(document, *range);
                if self.state.playing {
                    self.start_playback();
                }
                return;
            }
            Stop | GoToIn => start,
            GoToOut => end,
            PreviousFrame => self.state.frame.saturating_sub(1),
            NextFrame => self.state.frame.saturating_add(1),
            Jump(frame) => frame,
        };
        self.stop_playback();
        self.seek(frame);
    }
}
