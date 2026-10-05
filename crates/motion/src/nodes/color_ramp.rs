//! A scalar-to-color mapping independent of the source of its coordinate.
use super::*;
use crate::{evaluation::field, fields::Field, geometry::path::Stop};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Interpolation {
    #[default]
    Linear,
    Smooth,
    Stepped,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub stops: Vec<Stop>,
    #[serde(default)]
    pub interpolation: Interpolation,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            stops: vec![stop(0., [0., 0., 0., 1.]), stop(1., [1.; 4])],
            interpolation: Interpolation::Linear,
        }
    }
}
pub fn stop(position: f64, color: [f64; 4]) -> Stop {
    Stop {
        position,
        value: Datum::Color(color),
        extensions: Default::default(),
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.stops.is_empty() || self.stops.len() > 1024 {
            return Err("Color Ramp needs 1–1024 stops".into());
        }
        for (i, stop) in self.stops.iter().enumerate() {
            stop.value.validate()?;
            let color = stop.value.color()?;
            if !(0. ..=1.).contains(&stop.position)
                || !(0. ..=1.).contains(&color[3])
                || (i > 0 && self.stops[i - 1].position >= stop.position)
            {
                return Err("Ramp stops need increasing positions and opacity in 0–1".into());
            }
        }
        Ok(())
    }
}
/// Values outside the stop range hold the nearest endpoint. Interpolate straight
/// RGBA in the document's working space, preserving HDR RGB and bounded opacity.
pub fn sample(stops: &[Stop], mode: Interpolation, value: f64) -> Result<Datum, String> {
    if !value.is_finite() {
        return Err("Ramp coordinate must be finite".into());
    }
    let first = stops.first().ok_or("Empty color ramp")?;
    let end = stops.partition_point(|s| s.position <= value);
    if end == 0 {
        return Ok(first.value.clone());
    }
    if end == stops.len() {
        return Ok(stops[end - 1].value.clone());
    }
    let (a, b) = (&stops[end - 1], &stops[end]);
    let mut weight = (value - a.position) / (b.position - a.position);
    weight = match mode {
        Interpolation::Linear => weight,
        Interpolation::Smooth => weight * weight * (3. - 2. * weight),
        Interpolation::Stepped => 0.,
    };
    a.value.mix(&b.value, weight)
}
pub fn definitions() -> Vec<Definition> {
    let mut ramp = definition(
        "fold.motion.color_ramp",
        "Color Ramp",
        "Values",
        vec![scalar("value", 0., "0–1")],
        vec![("value", Kind::Color)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            Ok(field(e.field(n, "value")?.map(Kind::Color, move |v| {
                sample(&settings.stops, settings.interpolation, v.scalar()?)
            })))
        },
    );
    ramp.defaults = || serde_json::to_value(Settings::default()).unwrap();
    ramp.validate = |n| n.settings::<Settings>()?.validate();
    vec![
        ramp,
        definition(
            "fold.motion.copy_progress",
            "Copy Progress",
            "Instances",
            vec![],
            vec![("value", Kind::Scalar)],
            |_, _| {
                Ok(field(Field::new(Kind::Scalar, true, |s, _| {
                    Ok(Datum::Scalar(
                        s.index as f64 / s.count.saturating_sub(1).max(1) as f64,
                    ))
                })))
            },
        ),
    ]
}
