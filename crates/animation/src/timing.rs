//! Duration fitting shared by hosts and graphs. No editor or playback state.
use fold_foundation::Time;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum Behavior {
    #[default]
    Clamp,
    Repeat,
    PingPong,
}
impl Behavior {
    pub fn map(self, progress: f64) -> Result<f64, String> {
        if !progress.is_finite() {
            return Err("Timing progress must be finite".into());
        }
        Ok(match self {
            Self::Clamp => progress.clamp(0., 1.),
            Self::Repeat => progress.rem_euclid(1.),
            Self::PingPong => 1. - (progress.rem_euclid(2.) - 1.).abs(),
        })
    }
}

/// Range in the caller's clock. Inclusive curve endpoints are independent of
/// the host's half-open activity range; a clamped endpoint never wraps to zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Range {
    pub start: Time,
    pub duration: Time,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub progress: f64,
    pub elapsed: f64,
    pub duration: f64,
}
impl Range {
    pub fn validate(self) -> Result<(), String> {
        if self.duration <= Time::ZERO {
            return Err("Timing duration must be positive".into());
        }
        self.start
            .checked_add(self.duration)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn sample(self, time: Time, behavior: Behavior) -> Result<Sample, String> {
        self.validate()?;
        // Widen before subtracting: converting absolute times to f64 first
        // loses subframe offsets at large origins.
        let elapsed = super::interval(self.start, time);
        let duration = super::seconds(self.duration);
        Ok(Sample {
            progress: behavior.map(elapsed / duration)?,
            elapsed,
            duration,
        })
    }
}

/// Unit-domain curve used by Timing and the oscillator. Validation happens at
/// authoring/compilation; sampling performs no allocation or curve cloning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Curve {
    pub curve: Vec<[f64; 2]>,
    pub smooth: bool,
}
impl Default for Curve {
    fn default() -> Self {
        Self {
            curve: vec![[0., 0.], [1., 1.]],
            smooth: false,
        }
    }
}
impl Curve {
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=64).contains(&self.curve.len())
            || self
                .curve
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || self.curve.first().unwrap()[0] != 0.
            || self.curve.last().unwrap()[0] != 1.
            || self.curve.windows(2).any(|p| p[0][0] >= p[1][0])
        {
            return Err("Curve requires ordered points in 0–1, with endpoints at 0 and 1".into());
        }
        Ok(())
    }
    /// Sample a validated curve at a finite normalized coordinate.
    pub fn sample(&self, progress: f64) -> f64 {
        let p = progress.clamp(0., 1.);
        let end = self
            .curve
            .partition_point(|v| v[0] <= p)
            .clamp(1, self.curve.len() - 1);
        let [a, b] = [self.curve[end - 1], self.curve[end]];
        let mut u = (p - a[0]) / (b[0] - a[0]);
        if self.smooth {
            u = u * u * (3. - 2. * u);
        }
        a[1] + (b[1] - a[1]) * u
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(n: i64, d: u32) -> Time {
        Time::new(n, d).unwrap()
    }
    #[test]
    fn endpoints_reverse_seeks_and_repeat_are_distinct() {
        let range = Range {
            start: t(10, 1),
            duration: t(2, 1),
        };
        for (time, expected) in [
            (t(12, 1), 1.),
            (t(11, 1), 0.5),
            (t(9, 1), 0.),
            (t(13, 1), 1.),
        ] {
            assert_eq!(
                range.sample(time, Behavior::Clamp).unwrap().progress,
                expected
            );
        }
        assert_eq!(
            range.sample(t(12, 1), Behavior::Repeat).unwrap().progress,
            0.
        );
        assert_eq!(
            range.sample(t(9, 1), Behavior::Repeat).unwrap().progress,
            0.5
        );
        assert_eq!(
            range.sample(t(12, 1), Behavior::PingPong).unwrap().progress,
            1.
        );
        assert_eq!(
            range.sample(t(14, 1), Behavior::PingPong).unwrap().progress,
            0.
        );
    }
    #[test]
    fn exact_origin_and_half_open_frame_sampling() {
        let range = Range {
            start: t(1_i64 << 54, 1),
            duration: t(1, 1),
        };
        assert_eq!(
            range
                .sample(t((1_i64 << 54) + 1, 1), Behavior::Clamp)
                .unwrap()
                .progress,
            1.
        );
        let range = Range {
            start: Time::ZERO,
            duration: t(1001, 1000),
        };
        assert!(
            (range
                .sample(t(23 * 1001, 24000), Behavior::Clamp)
                .unwrap()
                .progress
                - 23. / 24.)
                .abs()
                < 1e-12
        );
        assert!(
            Range {
                duration: Time::ZERO,
                ..range
            }
            .validate()
            .is_err()
        );
        assert!(Behavior::Clamp.map(f64::NAN).is_err());
    }
    #[test]
    fn curve_endpoint_is_not_a_cycle_boundary() {
        let mut curve = Curve::default();
        curve.validate().unwrap();
        assert_eq!(curve.sample(1.), 1.);
        assert_eq!(curve.sample(0.25), 0.25);
        curve.smooth = true;
        assert_eq!(curve.sample(0.25), 0.15625);
        curve.curve.insert(1, [0., 0.5]);
        assert!(curve.validate().is_err());
    }
}

/// Three composable template phases. Intro/outro retain their requested seconds;
/// a short total duration compresses both proportionally and leaves no hold.
/// Returned values are normalized intro, hold and outro progress, respectively.
pub fn segments(elapsed: f64, duration: f64, intro: f64, outro: f64) -> Result<[f64; 3], String> {
    if ![elapsed, duration, intro, outro]
        .iter()
        .all(|v| v.is_finite())
        || duration <= 0.
        || intro < 0.
        || outro < 0.
    {
        return Err(
            "Template segments require a positive duration and nonnegative finite intro/outro"
                .into(),
        );
    }
    let sum = intro + outro;
    if !sum.is_finite() {
        return Err("Template segment duration overflow".into());
    }
    let scale = if sum > duration { duration / sum } else { 1. };
    let a = intro * scale;
    let b = outro * scale;
    let hold = (duration - a - b).max(0.);
    let phase = |time: f64, length: f64| {
        if length == 0. {
            if time >= 0. { 1. } else { 0. }
        } else {
            (time / length).clamp(0., 1.)
        }
    };
    Ok([
        phase(elapsed, a),
        phase(elapsed - a, hold),
        phase(elapsed - (duration - b), b),
    ])
}

#[cfg(test)]
mod segment_tests {
    use super::*;
    #[test]
    fn intro_and_outro_keep_seconds_and_compress_proportionally() {
        assert_eq!(segments(0.5, 10., 1., 2.).unwrap(), [0.5, 0., 0.]);
        assert_eq!(segments(9., 10., 1., 2.).unwrap(), [1., 1., 0.5]);
        assert_eq!(segments(0.25, 1.5, 1., 2.).unwrap(), [0.5, 0., 0.]);
        assert_eq!(segments(1., 1.5, 1., 2.).unwrap(), [1., 1., 0.5]);
        assert_eq!(segments(-1., 1., 0., 0.).unwrap(), [0., 0., 0.]);
        assert!(segments(0., 0., 1., 1.).is_err());
    }
}
