//! Exact authored key times; interpolation only converts the local interval ratio.
use crate::fields::Datum;
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum Interpolation {
    Hold,
    #[default]
    Linear,
    Ease,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Key {
    pub time: Time,
    pub value: Datum,
    pub interpolation: Interpolation,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Track {
    pub keys: Vec<Key>,
}
impl Track {
    pub fn validate(&self) -> Result<(), String> {
        if self.keys.is_empty() || self.keys.len() > 18000 {
            return Err("keyframe track requires 1..=18000 keys".into());
        }
        let kind = self.keys[0].value.kind();
        if !matches!(
            kind,
            crate::fields::Kind::Scalar | crate::fields::Kind::Vector | crate::fields::Kind::Color
        ) {
            return Err("unsupported keyframe type".into());
        }
        for k in &self.keys {
            k.value.validate()?;
            if k.value.kind() != kind {
                return Err("keyframe types must agree".into());
            }
        }
        if self.keys.windows(2).any(|w| w[0].time >= w[1].time) {
            return Err("keys must have unique increasing exact times".into());
        }
        Ok(())
    }
    pub fn sample(&self, time: Time) -> Result<Datum, String> {
        self.validate()?;
        let end = self.keys.partition_point(|k| k.time <= time);
        if end == 0 {
            return Ok(self.keys[0].value.clone());
        }
        if end == self.keys.len() {
            return Ok(self.keys[end - 1].value.clone());
        }
        let a = &self.keys[end - 1];
        let b = &self.keys[end];
        let elapsed = time.checked_sub(a.time).map_err(|e| e.to_string())?;
        let duration = b.time.checked_sub(a.time).map_err(|e| e.to_string())?;
        let ratio = (elapsed.numerator() as f64 / elapsed.denominator() as f64)
            / (duration.numerator() as f64 / duration.denominator() as f64);
        let weight = match a.interpolation {
            Interpolation::Hold => 0.,
            Interpolation::Linear => ratio,
            Interpolation::Ease => ratio * ratio * (3. - 2. * ratio),
        };
        a.value.mix(&b.value, weight)
    }
}
