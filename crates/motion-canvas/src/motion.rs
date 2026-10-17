//! Prepared code motion is separate from authored placement and ordinary keys.
//! Sampling is explicit and bounded; all consumers evaluate the retained curves.
use crate::model::{Canvas, Object};
use fold_animation::Curve;
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub source: String,
    pub controls: BTreeMap<String, f64>,
    /// Samples per second; intermediate evaluations use linear interpolation.
    pub sample_rate: u32,
}
impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        if self.source.len() > 1_000_000
            || self.source.trim().is_empty()
            || !(1..=480).contains(&self.sample_rate)
            || self
                .controls
                .iter()
                .any(|(name, value)| name.trim().is_empty() || !value.is_finite())
        {
            return Err("Invalid motion source, controls or sample rate (1–480 Hz)".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub spec: Spec,
    pub enabled: bool,
    /// X/Y offsets, rotation offset, X/Y scale multipliers, opacity multiplier.
    pub curves: [Curve; 6],
}
impl Motion {
    pub fn pose(&self, time: Time) -> Pose {
        if !self.enabled {
            return Pose::default();
        }
        let defaults = [0., 0., 0., 1., 1., 1.];
        let values =
            std::array::from_fn::<_, 6, _>(|i| self.curves[i].sample(time).unwrap_or(defaults[i]));
        Pose {
            position: [values[0], values[1]],
            rotation: values[2],
            scale: [values[3], values[4]],
            opacity: values[5],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.spec.validate()?;
        for curve in &self.curves {
            curve.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: [f64; 2],
    pub rotation: f64,
    pub scale: [f64; 2],
    pub opacity: f64,
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            position: [0.; 2],
            rotation: 0.,
            scale: [1.; 2],
            opacity: 1.,
        }
    }
}
impl Pose {
    pub fn values(self) -> Result<[f64; 6], String> {
        let v = [
            self.position[0],
            self.position[1],
            self.rotation,
            self.scale[0],
            self.scale[1],
            self.opacity,
        ];
        if v.iter().any(|v| !v.is_finite()) {
            return Err("Motion code produced a nonfinite value".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub canvas: &'a Canvas,
    pub object: &'a Object,
    pub time: Time,
    pub controls: &'a BTreeMap<String, f64>,
}
impl Context<'_> {
    pub fn seconds(self) -> f64 {
        fold_animation::seconds(self.time)
    }
    pub fn number(self, name: &str) -> Result<f64, String> {
        self.controls
            .get(name)
            .copied()
            .ok_or_else(|| format!("Missing motion control {name:?}"))
    }
}
