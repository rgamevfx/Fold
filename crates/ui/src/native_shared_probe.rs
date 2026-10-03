//! Opt-in desktop acceptance driver; uses ordinary transports, demands and caches.
use crate::{preview::PreviewHost, shell::Shell};
use fold_foundation::Time;
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand as C, PlaybackMode, PlaybackRange, ViewerTransport},
    workspace::{DocumentRef, PanelInstanceId, ViewerBinding},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) struct Shared {
    start: Instant,
    directory: PathBuf,
    ids: Option<[PanelInstanceId; 2]>,
    output: Option<DocumentRef>,
    frames: u32,
    phase: u8,
    began: Instant,
    phase_started: Instant,
    last: BTreeMap<PanelInstanceId, u32>,
    pub events: Vec<serde_json::Value>,
    admissions: [u32; 2],
    skips: [u32; 2],
    underruns: [Option<u64>; 2],
    seek: usize,
    export_started: Option<Instant>,
    pub phases: Vec<serde_json::Value>,
}
impl Shared {
    pub fn new(directory: PathBuf, start: Instant) -> Self {
        Self {
            start,
            directory,
            ids: None,
            output: None,
            frames: 0,
            phase: 0,
            began: Instant::now(),
            phase_started: Instant::now(),
            last: Default::default(),
            events: vec![],
            admissions: [0; 2],
            skips: [0; 2],
            underruns: [None; 2],
            seek: 0,
            export_started: None,
            phases: vec![],
        }
    }
    fn transport(
        &self,
        id: PanelInstanceId,
        frame: u32,
        playing: bool,
        mode: PlaybackMode,
        range: PlaybackRange,
        shell: &mut Shell,
        client: &mut dyn DesktopClient,
    ) {
        let output = self.output.clone().unwrap();
        let time = Time::new(i64::from(frame), 30).unwrap();
        let viewer = shell.workspace.viewers.get_mut(&id).unwrap();
        viewer.binding = ViewerBinding::Pinned(output.clone());
        viewer.last_output = Some(output.clone());
        viewer.playback_mode = Some(mode);
        viewer.time = time;
        viewer.playing = playing;
        viewer.range = range;
        viewer.looping = playing;
        client.command(C::ViewerTransport {
            viewer: id,
            transport: ViewerTransport {
                output,
                time,
                range,
                playing,
                mode,
                looping: playing,
            },
        });
    }
    fn begin(&mut self, phase: u8) {
        self.phase = phase;
        self.began = Instant::now();
        self.phase_started = self.began;
        self.last.clear();
        self.admissions = [0; 2];
        self.skips = [0; 2];
        self.underruns = [None; 2];
        self.events
            .push(serde_json::json!({"kind":"phase", "phase":phase,
                "at_ms":self.start.elapsed().as_secs_f64()*1000.}));
    }
    fn record(&mut self, client: &dyn DesktopClient, preview: &PreviewHost) {
        self.phases.push(serde_json::json!({"phase":self.phase,
            "seconds":self.phase_started.elapsed().as_secs_f64(), "admissions":self.admissions,
            "skipped_between_admissions":self.skips, "cache":preview.statistics(),
            "max_observed_underrun_callbacks":self.underruns,
            "resources":client.resource_statistics(), "audio_errors":self.ids.unwrap().map(|id| client.viewer_audio_error(id)) }));
    }
    pub fn advance(
        &mut self,
        shell: &mut Shell,
        preview: &mut PreviewHost,
        client: &mut dyn DesktopClient,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.start.elapsed() > Duration::from_secs(180) {
            return Err("shared probe exceeded its total observation deadline".into());
        }
        if self.ids.is_none() {
            let Ok((document, info)) = client.output_info() else {
                return Ok(false);
            };
            if info.width != 1920
                || info.height != 1080
                || info.rate != [30, 1]
                || info.frames < 450
            {
                return Err("shared probe requires two-layer/title/stereo 1080p30 fixture with >=450 frames".into());
            }
            let Some(a) = shell.workspace.viewers.keys().next().copied() else {
                return Ok(false);
            };
            let output = DocumentRef {
                document,
                output: "video".into(),
                extensions: Default::default(),
            };
            let b = shell.probe_add_viewer(output.clone());
            self.ids = Some([a, b]);
            self.output = Some(output);
            self.frames = info.frames;
            shell.workspace.monitored_viewer = Some(a);
            client.command(C::MonitorViewer(Some(a)));
            self.transport(
                a,
                0,
                true,
                PlaybackMode::RealTime,
                Default::default(),
                shell,
                client,
            );
            self.transport(
                b,
                240,
                false,
                PlaybackMode::EveryFrame,
                Default::default(),
                shell,
                client,
            );
            self.begin(0);
            return Ok(false);
        }
        let [a, b] = self.ids.unwrap();
        for (index, id) in [a, b].into_iter().enumerate() {
            if let Some(count) = client.viewer_audio_underruns(id) {
                self.underruns[index] = Some(self.underruns[index].unwrap_or(0).max(count));
            }
            if let Some(error) = client
                .viewer_audio_error(id)
                .or_else(|| preview.viewer_error(id).map(str::to_owned))
            {
                return Err(error.into());
            }
            if let Some(key) = preview.presented_key(id) {
                if key.dimensions != [1920, 1080] {
                    return Err("shared probe received reduced preview resolution".into());
                }
                if self.last.get(&id) != Some(&key.frame) {
                    self.admissions[index] += 1;
                    if let Some(old) = self.last.insert(id, key.frame) {
                        let advance = if key.frame < old
                            && client
                                .viewer_transport(id)
                                .is_some_and(|t| t.playing && t.looping)
                        {
                            self.frames - old + key.frame
                        } else {
                            key.frame.saturating_sub(old)
                        };
                        self.skips[index] += advance.saturating_sub(1);
                    }
                }
            }
        }
        let elapsed = self.began.elapsed();
        match self.phase {
            0 if client.viewer_transport(a).unwrap().time > Time::new(1, 2).unwrap()
                && preview.presented_key(a).is_some_and(|key| key.frame > 12) =>
            {
                self.record(client, preview);
                self.begin(1);
            }
            1 if elapsed >= Duration::from_secs(30) => {
                self.record(client, preview);
                self.transport(
                    a,
                    0,
                    true,
                    PlaybackMode::RealTime,
                    Default::default(),
                    shell,
                    client,
                );
                self.transport(
                    b,
                    240,
                    true,
                    PlaybackMode::EveryFrame,
                    Default::default(),
                    shell,
                    client,
                );
                let image = self.directory.join("linked.ppm");
                std::fs::write(&image, b"P6\n1 1\n255\n\x10\x20\x30")?;
                client.command(C::Export {
                    path: self.directory.join("delivery.mp4"),
                    start: 0,
                    end: 150,
                });
                client.command(C::Browser(fold_platform::browser::BrowserCommand::Import {
                    paths: vec![image],
                    destination: Default::default(),
                }));
                self.export_started = Some(Instant::now());
                self.begin(2);
            }
            2 if elapsed >= Duration::from_secs(10) && !client.state().busy => {
                if !self.directory.join("delivery.mp4").exists() {
                    return Err(client.state().status.clone().into());
                }
                self.record(client, preview);
                self.events.push(serde_json::json!({"kind":"export", "frames":150,
                    "through_completion_observed_seconds":self.export_started.unwrap().elapsed().as_secs_f64()}));
                self.transport(
                    a,
                    4,
                    false,
                    PlaybackMode::RealTime,
                    Default::default(),
                    shell,
                    client,
                );
                self.transport(
                    b,
                    6,
                    false,
                    PlaybackMode::EveryFrame,
                    Default::default(),
                    shell,
                    client,
                );
                self.begin(3);
            }
            3 => {
                const SEEKS: [u32; 6] = [4, 5, 6, 4, 5, 6];
                if preview
                    .presented_key(a)
                    .is_some_and(|key| key.frame == SEEKS[self.seek])
                {
                    self.events.push(serde_json::json!({"kind":"seek", "frame":SEEKS[self.seek],
                        "warm_repeat":self.seek >= 3, "through_present_call_ms":elapsed.as_secs_f64()*1000.}));
                    self.seek += 1;
                    self.began = Instant::now();
                    if self.seek == SEEKS.len() {
                        self.record(client, preview);
                        self.transport(
                            a,
                            0,
                            false,
                            PlaybackMode::RealTime,
                            PlaybackRange {
                                start: Some(0),
                                end: Some(29),
                            },
                            shell,
                            client,
                        );
                        // Selection refresh admits the new generation/range before preparation.
                        let demands = shell.keys(client);
                        preview.select_viewers(&demands, client);
                        preview.review_action(a, crate::review::Action::Prepare, client);
                        self.begin(4);
                    } else {
                        self.transport(
                            a,
                            SEEKS[self.seek],
                            false,
                            PlaybackMode::RealTime,
                            Default::default(),
                            shell,
                            client,
                        );
                    }
                }
            }
            4 if preview.review_state(a).1 => {
                self.record(client, preview);
                preview.review_action(a, crate::review::Action::PlayCached, client);
                if !client.viewer_transport(a).unwrap().playing {
                    return Err("cached replay did not start".into());
                }
                self.begin(5);
            }
            5 if elapsed >= Duration::from_millis(1500) => {
                self.record(client, preview);
                if self.admissions[0] < 20 {
                    return Err("cached replay did not present enough distinct frames".into());
                }
                if client.viewer_transport(a).unwrap().mode != PlaybackMode::RealTime {
                    return Err("cached replay changed playback mode".into());
                }
                // Changing seek generation must interrupt the previously prepared range.
                self.transport(
                    a,
                    3,
                    false,
                    PlaybackMode::RealTime,
                    PlaybackRange {
                        start: Some(0),
                        end: Some(29),
                    },
                    shell,
                    client,
                );
                let demands = shell.keys(client);
                preview.select_viewers(&demands, client);
                if preview.review_state(a).1 {
                    return Err("seek failed to invalidate review intent".into());
                }
                self.events
                    .push(serde_json::json!({"kind":"review_seek_interrupted"}));
                return Ok(true);
            }
            _ => {}
        }
        if elapsed > Duration::from_secs(60) {
            return Err(format!("shared probe phase {} timeout", self.phase).into());
        }
        Ok(false)
    }
}
