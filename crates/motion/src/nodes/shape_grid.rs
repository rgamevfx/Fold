//! Geometry occupancy, independent of text or a particular animation.
use super::*;
use crate::{
    evaluation::content,
    fields::Budget,
    geometry::{Content, Geometry, sampling},
};
use fold_render::vector::Segment;
use std::sync::Arc;
pub fn definition_grid() -> Definition {
    definition(
        "fold.motion.shape_grid",
        "Shape Grid",
        "Geometry",
        vec![
            Socket::required("content", Kind::Content),
            scalar("cell_size", 6., "px"),
            scalar("cell_scale", 1., "factor"),
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
                size,
                scale,
                &mut e.budget,
                0,
                &mut 0,
            )?))
        },
    )
}
fn grid(
    source: &Content,
    size: f64,
    scale: f64,
    budget: &mut Budget,
    depth: usize,
    cells: &mut usize,
) -> Result<Content, String> {
    if depth > 32 {
        return Err("Shape Grid nesting limit".into());
    }
    let mut result = Vec::new();
    for item in source.iter() {
        budget.spend()?;
        let mut item = item.clone();
        match &item.geometry {
            Geometry::Group(c) => {
                item.geometry = Geometry::Group(grid(c, size, scale, budget, depth + 1, cells)?)
            }
            Geometry::Instance(c) => {
                item.geometry = Geometry::Instance(grid(c, size, scale, budget, depth + 1, cells)?)
            }
            Geometry::Path(path) | Geometry::Glyph { path, .. } => {
                let contours = sampling::contours(path, (size * 0.025).max(0.05), budget)?;
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
