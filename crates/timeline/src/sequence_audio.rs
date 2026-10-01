//! Audio-track lowering. Linked A/V share exact source mapping. Project sample
//! boundaries round up; mapped source offsets round down without accumulation.
use crate::{Sequence, SourceMedia};
use fold_foundation::Rounding;
use fold_media::{
    VideoSource, WaveSource,
    audio::{AUDIO_RATE, AudioSource},
};
use fold_project::Snapshot;
use fold_render::audio::{AudioPlan, AudioRegion};
impl Sequence {
    pub fn audio_plan(&self, snapshot: &Snapshot) -> Result<AudioPlan, String> {
        self.validate()?;
        let info = self.info()?;
        let ticks = |time: fold_foundation::Time| {
            time.to_ticks(AUDIO_RATE, 1, Rounding::Ceil)
                .map(|n| n as u64)
                .map_err(|e| e.to_string())
        };
        let end = ticks(info.time(info.frames)?)?;
        let mut regions = Vec::new();
        for clip in &self.clips {
            if !self.audible(self.track(clip.track)?) || clip.level == 0.0 {
                continue;
            }
            let start = ticks(clip.start)?;
            let end = ticks(clip.end()?)?;
            if start == end {
                continue;
            }
            let source_offset = clip
                .source_start
                .checked_sub(clip.start)
                .and_then(|t| t.to_ticks(AUDIO_RATE, 1, Rounding::Floor))
                .map_err(|e| e.to_string())?;
            let asset = snapshot
                .state()
                .assets
                .get(&clip.asset)
                .ok_or("missing audio asset")?;
            let source = match &clip.info {
                SourceMedia::Video(info) => AudioSource::Video(VideoSource {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: info.clone(),
                }),
                SourceMedia::Audio(info) => AudioSource::Wave(WaveSource {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: info.clone(),
                }),
            };
            regions.push(AudioRegion {
                start,
                end,
                source_offset,
                gain: clip.level,
                source,
            });
        }
        let plan = AudioPlan { regions, end };
        plan.validate()?;
        Ok(plan)
    }
}
