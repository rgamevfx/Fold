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
        let mut matrix =
            std::array::from_fn(|i| self.sample(&format!("transform.{i}"), self.offset[i], time));
        if self
            .animation
            .keys()
            .any(|k| k.starts_with("scene.scale.") || k == "scene.rotation.0")
        {
            let (scale, rotation, shear) = components(matrix);
            let scale = [
                self.sample("scene.scale.0", scale[0], time),
                self.sample("scene.scale.1", scale[1], time),
            ];
            let rotation = self.sample("scene.rotation.0", rotation, time);
            set_components(&mut matrix, scale, rotation, shear);
        }
        matrix
    }
    pub fn active(&self, time: Time) -> bool {
        self.visible && self.start <= time && time < self.end
    }
}

/// Decompose the linear part while retaining shear and reflected axes.
pub(crate) fn components(m: [f64; 6]) -> ([f64; 2], f64, f64) {
    let sx = m[0].hypot(m[1]);
    let angle = if sx > f64::EPSILON {
        m[1].atan2(m[0])
    } else {
        (-m[2]).atan2(m[3])
    };
    let (sin, cos) = angle.sin_cos();
    (
        [sx, -sin * m[2] + cos * m[3]],
        angle.to_degrees(),
        cos * m[2] + sin * m[3],
    )
}
pub(crate) fn set_components(m: &mut [f64; 6], scale: [f64; 2], rotation: f64, shear: f64) {
    let (sin, cos) = rotation.to_radians().sin_cos();
    m[0] = cos * scale[0];
    m[1] = sin * scale[0];
    m[2] = cos * shear - sin * scale[1];
    m[3] = sin * shear + cos * scale[1];
}

#[cfg(test)]
mod transform_tests {
    use super::*;
    #[test]
    fn scale_rotation_controls_preserve_shear_and_sample_animation() {
        let original = [1., 0.3, 0.2, -2., 10., 20.];
        let (scale, rotation, shear) = components(original);
        let mut matrix = original;
        set_components(&mut matrix, scale, rotation, shear);
        for (a, b) in matrix.into_iter().zip(original) {
            assert!((a - b).abs() < 1e-10);
        }
        let mut layer = LayerState::new("Text", Time::new(2, 1).unwrap());
        layer.animation.insert(
            "scene.rotation.0".into(),
            Curve {
                keys: vec![fold_animation::Key::new(Time::ZERO, 90.)],
            },
        );
        let m = layer.matrix(Time::ZERO);
        assert!(m[0].abs() < 1e-10 && (m[1] - 1.).abs() < 1e-10 && (m[2] + 1.).abs() < 1e-10);
    }
}
