//! Logical scene units and framing. Resolution edits never rewrite geometry.
use crate::Motion;
use fold_platform::format::{Fit, Format};
use fold_render::Affine;
use serde::{Deserialize, Serialize};
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub enum Framing {
    #[default]
    Fit,
    Fill,
    Responsive,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Canvas {
    pub size: [f64; 2],
    pub framing: Framing,
}
impl Canvas {
    pub fn validate(self) -> Result<(), String> {
        if self
            .size
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 1e6)
        {
            return Err("Canvas dimensions must be positive finite scene units".into());
        }
        Ok(())
    }
    pub fn bounds(self, format: Format) -> Result<[f64; 2], String> {
        self.validate()?;
        format.validate()?;
        let [w, h] = format.display_size();
        Ok(if self.framing == Framing::Responsive {
            [self.size[1] * w / h, self.size[1]]
        } else {
            self.size
        })
    }
    pub fn matrix(self, format: Format, raster: [u32; 2]) -> Result<Affine, String> {
        let bounds = self.bounds(format)?;
        let destination = format.display_size();
        let fit = if self.framing == Framing::Fill {
            Fit::Fill
        } else {
            Fit::Fit
        };
        let m = fold_platform::format::placement(bounds, destination, fit, [0.5; 2], [0.; 2], 1.)?;
        fold_platform::format::rasterize(m, destination, raster)
    }
}
impl Motion {
    pub fn output_format(&self) -> Format {
        Format::square([self.info.width, self.info.height])
    }
    /// Existing documents without a declared canvas retain their old coordinates.
    /// The first format edit captures those coordinates before changing pixels.
    pub fn retain_canvas(&mut self) {
        if self.canvas.is_none() {
            self.canvas = Some(Canvas {
                size: [self.info.width as f64, self.info.height as f64],
                framing: Framing::Fit,
            });
        }
    }
    pub fn canvas_size(&self) -> [f64; 2] {
        self.canvas
            .and_then(|c| c.bounds(self.output_format()).ok())
            .unwrap_or([self.info.width as f64, self.info.height as f64])
    }
    pub fn canvas_matrix(&self, format: Format, raster: [u32; 2]) -> Result<Affine, String> {
        if let Some(canvas) = self.canvas {
            canvas.matrix(format, raster)
        } else {
            Ok(Affine {
                a: raster[0] as f64 / self.info.width as f64,
                d: raster[1] as f64 / self.info.height as f64,
                ..Affine::IDENTITY
            })
        }
    }
    /// Maps logical coordinates to a viewer rectangle, including framing offsets.
    pub fn viewport_mapping(&self, size: [f32; 2]) -> Result<([f64; 2], [f64; 2]), String> {
        let m = self.canvas_matrix(self.output_format(), [self.info.width, self.info.height])?;
        let sx = f64::from(size[0]) / f64::from(self.info.width);
        let sy = f64::from(size[1]) / f64::from(self.info.height);
        Ok(([m.a * sx, m.d * sy], [m.tx * sx, m.ty * sy]))
    }
}
