//! Per-Read timing and interpretation, shared by preview and delivery.
use fold_foundation::Time;
use fold_media::exr_sequence::Policy;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameMode {
    #[default]
    StartAt,
    Offset,
}
pub use fold_render::operations::SourceAlpha as Alpha;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadOptions {
    pub frame_mode: FrameMode,
    pub before: Policy,
    pub after: Policy,
    pub alpha: Alpha,
    pub raw: bool,
    pub proxy: Option<crate::Source>,
    pub use_proxy: bool,
}
impl ReadOptions {
    pub fn mapped_time(
        &self,
        time: Time,
        start: Time,
        first: Time,
        duration: Time,
        rate: [u32; 2],
    ) -> Result<Option<Time>, String> {
        let end = start.checked_add(duration).map_err(|e| e.to_string())?;
        let policy = if time < start {
            Some(self.before)
        } else if time >= end {
            Some(self.after)
        } else {
            None
        };
        match policy {
            Some(Policy::Black) => Ok(None),
            Some(Policy::Error) => Err("Read frame is outside its range".into()),
            Some(Policy::Hold) => Ok(Some(if time < start {
                first
            } else {
                let end = first.checked_add(duration).map_err(|e| e.to_string())?;
                let frame = end
                    .to_ticks(rate[0], rate[1], fold_foundation::Rounding::Ceil)
                    .map_err(|e| e.to_string())?
                    - 1;
                Time::new(
                    frame
                        .checked_mul(i64::from(rate[1]))
                        .ok_or("Read frame overflow")?,
                    rate[0],
                )
                .map_err(|e| e.to_string())?
            })),
            None => Ok(Some(
                time.checked_sub(start)
                    .and_then(|t| t.checked_add(first))
                    .map_err(|e| e.to_string())?,
            )),
        }
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            frame_mode: FrameMode::StartAt,
            before: Policy::Black,
            after: Policy::Black,
            alpha: Alpha::Premultiplied,
            raw: false,
            proxy: None,
            use_proxy: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_before_after_policies_use_exact_source_frame_boundaries() {
        let frame = |n: i64| Time::new(n * 1001, 24000).unwrap();
        let options = ReadOptions {
            before: Policy::Hold,
            after: Policy::Black,
            ..Default::default()
        };
        assert_eq!(
            options
                .mapped_time(frame(9), frame(10), frame(1001), frame(4), [24000, 1001])
                .unwrap(),
            Some(frame(1001))
        );
        assert_eq!(
            options
                .mapped_time(frame(12), frame(10), frame(1001), frame(4), [24000, 1001])
                .unwrap(),
            Some(frame(1003))
        );
        assert_eq!(
            options
                .mapped_time(frame(14), frame(10), frame(1001), frame(4), [24000, 1001])
                .unwrap(),
            None
        );
        let options = ReadOptions {
            after: Policy::Hold,
            ..options
        };
        assert_eq!(
            options
                .mapped_time(frame(90), frame(10), frame(1001), frame(4), [24000, 1001])
                .unwrap(),
            Some(frame(1004))
        );
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceOptions {
    pub still: bool,
    pub missing: Policy,
}
