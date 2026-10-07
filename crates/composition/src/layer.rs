//! Layer behavior shared by composition authoring and graph providers.
use fold_animation::Curve;
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];
pub type Channels = BTreeMap<String, Curve>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerState {
    pub id: ObjectId,
    pub name: String,
    #[serde(default)]
    pub parent: Option<ObjectId>,
    pub visible: bool,
    pub locked: bool,
    pub start: Time,
    pub end: Time,
    #[serde(default = "identity")]
    pub offset: [f64; 6],
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub animation: Channels,
}
fn identity() -> [f64; 6] {
    IDENTITY
}
fn one() -> f64 {
    1.
}
impl LayerState {
    pub fn new(name: impl Into<String>, end: Time) -> Self {
        Self {
            id: ObjectId::new(),
            name: name.into(),
            parent: None,
            visible: true,
            locked: false,
            start: Time::ZERO,
            end,
            offset: IDENTITY,
            opacity: 1.,
            animation: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.start >= self.end
            || !self.offset.iter().all(|v| v.is_finite())
            || !(0. ..=1.).contains(&self.opacity)
        {
            return Err("Invalid layer range, transform or opacity".into());
        }
        for curve in self.animation.values() {
            curve.validate()?;
        }
        Ok(())
    }
    pub fn sample(&self, path: &str, base: f64, time: Time) -> f64 {
        self.animation
            .get(path)
            .and_then(|c| c.sample(time))
            .unwrap_or(base)
    }
    pub fn matrix(&self, time: Time) -> [f64; 6] {
        std::array::from_fn(|i| self.sample(&format!("transform.{i}"), self.offset[i], time))
    }
    pub fn active(&self, time: Time) -> bool {
        self.visible && self.start <= time && time < self.end
    }
}
