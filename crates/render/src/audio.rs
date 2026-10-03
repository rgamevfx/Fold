//! Separate offline audio execution plan. Signed source offsets are in 48 kHz
//! samples; output is interleaved stereo f32, zero latency, half-open ranges.
use fold_media::{
    Cancel,
    audio::{AUDIO_RATE, AudioDecoder, AudioSource, MAX_AUDIO_SAMPLES},
};

#[derive(Clone, Debug)]
pub struct AudioRegion {
    pub start: u64,
    pub end: u64,
    pub source_offset: i64,
    pub source: AudioSource,
    pub gain: f32,
}
#[derive(Clone, Debug)]
pub struct AudioPlan {
    pub regions: Vec<AudioRegion>,
    pub end: u64,
}
impl AudioPlan {
    /// Shorten a half-open playback range without rebasing source offsets.
    pub fn clip_end(&mut self, end: u64) -> Result<(), String> {
        self.validate()?;
        if end > self.end {
            return Err("audio review range exceeds plan".into());
        }
        self.regions.retain(|region| region.start < end);
        for region in &mut self.regions {
            region.end = region.end.min(end);
        }
        self.end = end;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.end > MAX_AUDIO_SAMPLES || self.regions.len() > 4096 {
            return Err("audio plan exceeds budget".into());
        }
        for region in &self.regions {
            if region.start >= region.end
                || region.end > self.end
                || !region.gain.is_finite()
                || !(0.0..=1.0).contains(&region.gain)
                || region
                    .source_offset
                    .checked_add(region.end as i64)
                    .is_none()
            {
                return Err("invalid audio region".into());
            }
            region.source.validate()?;
        }
        Ok(())
    }
    pub fn preflight(&self, decoder: &mut AudioDecoder, cancel: &Cancel) -> Result<(), String> {
        cancel.check()?;
        self.validate()?;
        decoder.retain_sources(self.regions.iter().map(|region| &region.source));
        for region in &self.regions {
            decoder.preflight(&region.source, cancel)?;
        }
        Ok(())
    }
    pub fn evaluate(
        &self,
        start: u64,
        count: usize,
        decoder: &mut AudioDecoder,
        cancel: &Cancel,
    ) -> Result<Vec<[f32; 2]>, String> {
        self.validate()?;
        cancel.check()?;
        if count > AUDIO_RATE as usize || start > self.end || count as u64 > self.end - start {
            return Err("invalid audio demand".into());
        }
        let mut output = vec![[0.0; 2]; count];
        let end = start + count as u64;
        for region in &self.regions {
            let left = start.max(region.start);
            let right = end.min(region.end);
            if left >= right {
                continue;
            }
            let samples = decoder.read(
                &region.source,
                left as i64 + region.source_offset,
                (right - left) as usize,
                cancel,
            )?;
            for (dst, src) in output[(left - start) as usize..(right - start) as usize]
                .iter_mut()
                .zip(samples)
            {
                for ch in 0..2 {
                    dst[ch] += src[ch] * region.gain;
                }
            }
        }
        for frame in &mut output {
            for sample in frame {
                *sample = sample.clamp(-1.0, 1.0);
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortened_review_clips_regions_and_preserves_source_mapping() {
        let source = AudioSource::Wave(fold_media::WaveSource {
            path: "fixture.wav".into(),
            fingerprint: "fixture".into(),
            info: fold_media::WaveInfo {
                rate: 48_000,
                channels: 2,
                frames: 96_000,
            },
        });
        let mut plan = AudioPlan {
            end: 96_000,
            regions: vec![
                AudioRegion {
                    start: 0,
                    end: 48_000,
                    source_offset: 400,
                    source: source.clone(),
                    gain: 1.,
                },
                AudioRegion {
                    start: 48_000,
                    end: 96_000,
                    source_offset: -48_000,
                    source,
                    gain: 1.,
                },
            ],
        };
        plan.clip_end(1200).unwrap();
        plan.validate().unwrap();
        assert_eq!(plan.regions.len(), 1);
        assert_eq!(plan.regions[0].end, 1200);
        assert_eq!(plan.regions[0].source_offset, 400);
        assert!(plan.clip_end(1201).is_err());
        plan.clip_end(0).unwrap();
        assert!(plan.regions.is_empty());
    }
}
