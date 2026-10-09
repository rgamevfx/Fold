//! Camera-attached planar artwork prepared through the shared vector tessellator.
use super::{Error, camera_input, copied_resource, evaluation, render, screen};
use evaluation::placement::{CameraPlane, PlaneSizing};
use fold_render::vector::{self, Drawing};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Preparation {
    cache: render::planar::Cache,
    vertices: usize,
    indices: usize,
}
impl Preparation {
    pub(super) fn append(
        &mut self,
        entry: &evaluation::SceneEntry,
        plane: &CameraPlane,
        extents: [[u32; 2]; 2],
        scene: &mut render::Scene,
        step: &mut dyn FnMut() -> Result<(), Error>,
    ) -> Result<(), Error> {
        step()?;
        let [canvas, dimensions] = extents;
        let camera = camera_input(&plane.camera);
        let sizing = match plane.sizing {
            PlaneSizing::Viewport => render::plane::Sizing::Viewport,
            PlaneSizing::Physical { meters_per_unit } => {
                render::plane::Sizing::MetersPerUnit(meters_per_unit)
            }
        };
        let mapping = camera
            .plane(plane.distance, canvas, dimensions, sizing)
            .map_err(Error::Invalid)?;
        let placement = &entry.placement;
        let appearance = &placement.geometry.appearance;
        let paint = |mut color: [f64; 4]| {
            color[3] *= placement.opacity;
            color
        };
        let fill = appearance.fill.as_ref().map(|f| paint(f.color.value));
        let stroke = appearance.stroke.as_ref().map(|s| vector::Stroke {
            color: paint(s.color.value),
            width: s.width,
            cap: s.mode.cap(),
            join: s.mode.join(),
        });
        let paths = screen::geometry_paths(&placement.geometry.data, step)?;
        for (index, (path, transform)) in paths.into_iter().enumerate() {
            step()?;
            let [a, b, c, d, tx, ty] = transform;
            let mut bounds = [[f64::INFINITY; 2], [f64::NEG_INFINITY; 2]];
            for segment in path.iter() {
                step()?;
                let points: &[[f64; 2]] = match segment {
                    vector::Segment::Move(p) | vector::Segment::Line(p) => std::slice::from_ref(p),
                    vector::Segment::Quad(p, q) => &[*p, *q],
                    vector::Segment::Cubic(p, q, r) => &[*p, *q, *r],
                    vector::Segment::Close => &[],
                };
                for p in points {
                    let point = [
                        a * p[0] + c * p[1] + tx + plane.position[0],
                        b * p[0] + d * p[1] + ty + plane.position[1],
                    ];
                    for i in 0..2 {
                        bounds[0][i] = bounds[0][i].min(point[i]);
                        bounds[1][i] = bounds[1][i].max(point[i]);
                    }
                }
            }
            if !bounds[0][0].is_finite() {
                continue;
            }
            // Bound the miter extension conservatively in transformed canvas space.
            let padding = stroke.as_ref().map_or(0., |s| 10. * s.width)
                * (a.abs() + c.abs()).max(b.abs() + d.abs());
            for coordinate in &mut bounds[0] {
                *coordinate -= padding;
            }
            for coordinate in &mut bounds[1] {
                *coordinate += padding;
            }
            let pixel_scale = mapping
                .projected_scale(&scene.camera, dimensions, bounds)
                .map_err(Error::Invalid)?;
            let drawing = Drawing {
                path,
                transform,
                fill,
                stroke: stroke.clone(),
                even_odd: appearance.fill.as_ref().is_some_and(|f| f.even_odd),
            };
            let scale = pixel_scale * (a.abs() + c.abs()).max(b.abs() + d.abs());
            let mut stopped = None;
            let mut checkpoint = || {
                step().map_err(|error| {
                    let kind = if matches!(error, Error::Cancelled) {
                        render::planar::Error::Cancelled
                    } else {
                        render::planar::Error::Budget
                    };
                    stopped = Some(error);
                    kind
                })
            };
            let prepared = self
                .cache
                .prepare_with(&drawing, scale.max(1e-9), &mut checkpoint)
                .and_then(|prepared| self.cache.disjoint_with(prepared, &mut checkpoint))
                .map_err(|error| {
                    stopped.unwrap_or_else(|| match error {
                        render::planar::Error::Budget => Error::Budget,
                        render::planar::Error::Cancelled => Error::Cancelled,
                        other => Error::Invalid(format!("Camera Plane preparation: {other:?}")),
                    })
                })?;
            let overlap = fill.zip(stroke.as_ref()).map(|(fill, stroke)| {
                let foreground = stroke.color;
                let alpha = foreground[3] + fill[3] * (1. - foreground[3]);
                let mut color = [0.; 4];
                color[3] = alpha;
                if alpha > 0. {
                    for i in 0..3 {
                        color[i] = (foreground[i] * foreground[3]
                            + fill[i] * fill[3] * (1. - foreground[3]))
                            / alpha;
                    }
                }
                color
            });
            for (part, (mesh, color)) in [
                (prepared.fill, fill),
                (prepared.stroke, stroke.as_ref().map(|s| s.color)),
                (prepared.overlap, overlap),
            ]
            .into_iter()
            .enumerate()
            {
                let (Some(mesh), Some(color)) = (mesh, color) else {
                    continue;
                };
                step()?;
                self.vertices = self.vertices.saturating_add(mesh.positions.len());
                self.indices = self.indices.saturating_add(mesh.indices.len());
                if self.vertices > 1_000_000
                    || self.indices > 3_000_000
                    || scene.meshes.len() >= 1024
                    || scene.materials.len() >= 1024
                    || scene.instances.len() >= 4096
                {
                    return Err(Error::Budget);
                }
                let mut positions = Vec::with_capacity(mesh.positions.len());
                for point in &mesh.positions {
                    step()?;
                    let x = f64::from(point[0]);
                    let y = f64::from(point[1]);
                    positions.push(
                        mapping
                            .point([
                                a * x + c * y + tx + plane.position[0],
                                b * x + d * y + ty + plane.position[1],
                            ])
                            .map_err(Error::Invalid)?,
                    );
                }
                let mut indices = Vec::with_capacity(mesh.indices.len());
                for triangle in mesh.indices.chunks_exact(3) {
                    step()?;
                    // Select facing after the f32 boundary: tiny glyph triangles
                    // can collapse or reverse when translated into world coordinates.
                    let [p, q, r] =
                        [0, 1, 2].map(|i| positions[triangle[i] as usize].map(f64::from));
                    let u: [f64; 3] = std::array::from_fn(|i| q[i] - p[i]);
                    let v: [f64; 3] = std::array::from_fn(|i| r[i] - p[i]);
                    let cross = [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ];
                    let facing: f64 = (0..3)
                        .map(|i| cross[i] * f64::from(mapping.normal()[i]))
                        .sum();
                    if facing == 0. {
                        continue;
                    }
                    indices.extend(if facing < 0. {
                        [triangle[0], triangle[2], triangle[1]]
                    } else {
                        [triangle[0], triangle[1], triangle[2]]
                    });
                }
                let id = copied_resource(
                    b"fold.motion.camera_plane.part.v1",
                    entry.id,
                    index as u64,
                    part as u64,
                );
                scene.meshes.insert(
                    id,
                    Arc::new(render::Mesh {
                        normals: vec![mapping.normal(); positions.len()],
                        positions,
                        indices,
                    }),
                );
                scene.materials.insert(
                    id,
                    render::Material {
                        color: color.map(|v| v as f32),
                        unlit: true,
                        roughness: 1.,
                        reflectance: 0.,
                    },
                );
                scene.instances.push(render::Instance {
                    id,
                    mesh: id,
                    material: id,
                    transform: Default::default(),
                });
            }
        }
        step()
    }
}
