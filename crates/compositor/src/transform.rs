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
    pub skew: [f64; 2],
    pub skew_order: SkewOrder,
    pub edges: fold_render::operations::Edges,
    pub clamp: bool,
    /// Transform-only shutter samples; upstream images remain at the requested time.
    pub motion_samples: u32,
    /// Exposure duration in exact fractions of a frame.
    pub shutter: fold_foundation::Time,
    pub shutter_offset: ShutterOffset,
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
            skew: [0.; 2],
            skew_order: Default::default(),
            edges: Default::default(),
            clamp: false,
            motion_samples: 0,
            shutter: fold_foundation::Time::new(1, 2).unwrap(),
            shutter_offset: Default::default(),
            invert: false,
            filter: Filter::Cubic,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkewOrder {
    #[default]
    XY,
    YX,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutterOffset {
    #[default]
    Start,
    Center,
    End,
}
impl Transform {
    pub fn sample_times(
        &self,
        time: fold_foundation::Time,
        rate: [u32; 2],
    ) -> Result<Vec<fold_foundation::Time>, String> {
        use fold_foundation::Time;
        self.matrix([1.; 2])?;
        if self.motion_samples == 0 || self.shutter == Time::ZERO {
            return Ok(vec![time]);
        }
        let exposure = self
            .shutter
            .checked_scale(i64::from(rate[1]), rate[0])
            .map_err(|e| e.to_string())?;
        let offset = match self.shutter_offset {
            ShutterOffset::Start => 0,
            ShutterOffset::Center => -1,
            ShutterOffset::End => -2,
        };
        (0..self.motion_samples)
            .map(|i| {
                exposure
                    .checked_scale(
                        i64::from(2 * i + 1) + i64::from(offset) * i64::from(self.motion_samples),
                        2 * self.motion_samples,
                    )
                    .and_then(|dt| time.checked_add(dt))
                    .map_err(|e| e.to_string())
            })
            .collect()
    }
    pub fn matrix(&self, preview_scale: [f64; 2]) -> Result<Affine, String> {
        if self
            .translate
            .iter()
            .chain(&self.scale)
            .chain(&self.pivot)
            .chain([&self.rotate])
            .chain(&self.skew)
            .any(|v| !v.is_finite() || v.abs() > 1e6)
            || self.scale.iter().any(|v| v.abs() < 1e-6)
            || self.skew.iter().any(|v| v.abs() >= 89.9)
            || self.motion_samples > 64
            || self.shutter < fold_foundation::Time::ZERO
            || self.shutter > fold_foundation::Time::new(2, 1).unwrap()
        {
            return Err("Transform requires finite values and nonzero scale".into());
        }
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let [kx, ky] = self.skew.map(|v| v.to_radians().tan());
        // Skew order names the first applied shear. Both preserve determinant.
        let (xx, yy) = match self.skew_order {
            SkewOrder::XY => (1., 1. + kx * ky),
            SkewOrder::YX => (1. + kx * ky, 1.),
        };
        let a = (cos * xx - sin * ky) * self.scale[0];
        let b = (sin * xx + cos * ky) * self.scale[0];
        let c = (cos * kx - sin * yy) * self.scale[1];
        let d = (sin * kx + cos * yy) * self.scale[1];
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

#[cfg(test)]
mod node_tests {
    use super::*;
    use fold_foundation::Time;
    #[test]
    fn shear_order_and_exact_shutter_times_are_authored() {
        let mut transform = Transform {
            skew: [20., -15.],
            pivot: [50., 40.],
            motion_samples: 4,
            shutter_offset: ShutterOffset::Center,
            ..Default::default()
        };
        let xy = transform.matrix([1.; 2]).unwrap();
        transform.skew_order = SkewOrder::YX;
        let yx = transform.matrix([1.; 2]).unwrap();
        assert!((xy.a - yx.a).abs() > 1e-3);
        for m in [xy, yx] {
            assert!((m.a * 50. + m.c * 40. + m.tx - 50.).abs() < 1e-10);
            assert!((m.b * 50. + m.d * 40. + m.ty - 40.).abs() < 1e-10);
            assert!((m.a * m.d - m.b * m.c - 1.).abs() < 1e-10);
        }
        let times = transform.sample_times(Time::ZERO, [24, 1]).unwrap();
        assert_eq!(times, [-3, -1, 1, 3].map(|n| Time::new(n, 384).unwrap()));
        transform.motion_samples = 0;
        assert_eq!(
            transform.sample_times(Time::ZERO, [24, 1]).unwrap(),
            [Time::ZERO]
        );
    }
}
