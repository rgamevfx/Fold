//! Full output format and placement math, independent of preview raster size.
use fold_render::Affine;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Format {
    pub dimensions: [u32; 2],
    /// Physical pixel width / height as a positive rational.
    pub pixel_aspect: [u32; 2],
}
impl Format {
    pub fn square(dimensions: [u32; 2]) -> Self {
        Self {
            dimensions,
            pixel_aspect: [1, 1],
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if self.dimensions.contains(&0) || self.pixel_aspect.contains(&0) {
            return Err("Format dimensions and pixel aspect must be positive".into());
        }
        Ok(())
    }
    pub fn display_size(self) -> [f64; 2] {
        [
            f64::from(self.dimensions[0]) * f64::from(self.pixel_aspect[0])
                / f64::from(self.pixel_aspect[1]),
            f64::from(self.dimensions[1]),
        ]
    }
    pub fn covering_radius(self, center: [f64; 2]) -> Result<f64, String> {
        self.validate()?;
        if center.iter().any(|v| !v.is_finite()) {
            return Err("Center must be finite".into());
        }
        let [w, h] = self.display_size();
        Ok(center[0]
            .abs()
            .max((w - center[0]).abs())
            .hypot(center[1].abs().max((h - center[1]).abs())))
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum Fit {
    #[default]
    Fit,
    Fill,
    Stretch,
    Native,
}

/// Anchor values are fractions of the destination and source extents.
/// The same anchor aligns both rectangles; an offset is destination-relative.
pub fn placement(
    source: [f64; 2],
    destination: [f64; 2],
    fit: Fit,
    anchor: [f64; 2],
    offset: [f64; 2],
    scale: f64,
) -> Result<Affine, String> {
    if source
        .iter()
        .chain(&destination)
        .any(|v| !v.is_finite() || *v <= 0.)
        || anchor
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        || offset.iter().any(|v| !v.is_finite())
        || !scale.is_finite()
        || scale <= 0.
    {
        return Err(
            "Placement requires positive sizes/scale, finite offsets and anchors in 0–1".into(),
        );
    }
    let ratios = [destination[0] / source[0], destination[1] / source[1]];
    let size = match fit {
        Fit::Fit => [ratios[0].min(ratios[1]); 2],
        Fit::Fill => [ratios[0].max(ratios[1]); 2],
        Fit::Stretch => ratios,
        Fit::Native => [1.; 2],
    }
    .map(|v| v * scale);
    let result = Affine {
        a: size[0],
        d: size[1],
        b: 0.,
        c: 0.,
        tx: (destination[0] - source[0] * size[0]) * anchor[0] + destination[0] * offset[0],
        ty: (destination[1] - source[1] * size[1]) * anchor[1] + destination[1] * offset[1],
    };
    result.inverse()?;
    Ok(result)
}

/// Convert a transform in full destination pixels to preview raster pixels.
/// Layout is already resolved; changing preview resolution cannot reflow it.
pub fn rasterize(
    mut matrix: Affine,
    logical: [f64; 2],
    raster: [u32; 2],
) -> Result<Affine, String> {
    if logical.iter().any(|v| !v.is_finite() || *v <= 0.) || raster.contains(&0) {
        return Err("Invalid logical/raster dimensions".into());
    }
    let [sx, sy] = [
        f64::from(raster[0]) / logical[0],
        f64::from(raster[1]) / logical[1],
    ];
    matrix.a *= sx;
    matrix.c *= sx;
    matrix.tx *= sx;
    matrix.b *= sy;
    matrix.d *= sy;
    matrix.ty *= sy;
    matrix.inverse()?;
    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_fill_and_preview_preserve_circles_and_anchors() {
        let fit = placement(
            [1920., 1080.],
            [1080., 1920.],
            Fit::Fit,
            [0.5; 2],
            [0.; 2],
            1.,
        )
        .unwrap();
        assert_eq!(fit.a, fit.d);
        assert_eq!(fit.tx, 0.);
        assert_eq!(fit.ty, (1920. - 1080. * fit.d) / 2.);
        let fill = placement(
            [1920., 1080.],
            [1080., 1920.],
            Fit::Fill,
            [0.5; 2],
            [0.; 2],
            1.,
        )
        .unwrap();
        assert_eq!(fill.a, fill.d);
        assert!(fill.tx < 0.);
        assert_eq!(fill.ty, 0.);
        let half = rasterize(fit, [1080., 1920.], [540, 960]).unwrap();
        assert_eq!(half.ty, fit.ty / 2.);
        assert_eq!(half.a, fit.a / 2.);
    }
    #[test]
    fn off_center_wipe_covers_every_corner_in_display_units() {
        for dimensions in [[1080, 1920], [3840, 2160], [1024, 1024]] {
            let f = Format::square(dimensions);
            let center = [
                f64::from(dimensions[0]) * 0.2,
                f64::from(dimensions[1]) * 0.7,
            ];
            let r = f.covering_radius(center).unwrap();
            for x in [0., f64::from(dimensions[0])] {
                for y in [0., f64::from(dimensions[1])] {
                    assert!((x - center[0]).hypot(y - center[1]) <= r);
                }
            }
        }
    }
}
