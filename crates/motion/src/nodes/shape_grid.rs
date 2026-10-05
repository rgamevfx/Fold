//! Geometry occupancy, independent of text or a particular animation.
use super::*;
use crate::{
    evaluation::content,
    fields::Budget,
    geometry::{Content, Geometry, IDENTITY, multiply, sampling},
};
use fold_render::vector::Segment;
use std::sync::Arc;
struct Grid {
    size: f64,
    scale: f64,
    circular: bool,
    world_aligned: bool,
}
pub fn definition_grid() -> Definition {
    let mut def = definition(
        "fold.motion.shape_grid",
        "Shape Grid",
        "Geometry",
        vec![
            Socket::required("content", Kind::Content),
            scalar("cell_size", 6., "px"),
            scalar("cell_scale", 1., "factor"),
            boolean("circular", false),
            boolean("world_aligned", false),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            let size = e.scalar(n, "cell_size")?;
            let scale = e.scalar(n, "cell_scale")?;
            if !size.is_finite() || size < 0. || !(0. ..=1.).contains(&scale) {
                return Err("Shape Grid requires nonnegative size and scale in 0..1".into());
            }
            if size <= 1. {
                return Ok(content(source));
            }
            Ok(content(grid(
                &source,
                &Grid {
                    size,
                    scale,
                    circular: e.uniform(n, "circular")?.boolean()?,
                    world_aligned: e.uniform(n, "world_aligned")?.boolean()?,
                },
                IDENTITY,
                &mut e.budget,
                0,
                &mut 0,
            )?))
        },
    );
    def.description = "Sample filled geometry into cells. Circular cells produce stipple; world_aligned applies upstream transforms before sampling on one shared grid. Default local square cells preserve existing projects.";
    def
}
fn grid(
    source: &Content,
    options: &Grid,
    parent: [f64; 6],
    budget: &mut Budget,
    depth: usize,
    cells: &mut usize,
) -> Result<Content, String> {
    if depth > 32 {
        return Err("Shape Grid nesting limit".into());
    }
    let mut result = Vec::new();
    let Grid { size, scale, .. } = *options;
    for item in source.iter() {
        budget.spend()?;
        let mut item = item.clone();
        let world = multiply(parent, item.transform);
        if options.world_aligned {
            item.transform = IDENTITY;
        }
        match &item.geometry {
            Geometry::Group(c) => {
                item.geometry = Geometry::Group(grid(c, options, world, budget, depth + 1, cells)?)
            }
            Geometry::Instance(c) => {
                item.geometry =
                    Geometry::Instance(grid(c, options, world, budget, depth + 1, cells)?)
            }
            Geometry::Path(path) | Geometry::Glyph { path, .. } => {
                let tolerance = if options.world_aligned {
                    (size * 0.025).max(0.05)
                        / world[0]
                            .hypot(world[1])
                            .max(world[2].hypot(world[3]))
                            .max(1.)
                } else {
                    (size * 0.025).max(0.05)
                };
                let mut contours = sampling::contours(path, tolerance, budget)?;
                if options.world_aligned {
                    for p in contours.iter_mut().flatten() {
                        *p = [
                            world[0] * p[0] + world[2] * p[1] + world[4],
                            world[1] * p[0] + world[3] * p[1] + world[5],
                        ];
                        if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                            return Err("Shape Grid transformed coordinates exceed bounds".into());
                        }
                    }
                }
                let mut min = [f64::INFINITY; 2];
                let mut max = [f64::NEG_INFINITY; 2];
                for p in contours.iter().flatten() {
                    for i in 0..2 {
                        min[i] = min[i].min(p[i]);
                        max[i] = max[i].max(p[i]);
                    }
                }
                let mut path = Vec::new();
                if min[0].is_finite() {
                    let start = [
                        (min[0] / size).floor() as i64,
                        (min[1] / size).floor() as i64,
                    ];
                    let end = [(max[0] / size).ceil() as i64, (max[1] / size).ceil() as i64];
                    if (end[0] - start[0]).saturating_mul(end[1] - start[1]) > 65536 {
                        return Err("Shape Grid exceeds 65536 candidate cells".into());
                    }
                    for y in start[1]..end[1] {
                        for x in start[0]..end[0] {
                            let center = [(x as f64 + 0.5) * size, (y as f64 + 0.5) * size];
                            if sampling::contains(&contours, center, item.even_odd, budget)? {
                                *cells += 1;
                                if *cells > 16384 {
                                    return Err("Shape Grid exceeds 16384 occupied cells".into());
                                }
                                let half = size * scale * 0.5;
                                let [x, y] = center;
                                if options.circular {
                                    let k = half * 0.5522847498307936;
                                    path.extend([
                                        Segment::Move([x, y - half]),
                                        Segment::Cubic(
                                            [x + k, y - half],
                                            [x + half, y - k],
                                            [x + half, y],
                                        ),
                                        Segment::Cubic(
                                            [x + half, y + k],
                                            [x + k, y + half],
                                            [x, y + half],
                                        ),
                                        Segment::Cubic(
                                            [x - k, y + half],
                                            [x - half, y + k],
                                            [x - half, y],
                                        ),
                                        Segment::Cubic(
                                            [x - half, y - k],
                                            [x - k, y - half],
                                            [x, y - half],
                                        ),
                                        Segment::Close,
                                    ]);
                                } else {
                                    path.extend([
                                        Segment::Move([x - half, y - half]),
                                        Segment::Line([x + half, y - half]),
                                        Segment::Line([x + half, y + half]),
                                        Segment::Line([x - half, y + half]),
                                        Segment::Close,
                                    ]);
                                }
                            }
                        }
                    }
                }
                item.geometry = match item.geometry {
                    Geometry::Glyph { cluster, .. } => Geometry::Glyph {
                        path: Arc::new(path),
                        cluster,
                    },
                    _ => Geometry::Path(Arc::new(path)),
                };
                item.stroke = None;
            }
            Geometry::Point => return Err("Shape Grid requires filled paths".into()),
        }
        result.push(item);
    }
    Ok(Arc::new(result))
}
