//! Authored 2D transform controls; render math remains feature-neutral.
use fold_render::{Affine, operations::Filter};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub translate: [f64; 2],
    pub scale: [f64; 2],
    pub pivot: [f64; 2],
    pub rotate: f64,
    pub skew: f64,
    pub invert: bool,
    pub filter: Filter,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            translate: [0.; 2],
            scale: [1.; 2],
            pivot: [0.; 2],
            rotate: 0.,
            skew: 0.,
            invert: false,
            filter: Filter::default(),
        }
    }
}
impl Transform {
    pub fn matrix(&self, preview_scale: [f64; 2]) -> Result<Affine, String> {
        if self
            .translate
            .iter()
            .chain(&self.scale)
            .chain(&self.pivot)
            .chain([&self.rotate, &self.skew])
            .any(|v| !v.is_finite() || v.abs() > 1e6)
            || self.scale.iter().any(|v| v.abs() < 1e-6)
        {
            return Err("Transform requires finite values and nonzero scale".into());
        }
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let a = cos * self.scale[0];
        let b = sin * self.scale[0];
        let c = (cos * self.skew - sin) * self.scale[1];
        let d = (sin * self.skew + cos) * self.scale[1];
        let mut m = Affine {
            a,
            b,
            c,
            d,
            tx: self.pivot[0] + self.translate[0] - a * self.pivot[0] - c * self.pivot[1],
            ty: self.pivot[1] + self.translate[1] - b * self.pivot[0] - d * self.pivot[1],
        };
        if self.invert {
            m = m.inverse()?;
        }
        m.b *= preview_scale[1] / preview_scale[0];
        m.c *= preview_scale[0] / preview_scale[1];
        m.tx *= preview_scale[0];
        m.ty *= preview_scale[1];
        m.inverse()?;
        Ok(m)
    }
}
