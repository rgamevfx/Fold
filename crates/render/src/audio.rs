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
