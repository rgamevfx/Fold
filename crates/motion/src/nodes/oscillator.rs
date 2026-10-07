//! Periodic numeric fields. Time is explicit; consuming domains supply copy order.
use super::*;
use crate::{evaluation::field, fields::Field};
use serde::{Deserialize, Serialize};

pub const SHAPES: &[&str] = &["Sine", "Triangle", "Square", "Sawtooth", "Custom"];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(flatten)]
    pub curve_data: fold_animation::timing::Curve,
}
impl std::ops::Deref for Settings {
    type Target = fold_animation::timing::Curve;
    fn deref(&self) -> &Self::Target {
        &self.curve_data
    }
}
impl std::ops::DerefMut for Settings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.curve_data
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            curve_data: fold_animation::timing::Curve {
                curve: vec![[0., 0.5], [0.25, 1.], [0.75, 0.], [1., 0.5]],
                smooth: true,
            },
        }
    }
}
impl Settings {
    /// Periodic behavior belongs to Oscillator, not the shared curve.
    pub fn sample(&self, shape: &str, phase: f64) -> Result<f64, String> {
        let p = fold_animation::timing::Behavior::Repeat.map(phase)?;
        Ok(match shape {
            "Sine" => 0.5 + 0.5 * (p * std::f64::consts::TAU).sin(),
            "Triangle" => 1. - (2. * p - 1.).abs(),
            "Square" => {
                if p < 0.5 {
                    1.
                } else {
                    0.
                }
            }
            "Sawtooth" => p,
            "Custom" => self.curve_data.sample(p),
            _ => return Err("Unknown oscillator shape".into()),
        })
    }
}

pub fn definition_oscillator() -> Definition {
    let mut d = definition(
        "fold.motion.oscillator",
        "Oscillator",
        "Animation",
        vec![
            scalar("time", 0., "seconds"),
            Socket::value("shape", Datum::Text("Sine".into()), ""),
            scalar("minimum", -1., ""),
            scalar("maximum", 1., ""),
            Socket::value("time_mode", Datum::Text("Seconds".into()), ""),
            scalar("duration", 2., "seconds"),
            scalar("bpm", 60., "BPM"),
            scalar("phase", 0., "cycles"),
            scalar("time_scale", 1., "factor"),
            scalar("time_offset", 0., "seconds"),
            scalar("phase_spread", 0., "cycles across copies"),
            scalar("channel_phase", 0., "cycles / channel"),
            scalar("strength", 1., "factor"),
        ],
        vec![("value", Kind::Generic)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            let shape = match e.uniform(n, "shape")? {
                Datum::Text(s) => s,
                _ => return Err("Expected wave shape".into()),
            };
            settings.sample(&shape, 0.)?;
            let mode = match e.uniform(n, "time_mode")? {
                Datum::Text(s) => s,
                _ => return Err("Expected time mode".into()),
            };
            if !matches!(mode.as_str(), "Seconds" | "BPM") {
                return Err("Unknown oscillator time mode".into());
            }
            let min = e.field(n, "minimum")?;
            let max = e.field(n, "maximum")?;
            let time = e.field(n, "time")?;
            let duration = e.field(n, "duration")?;
            let bpm = e.field(n, "bpm")?;
            let phase = e.field(n, "phase")?;
            let scale = e.field(n, "time_scale")?;
            let offset = e.field(n, "time_offset")?;
            let strength = e.field(n, "strength")?;
            let spread = e.scalar(n, "phase_spread")?;
            let channel = e.scalar(n, "channel_phase")?;
            let varying = spread != 0.
                || [
                    &min, &max, &time, &duration, &bpm, &phase, &scale, &offset, &strength,
                ]
                .iter()
                .any(|f| f.varying);
            Ok(field(Field::new(min.kind, varying, move |s, budget| {
                let frequency = if mode == "BPM" {
                    let bpm = bpm.sample(s, budget)?.scalar()?;
                    if bpm <= 0. {
                        return Err("Tempo must be greater than zero".into());
                    }
                    bpm / 60.
                } else {
                    let duration = duration.sample(s, budget)?.scalar()?;
                    if duration <= 0. {
                        return Err("Cycle duration must be greater than zero".into());
                    }
                    1. / duration
                };
                let t = (time.sample(s, budget)?.scalar()? + offset.sample(s, budget)?.scalar()?)
                    * scale.sample(s, budget)?.scalar()?
                    * frequency
                    + phase.sample(s, budget)?.scalar()?
                    + spread * s.index as f64 / s.count.saturating_sub(1).max(1) as f64;
                if !t.is_finite() {
                    return Err("Oscillator time exceeds supported range".into());
                }
                let a = min.sample(s, budget)?;
                let b = max.sample(s, budget)?;
                let strength = strength.sample(s, budget)?.scalar()?;
                let component = |a: f64, b: f64, i: usize| -> Result<f64, String> {
                    let weight =
                        0.5 + (settings.sample(&shape, t + i as f64 * channel)? - 0.5) * strength;
                    Ok(a + (b - a) * weight)
                };
                Ok(match (a, b) {
                    (Datum::Scalar(a), Datum::Scalar(b)) => Datum::Scalar(component(a, b, 0)?),
                    (Datum::Vector(a), Datum::Vector(b)) => {
                        Datum::Vector([component(a[0], b[0], 0)?, component(a[1], b[1], 1)?])
                    }
                    (Datum::Color(a), Datum::Color(b)) => Datum::Color([
                        component(a[0], b[0], 0)?,
                        component(a[1], b[1], 1)?,
                        component(a[2], b[2], 2)?,
                        component(a[3], b[3], 3)?,
                    ]),
                    _ => return Err("Oscillator requires matching numeric ranges".into()),
                })
            })))
        },
    );
    d.inputs[2].kind = Kind::Generic;
    d.inputs[3].kind = Kind::Generic;
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.validate = |n| n.settings::<Settings>()?.validate();
    d
}
