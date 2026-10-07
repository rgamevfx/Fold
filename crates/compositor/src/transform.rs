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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SizeReference {
    #[default]
    Placement,
    Width,
    Height,
    ShorterSide,
}
/// Image placement relative to the full destination, never preview dimensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelativeTransform {
    pub source_size: [f64; 2],
    pub anchor: [f64; 2],
    /// Percent of destination width/height, measured from the aligned anchor.
    pub position: [f64; 2],
    #[serde(default)]
    pub size_reference: SizeReference,
    /// Uniform percentage of the selected reference size.
    pub size: f64,
    pub fit: fold_platform::format::Fit,
    pub filter: Filter,
}
impl Default for RelativeTransform {
    fn default() -> Self {
        Self {
            source_size: [1920., 1080.],
            anchor: [0.5; 2],
            position: [0.; 2],
            size: 100.,
            size_reference: SizeReference::Placement,
            fit: Default::default(),
            filter: Default::default(),
        }
    }
}
impl RelativeTransform {
    pub fn matrix(
        &self,
        format: fold_platform::format::Format,
        raster: [u32; 2],
    ) -> Result<Affine, String> {
        format.validate()?;
        let destination = format.display_size();
        let (fit, scale) = match self.size_reference {
            SizeReference::Placement => (self.fit, 1.),
            SizeReference::Width => (
                fold_platform::format::Fit::Native,
                destination[0] / self.source_size[0],
            ),
            SizeReference::Height => (
                fold_platform::format::Fit::Native,
                destination[1] / self.source_size[1],
            ),
            SizeReference::ShorterSide => (
                fold_platform::format::Fit::Native,
                destination[0].min(destination[1]) / self.source_size[0].min(self.source_size[1]),
            ),
        };
        let mut m = fold_platform::format::placement(
            self.source_size,
            destination,
            fit,
            self.anchor,
            self.position.map(|v| v / 100.),
            self.size / 100. * scale,
        )?;
        // Image inputs occupy the common raster. Reinterpret their sample grid
        // in the declared source frame, then place into destination pixels.
        m.a *= self.source_size[0] / f64::from(raster[0]);
        m.b *= self.source_size[0] / f64::from(raster[0]);
        m.c *= self.source_size[1] / f64::from(raster[1]);
        m.d *= self.source_size[1] / f64::from(raster[1]);
        fold_platform::format::rasterize(m, destination, raster)
    }
}
