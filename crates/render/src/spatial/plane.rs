//! Explicit top-left/Y-down canvas to a camera-attached world plane.
//! Mapping prepares spatial geometry; ordinary scene depth tests and clipping apply.
use super::{Camera, Projection};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// The logical canvas spans this camera's viewport at the declared distance.
    /// Each axis uses the full raster extent; crops must retain that full extent.
    Viewport,
    /// Fixed physical size per canvas unit. The logical canvas center lies on the
    /// camera's view axis; projected size may change with camera distance.
    MetersPerUnit(f64),
}

#[derive(Clone, Debug)]
pub struct Mapping {
    origin: [f64; 3],
    right: [f64; 3],
    down: [f64; 3],
    normal: [f32; 3],
}
impl Mapping {
    /// Map logical canvas coordinates to renderer-owned world-meter positions.
    pub fn point(&self, point: [f64; 2]) -> Result<[f32; 3], String> {
        let result: [f64; 3] = std::array::from_fn(|i| {
            self.origin[i] + self.right[i] * point[0] + self.down[i] * point[1]
        });
        if result.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
            return Err("Camera-plane point exceeds spatial coordinate bounds".into());
        }
        Ok(result.map(|v| v as f32))
    }
    /// Faces the attached camera. Canvas paths with Y-down winding must reverse
    /// their triangle indices when mapped to this front-facing spatial plane.
    pub fn normal(&self) -> [f32; 3] {
        self.normal
    }

    /// Conservative pixels-per-canvas-unit bound over an artwork rectangle.
    /// Perspective bounds clamp to the view's near plane, where clipping starts.
    pub fn projected_scale(
        &self,
        view: &Camera,
        raster: [u32; 2],
        bounds: [[f64; 2]; 2],
    ) -> Result<f64, String> {
        view.validate()?;
        if raster.contains(&0)
            || bounds.iter().flatten().any(|v| !v.is_finite())
            || (0..2).any(|i| bounds[0][i] > bounds[1][i])
        {
            return Err("Invalid projected plane extent".into());
        }
        let forward = normalize(std::array::from_fn(|i| {
            f64::from(view.target[i]) - f64::from(view.position[i])
        }));
        let right = normalize(cross(forward, view.up.map(f64::from)));
        let up = cross(right, forward);
        let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
        let axes = [right, up, forward];
        let derivatives = axes.map(|axis| dot(axis, self.right).abs() + dot(axis, self.down).abs());
        let mut depth = f64::INFINITY;
        let mut lateral = [0f64; 2];
        for x in [bounds[0][0], bounds[1][0]] {
            for y in [bounds[0][1], bounds[1][1]] {
                let point = std::array::from_fn(|i| {
                    self.origin[i] + self.right[i] * x + self.down[i] * y
                        - f64::from(view.position[i])
                });
                depth = depth.min(dot(point, forward));
                for i in 0..2 {
                    lateral[i] = lateral[i].max(dot(point, axes[i]).abs());
                }
            }
        }
        let scale = match view.projection {
            Projection::Orthographic { vertical_size } => {
                f64::from(raster[1]) / f64::from(vertical_size) * derivatives[0].max(derivatives[1])
            }
            Projection::Perspective { vertical_fov } => {
                let depth = depth.max(f64::from(view.near));
                let focal = f64::from(raster[1]) / (2. * (f64::from(vertical_fov) / 2.).tan());
                (0..2)
                    .map(|i| {
                        focal
                            * (derivatives[i] / depth
                                + lateral[i] * derivatives[2] / (depth * depth))
                    })
                    .fold(0., f64::max)
            }
        };
        if !scale.is_finite() {
            return Err("Projected plane scale exceeds numeric bounds".into());
        }
        Ok(scale.max(1e-9))
    }
}

impl Camera {
    /// Resolve a plane from this exact camera snapshot. Distance is positive
    /// camera-axis meters, not Euclidean distance from each plane point. It may
    /// lie outside the clipping interval; the renderer then clips it normally.
    pub fn plane(
        &self,
        distance: f64,
        canvas: [u32; 2],
        raster: [u32; 2],
        sizing: Sizing,
    ) -> Result<Mapping, String> {
        self.validate()?;
        if !distance.is_finite()
            || !(1e-6..=1e6).contains(&distance)
            || canvas.contains(&0)
            || raster.iter().any(|v| *v == 0 || *v > 4096)
        {
            return Err("Invalid camera-plane distance or extent".into());
        }
        let position = self.position.map(f64::from);
        let forward = normalize(std::array::from_fn(|i| {
            f64::from(self.target[i]) - position[i]
        }));
        let right = normalize(cross(forward, self.up.map(f64::from)));
        let up = cross(right, forward);
        let scale = match sizing {
            Sizing::Viewport => {
                let height = match self.projection {
                    Projection::Perspective { vertical_fov } => {
                        2. * distance * (f64::from(vertical_fov) / 2.).tan()
                    }
                    Projection::Orthographic { vertical_size } => f64::from(vertical_size),
                };
                [
                    height * f64::from(raster[0]) / f64::from(raster[1]) / f64::from(canvas[0]),
                    height / f64::from(canvas[1]),
                ]
            }
            Sizing::MetersPerUnit(scale) => {
                if !scale.is_finite() || !(1e-9..=1e6).contains(&scale) {
                    return Err("Invalid camera-plane meters per canvas unit".into());
                }
                [scale; 2]
            }
        };
        let right = right.map(|v| v * scale[0]);
        let down = up.map(|v| -v * scale[1]);
        Ok(Mapping {
            origin: std::array::from_fn(|i| {
                position[i] + forward[i] * distance
                    - right[i] * f64::from(canvas[0]) / 2.
                    - down[i] * f64::from(canvas[1]) / 2.
            }),
            right,
            down,
            normal: forward.map(|v| -v as f32),
        })
    }
}
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let length = v.iter().map(|v| v * v).sum::<f64>().sqrt();
    v.map(|v| v / length)
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
