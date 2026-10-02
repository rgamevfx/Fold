//! Opt-in bounded diagnostic of the real desktop path, not a replacement renderer.
//! FOLD_NATIVE_PROBE names a new JSON report. Seeks never mutate project content.
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use std::{
    fs::{File, OpenOptions},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) struct Sample {
    pub frame: Option<u32>,
    pub ready: bool,
    pub start_ms: f64,
    pub acquire_ms: f64,
    pub draw_ms: f64,
    pub submit_ms: f64,
    pub submitted_ms: f64,
    pub present_ms: f64,
    pub upload: Option<(f64, f64, usize)>,
    pub completion_ns: Arc<AtomicU64>,
}

pub(crate) struct Probe {
    pub start: Instant,
    epoch_ms: u128,
    report: File,
    samples: Vec<Sample>,
    events: Vec<serde_json::Value>,
    targets: Vec<u32>,
    step: usize,
    issued: Instant,
    ready_since: Option<Instant>,
    finished: bool,
}
impl Probe {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(path) = std::env::var_os("FOLD_NATIVE_PROBE") else {
            return Ok(None);
        };
        Self::create(std::path::Path::new(&path)).map(Some)
    }
    fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let report = OpenOptions::new().write(true).create_new(true).open(path)?;
        let start = Instant::now();
        Ok(Self {
            start,
            epoch_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            report,
            samples: Vec::new(),
            events: Vec::new(),
            targets: (0..10).chain(0..10).chain([12, 29]).collect(),
            step: 0,
            issued: start,
            ready_since: None,
            finished: false,
        })
    }
    pub fn ms(&self, time: Instant) -> f64 {
        time.duration_since(self.start).as_secs_f64() * 1000.
    }
    pub fn push(&mut self, sample: Sample) {
        self.samples.push(sample);
    }
    fn event(&mut self, kind: &str, frame: u32) {
        let event = serde_json::json!({"kind": kind, "frame": frame, "step": self.step,
            "at_ms": self.ms(Instant::now()), "since_command_ms": self.issued.elapsed().as_secs_f64()*1000.});
        // Only step changes, never per-frame logging on the UI thread.
        eprintln!("Fold native probe: {event}");
        self.events.push(event);
    }
    /// Called after present, preserving the ordinary poll/select/draw/submit path.
    pub fn advance(
        &mut self,
        client: &mut dyn DesktopClient,
        frame: Option<u32>,
        ready: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.start.elapsed() > Duration::from_secs(90) || self.samples.len() >= 6000 {
            return Err("native probe timed out or exceeded sample budget".into());
        }
        if client.state().content.is_none() {
            return Ok(false);
        }
        if client.state().dimensions != [1920, 1080]
            || client.state().frames < 30
            || client.state().rate != [30, 1]
        {
            return Err(
                "native probe requires the 1080p30 phase-11 project with >=30 frames".into(),
            );
        }
        let target = self.targets[self.step];
        // Replace a cold request while pending. A completed old request invalidates
        // this cancellation fixture rather than masquerading as cancellation.
        if self.step == 20 {
            if ready && frame == Some(target) {
                return Err(
                    "cancellation source finished before replacement; fixture invalid".into(),
                );
            }
            if self.issued.elapsed() >= Duration::from_millis(50) {
                self.next(client);
                self.event("cancel_replace", 29);
            }
            return Ok(false);
        }
        if self.step == 21 && ready && frame != Some(target) {
            return Err("stale image presented after cancellation".into());
        }
        if ready && frame == Some(target) && self.ready_since.is_none() {
            self.event("first_present_call", target);
            self.ready_since = Some(Instant::now());
        }
        if let Some(since) = self.ready_since {
            if self.step + 1 == self.targets.len() {
                // Keep the final frame visible long enough for screenshot verification.
                if since.elapsed() >= Duration::from_secs(12) {
                    return Ok(true);
                }
            } else if since.elapsed() >= Duration::from_millis(150) {
                self.next(client);
            }
        }
        Ok(false)
    }
    fn next(&mut self, client: &mut dyn DesktopClient) {
        self.step += 1;
        self.issued = Instant::now();
        self.ready_since = None;
        client.command(DesktopCommand::Seek(self.targets[self.step]));
        self.event("seek", self.targets[self.step]);
    }
    pub fn finish(&mut self, success: bool) -> Result<(), Box<dyn std::error::Error>> {
        if self.finished {
            return Ok(());
        }
        let samples: Vec<_> = self.samples.iter().map(|s| {
            let ns = s.completion_ns.load(Ordering::Acquire);
            serde_json::json!({"frame": s.frame, "ready": s.ready,
                "start_ms": s.start_ms, "acquire_ms": s.acquire_ms, "draw_ms": s.draw_ms,
                "submit_ms": s.submit_ms, "present_call_ms": s.present_ms,
                "submitted_ms": s.submitted_ms,
                "submit_to_completion_observed_ms": if ns == 0 { None } else { Some(ns as f64 / 1e6 - s.submitted_ms) },
                "completion_observed_ms": if ns == 0 { None } else { Some(ns as f64 / 1e6) },
                "upload": s.upload.map(|(start, end, bytes)| serde_json::json!({
                    "start_ms": start, "host_ms": end-start, "bytes": bytes,
                    "through_ui_completion_observed_ms": if ns == 0 { None } else {Some(ns as f64/1e6-start)}
                }))})
        }).collect();
        serde_json::to_writer_pretty(
            &mut self.report,
            &serde_json::json!({
                "success": success, "start_unix_ms": self.epoch_ms,
                "events": self.events, "samples": samples,
                "limitations": "completion is callback observation after nonblocking device poll; upload shares submission with UI; present call is not compositor scanout; diagnostic overhead included"
            }),
        )?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_platform::desktop::{DesktopState, PreviewKey, PreviewResult};
    struct Client {
        state: DesktopState,
        seeks: Vec<u32>,
    }
    impl DesktopClient for Client {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn poll(&mut self) {}
        fn command(&mut self, command: DesktopCommand) {
            match command {
                DesktopCommand::Seek(frame) => self.seeks.push(frame),
                _ => panic!("probe must only seek, never mutate a project"),
            }
        }
        fn request_preview(&mut self, _: PreviewKey) {
            unreachable!()
        }
        fn cancel_preview(&mut self) {
            unreachable!()
        }
        fn take_preview(&mut self) -> Option<PreviewResult> {
            unreachable!()
        }
    }
    #[test]
    fn schedule_cancellation_and_report_safety() {
        let path = std::env::temp_dir().join(format!(
            "fold-native-probe-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut probe = Probe::create(&path).unwrap();
        assert!(Probe::create(&path).is_err(), "must not clobber reports");
        let mut client = Client {
            state: DesktopState {
                dimensions: [1920, 1080],
                rate: [30, 1],
                frames: 30,
                content: Some("fixture".into()),
                ..Default::default()
            },
            seeks: Vec::new(),
        };
        for frame in (0..10).chain(0..10) {
            assert!(!probe.advance(&mut client, Some(frame), true).unwrap());
            probe.ready_since = Some(Instant::now() - Duration::from_millis(151));
            assert!(!probe.advance(&mut client, Some(frame), true).unwrap());
        }
        assert_eq!(probe.step, 20);
        assert!(probe.advance(&mut client, Some(12), true).is_err());
        probe.issued = Instant::now() - Duration::from_millis(51);
        assert!(!probe.advance(&mut client, Some(9), false).unwrap());
        assert_eq!(probe.step, 21);
        assert!(probe.advance(&mut client, Some(12), true).is_err());
        assert!(!probe.advance(&mut client, Some(29), true).unwrap());
        probe.ready_since = Some(Instant::now() - Duration::from_secs(13));
        assert!(probe.advance(&mut client, Some(29), true).unwrap());
        assert_eq!(
            client.seeks,
            (1..10).chain(0..10).chain([12, 29]).collect::<Vec<_>>()
        );
        drop(probe); // An unfinished/aborted report must not claim success.
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["success"], false);
        std::fs::remove_file(path).unwrap();
    }
}
