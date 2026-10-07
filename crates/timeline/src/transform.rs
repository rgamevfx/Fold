//! Clip-local spatial processing, after effects and before compositing the track.
use serde::{Deserialize, Serialize};
pub const KEY: &str = "fold.timeline.transform";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Transform {
    pub position: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.; 2],
            scale: [1.; 2],
            rotation: 0.,
            extensions: Default::default(),
        }
    }
}
impl Transform {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .position
            .iter()
            .chain(&self.scale)
            .chain(std::iter::once(&self.rotation))
            .any(|v| !v.is_finite())
        {
            return Err("Transform values must be finite".into());
        }
        Ok(())
    }
    pub fn matrix(&self, dimensions: [u32; 2], raster: [u32; 2]) -> fold_render::Affine {
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let [sx, sy] = self.scale;
        let a = cos * sx;
        let b = sin * sx;
        let c = -sin * sy;
        let d = cos * sy;
        let x = f64::from(dimensions[0]) / 2.;
        let y = f64::from(dimensions[1]) / 2.;
        let rx = f64::from(raster[0]) / f64::from(dimensions[0]);
        let ry = f64::from(raster[1]) / f64::from(dimensions[1]);
        fold_render::Affine {
            a,
            b: b * ry / rx,
            c: c * rx / ry,
            d,
            tx: (x - a * x - c * y + self.position[0]) * rx,
            ty: (y - b * x - d * y + self.position[1]) * ry,
        }
    }
}
impl crate::Clip {
    pub fn transform(&self) -> Result<Transform, String> {
        let value = self
            .extensions
            .get(KEY)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| format!("Invalid clip transform: {e}"))?
            .unwrap_or_default();
        Transform::validate(&value)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_center_and_translation_are_independent_of_preview_resolution() {
        let t = Transform {
            position: [100., 40.],
            scale: [0.5, 2.],
            rotation: 90.,
            ..Default::default()
        };
        for raster in [[1920, 1080], [960, 540]] {
            let m = t.matrix([1920, 1080], raster);
            let (x, y) = (f64::from(raster[0]) / 2., f64::from(raster[1]) / 2.);
            assert!(
                (m.a * x + m.c * y + m.tx - (x + 100. * f64::from(raster[0]) / 1920.)).abs() < 1e-8
            );
            assert!(
                (m.b * x + m.d * y + m.ty - (y + 40. * f64::from(raster[1]) / 1080.)).abs() < 1e-8
            );
        }
    }
}
