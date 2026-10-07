//! Feature-neutral textured triangles. UV and sample coordinates are normalized,
//! top-left based. Sampling is bilinear, clamped, in premultiplied working color.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "gpu", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub sample: [f32; 2],
    /// Center for per-cell scale responses; position for vertex displacement.
    pub center: [f32; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Channel {
    #[default]
    Luminance,
    Red,
    Green,
    Blue,
    Alpha,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "gpu", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct Instance {
    pub matrix: [f32; 4],
    pub offset: [f32; 4],
}
impl Default for Instance {
    fn default() -> Self {
        Self {
            matrix: [1., 0., 0., 1.],
            offset: [0., 0., 1., 0.],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Draw {
    pub mesh: Arc<Mesh>,
    pub instances: Vec<Instance>,
    pub transform: [f32; 6],
    pub opacity: f32,
    pub channel: Channel,
    pub document_samples: bool,
    /// Sample remapping: value * gain + bias; does not clamp HDR values.
    pub remap: [f32; 2],
    pub displacement: [f32; 2],
    pub scale: f32,
    pub rotation: f32,
    pub color: f32,
    pub alpha: f32,
    /// Glyph atlas: count, normalized cell width and height. Zero disables it.
    pub atlas: [f32; 3],
}
impl Draw {
    pub fn new(mesh: Arc<Mesh>) -> Self {
        Self {
            mesh,
            instances: vec![Instance::default()],
            transform: [1., 0., 0., 1., 0., 0.],
            opacity: 1.,
            channel: Channel::Luminance,
            document_samples: false,
            remap: [1., 0.],
            displacement: [0.; 2],
            scale: 0.,
            rotation: 0.,
            color: 0.,
            alpha: 0.,
            atlas: [0.; 3],
        }
    }
    /// Draw state shared by an ordered instance batch (excluding instances).
    pub fn compatible(&self, other: &Self) -> bool {
        self.mesh == other.mesh
            && self.transform == other.transform
            && self.opacity == other.opacity
            && self.channel == other.channel
            && self.document_samples == other.document_samples
            && self.remap == other.remap
            && self.displacement == other.displacement
            && self.scale == other.scale
            && self.rotation == other.rotation
            && self.color == other.color
            && self.alpha == other.alpha
            && self.atlas == other.atlas
    }
    pub fn validate(&self) -> Result<(), String> {
        let m = &self.mesh;
        if self.instances.is_empty()
            || m.vertices.is_empty()
            || m.indices.is_empty()
            || self.instances.len().saturating_mul(m.vertices.len()) > 2_000_000
            || self.instances.len() > 16384
            || self.instances.iter().any(|i| {
                i.matrix
                    .iter()
                    .chain(&i.offset)
                    .any(|v| !v.is_finite() || v.abs() > 1e8)
                    || !(0. ..=1.).contains(&i.offset[2])
            })
            || m.vertices.len() > 65536
            || m.indices.len() > 393216
            || !m.indices.len().is_multiple_of(3)
            || m.indices.iter().any(|i| *i as usize >= m.vertices.len())
            || m.vertices.iter().any(|v| {
                v.position
                    .iter()
                    .chain(&v.uv)
                    .chain(&v.sample)
                    .chain(&v.center)
                    .any(|v| !v.is_finite() || v.abs() > 1e8)
            })
            || self
                .transform
                .iter()
                .chain(&self.remap)
                .chain(&self.displacement)
                .chain(&self.atlas)
                .any(|v| !v.is_finite() || v.abs() > 1e8)
            || [
                self.opacity,
                self.scale,
                self.rotation,
                self.color,
                self.alpha,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || !(0. ..=1.).contains(&self.opacity)
            || !(0. ..=1.).contains(&self.color)
            || !(0. ..=1.).contains(&self.alpha)
            || !(0. ..=64.).contains(&self.atlas[0])
        {
            return Err("invalid or oversized textured geometry".into());
        }
        Ok(())
    }
}
pub fn sample(pixels: &[[f32; 4]], width: u32, height: u32, uv: [f32; 2]) -> [f32; 4] {
    let x = uv[0].clamp(0., 1.) * width as f32 - 0.5;
    let y = uv[1].clamp(0., 1.) * height as f32 - 0.5;
    let ix = x.floor();
    let iy = y.floor();
    let at = |dx: f32, dy: f32| {
        pixels[((iy + dy).clamp(0., height as f32 - 1.) as u32 * width
            + (ix + dx).clamp(0., width as f32 - 1.) as u32) as usize]
    };
    let a = at(0., 0.);
    let b = at(1., 0.);
    let c = at(0., 1.);
    let d = at(1., 1.);
    std::array::from_fn(|i| {
        (a[i] * (1. - (x - ix)) + b[i] * (x - ix)) * (1. - (y - iy))
            + (c[i] * (1. - (x - ix)) + d[i] * (x - ix)) * (y - iy)
    })
}
pub fn scalar(c: [f32; 4], channel: Channel, aces: bool) -> f32 {
    if channel == Channel::Alpha {
        return c[3];
    }
    let rgb = if c[3] > 1e-8 {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
    } else {
        [0.; 3]
    };
    match channel {
        Channel::Luminance => {
            let w = if aces {
                [0.27222872, 0.67408174, 0.053689517]
            } else {
                [0.2126, 0.7152, 0.0722]
            };
            rgb.iter().zip(w).map(|(v, w)| v * w).sum()
        }
        Channel::Red => rgb[0],
        Channel::Green => rgb[1],
        Channel::Blue => rgb[2],
        Channel::Alpha => unreachable!(),
    }
}
/// Bounded reference rasterizer, same center coverage and interpolation as GPU.
pub(crate) fn rasterize(
    draw: &Draw,
    source: &crate::Frame,
    driver: &crate::Frame,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
    aces: bool,
) -> Result<Vec<[f32; 4]>, String> {
    rasterize_impl(draw, source, &[], driver, width, height, cancel, aces)
}
pub(crate) fn rasterize_mips(
    draw: &Draw,
    levels: &[crate::Frame],
    driver: &crate::Frame,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
    aces: bool,
) -> Result<Vec<[f32; 4]>, String> {
    rasterize_impl(
        draw, &levels[0], levels, driver, width, height, cancel, aces,
    )
}
fn rasterize_impl(
    draw: &Draw,
    source: &crate::Frame,
    levels: &[crate::Frame],
    driver: &crate::Frame,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
    aces: bool,
) -> Result<Vec<[f32; 4]>, String> {
    draw.validate()?;
    let mut vertices = Vec::with_capacity(draw.mesh.vertices.len());
    for instance in &draw.instances {
        for v in &draw.mesh.vertices {
            cancel.check()?;
            let uv = if draw.document_samples {
                let m = instance.matrix;
                let p = v.center;
                let p = [
                    m[0] * p[0] + m[2] * p[1] + instance.offset[0],
                    m[1] * p[0] + m[3] * p[1] + instance.offset[1],
                ];
                let m = draw.transform;
                [
                    (m[0] * p[0] + m[2] * p[1] + m[4]) / width as f32,
                    (m[1] * p[0] + m[3] * p[1] + m[5]) / height as f32,
                ]
            } else {
                v.sample
            };
            let c = sample(&driver.pixels, driver.width, driver.height, uv);
            let value = scalar(c, draw.channel, aces) * draw.remap[0] + draw.remap[1];
            let mut p = std::array::from_fn::<_, 2, _>(|i| {
                v.center[i]
                    + (v.position[i] - v.center[i]) * (1. + value * draw.scale)
                    + value * draw.displacement[i]
            });
            let angle = (value * draw.rotation).to_radians();
            let (sin, cos) = angle.sin_cos();
            let delta = [
                p[0] - v.center[0] - value * draw.displacement[0],
                p[1] - v.center[1] - value * draw.displacement[1],
            ];
            p = [
                v.center[0] + cos * delta[0] - sin * delta[1] + value * draw.displacement[0],
                v.center[1] + sin * delta[0] + cos * delta[1] + value * draw.displacement[1],
            ];
            let im = instance.matrix;
            p = [
                im[0] * p[0] + im[2] * p[1] + instance.offset[0],
                im[1] * p[0] + im[3] * p[1] + instance.offset[1],
            ];
            let m = draw.transform;
            p = [
                m[0] * p[0] + m[2] * p[1] + m[4],
                m[1] * p[0] + m[3] * p[1] + m[5],
            ];
            if p.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
                return Err("image response exceeds coordinate range".into());
            }
            let mut uv = v.uv;
            if draw.atlas[0] > 0. {
                let glyph = (value.clamp(0., 1.) * (draw.atlas[0] - 1.) + 0.5).floor();
                uv = [(glyph + uv[0]) * draw.atlas[1], uv[1] * draw.atlas[2]];
            }
            let tint = if c[3] > 1e-8 {
                [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
            } else {
                [0.; 3]
            };
            let factor = std::array::from_fn::<_, 4, _>(|i| {
                if i < 3 {
                    1. + (tint[i] - 1.) * draw.color
                } else {
                    (1. + (value.clamp(0., 1.) - 1.) * draw.alpha) * instance.offset[2]
                }
            });
            // A declared 1/16-pixel coverage grid is representable by every
            // Vulkan rasterizer; CPU and GPU agree at shared triangle edges.
            p = p.map(|v| (v * 16. + 0.5).floor() / 16.);
            vertices.push((p, uv, factor));
        }
    }
    let mut out = vec![[0.; 4]; (width * height) as usize];
    let mut work = 0u64;
    let edge = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let top = |a: [f32; 2], b: [f32; 2]| b[1] < a[1] || (b[1] == a[1] && b[0] > a[0]);
    for instance in 0..draw.instances.len() {
        let offset = instance * draw.mesh.vertices.len();
        for triangle in draw.mesh.indices.chunks_exact(3) {
            cancel.check()?;
            let mut t = [
                vertices[offset + triangle[0] as usize],
                vertices[offset + triangle[1] as usize],
                vertices[offset + triangle[2] as usize],
            ];
            let mut area = edge(t[0].0, t[1].0, t[2].0);
            if area == 0. {
                continue;
            }
            if area < 0. {
                t.swap(1, 2);
                area = -area;
            }
            let dx = [t[1].0[0] - t[0].0[0], t[2].0[0] - t[0].0[0]];
            let dy = [t[1].0[1] - t[0].0[1], t[2].0[1] - t[0].0[1]];
            let du = [t[1].1[0] - t[0].1[0], t[2].1[0] - t[0].1[0]];
            let dv = [t[1].1[1] - t[0].1[1], t[2].1[1] - t[0].1[1]];
            let gx = [
                (du[0] * dy[1] - du[1] * dy[0]) / area * source.width as f32,
                (dv[0] * dy[1] - dv[1] * dy[0]) / area * source.height as f32,
            ];
            let gy = [
                (du[1] * dx[0] - du[0] * dx[1]) / area * source.width as f32,
                (dv[1] * dx[0] - dv[0] * dx[1]) / area * source.height as f32,
            ];
            let lod = gx[0]
                .hypot(gx[1])
                .max(gy[0].hypot(gy[1]))
                .max(1.)
                .log2()
                .min(levels.len().saturating_sub(1) as f32);
            let lo = std::array::from_fn::<_, 2, _>(|i| {
                t.iter()
                    .map(|v| v.0[i])
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    .max(0.) as u32
            });
            let hi = std::array::from_fn::<_, 2, _>(|i| {
                t.iter()
                    .map(|v| v.0[i])
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    .clamp(0., [width, height][i] as f32) as u32
            });
            work += u64::from(hi[0].saturating_sub(lo[0])) * u64::from(hi[1].saturating_sub(lo[1]));
            if work > 256 * 1024 * 1024 {
                return Err("textured CPU raster work budget exceeded".into());
            }
            for y in lo[1]..hi[1] {
                cancel.check()?;
                for x in lo[0]..hi[0] {
                    let p = [x as f32 + 0.5, y as f32 + 0.5];
                    let edges = [
                        edge(t[1].0, t[2].0, p),
                        edge(t[2].0, t[0].0, p),
                        edge(t[0].0, t[1].0, p),
                    ];
                    if edges.iter().enumerate().any(|(i, &e)| {
                        e < 0. || (e == 0. && !top(t[(i + 1) % 3].0, t[(i + 2) % 3].0))
                    }) {
                        continue;
                    }
                    let w = edges.map(|v| v / area);
                    let uv = std::array::from_fn(|j| (0..3).map(|i| t[i].1[j] * w[i]).sum());
                    let f: [f32; 4] = std::array::from_fn(|j| {
                        t[0].2[j] + (t[1].2[j] - t[0].2[j]) * w[1] + (t[2].2[j] - t[0].2[j]) * w[2]
                    });
                    let mut c = if levels.is_empty() {
                        sample(&source.pixels, source.width, source.height, uv)
                    } else {
                        let lo = &levels[lod.floor() as usize];
                        let hi = &levels[lod.ceil() as usize];
                        let a = sample(&lo.pixels, lo.width, lo.height, uv);
                        let b = sample(&hi.pixels, hi.width, hi.height, uv);
                        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * lod.fract())
                    };
                    for i in 0..3 {
                        c[i] *= f[i];
                    }
                    for v in &mut c {
                        *v *= f[3] * draw.opacity;
                    }
                    // Convex alpha interpolation may overshoot by one float ULP.
                    c[3] = c[3].clamp(0., 1.);
                    let dst = &mut out[(y * width + x) as usize];
                    for i in 0..4 {
                        dst[i] = c[i] + dst[i] * (1. - c[3]);
                    }
                    dst[3] = dst[3].clamp(0., 1.);
                }
            }
        }
    }
    Ok(out)
}
