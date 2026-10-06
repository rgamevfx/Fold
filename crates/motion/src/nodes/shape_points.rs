//! Explicit conversion from filled geometry to flat, stably identified grid points.
use super::*;
use crate::{
    evaluation::{Value, output},
    geometry::{Element, Geometry, derived_id, sampling},
};
use std::sync::Arc;
pub fn definition_points() -> Definition {
    let mut d = definition(
        "fold.motion.shape_points",
        "Shape Points",
        "Distribution",
        vec![
            Socket::required("content", Kind::Content),
            scalar("spacing", 20., "px"),
            vector("origin", [0., 0.], "input coordinates"),
        ],
        vec![("points", Kind::Points)],
        |e, n| {
            let source = e.content(n, "content")?;
            let spacing = e.scalar(n, "spacing")?;
            let origin = e.vector(n, "origin")?;
            if !spacing.is_finite()
                || !(1. ..=1e6).contains(&spacing)
                || origin.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
            {
                return Err(
                    "Shape Points requires spacing in 1..1e6 and finite bounded origin".into(),
                );
            }
            let shapes = sampling::filled_shapes(&source, &mut e.budget)?;
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for shape in &shapes {
                for axis in 0..2 {
                    bounds[axis] = bounds[axis].min(shape.bounds[axis]);
                    bounds[axis + 2] = bounds[axis + 2].max(shape.bounds[axis + 2]);
                }
            }
            let mut points = vec![];
            if !shapes.is_empty() {
                let start = [
                    ((bounds[0] - origin[0]) / spacing).floor() as i64,
                    ((bounds[1] - origin[1]) / spacing).floor() as i64,
                ];
                let end = [
                    ((bounds[2] - origin[0]) / spacing).ceil() as i64,
                    ((bounds[3] - origin[1]) / spacing).ceil() as i64,
                ];
                if (end[0] - start[0]).saturating_mul(end[1] - start[1]) > 65536 {
                    return Err(
                        "Shape Points exceeds 65536 candidate cells; increase spacing".into(),
                    );
                }
                for y in start[1]..end[1] {
                    let py = origin[1] + (y as f64 + 0.5) * spacing;
                    let scans: Vec<_> = shapes
                        .iter()
                        .map(|shape| sampling::Scanline::new(&shape.contours, py, &mut e.budget))
                        .collect::<Result<_, _>>()?;
                    for x in start[0]..end[0] {
                        e.budget.spend()?;
                        let px = origin[0] + (x as f64 + 0.5) * spacing;
                        let mut inside = false;
                        for (scan, shape) in scans.iter().zip(&shapes) {
                            if scan.contains(px, shape.even_odd, &mut e.budget)? {
                                inside = true;
                                break;
                            }
                        }
                        if inside {
                            if points.len() >= 16384 {
                                return Err("Shape Points exceeds 16384 occupied cells".into());
                            }
                            let cell = ((x as u32 as u64) << 32) | y as u32 as u64;
                            let mut point =
                                Element::new(derived_id(n.seed(), cell), Geometry::Point);
                            point.transform[4] = px;
                            point.transform[5] = py;
                            points.push(point);
                        }
                    }
                }
            }
            Ok(output("points", Value::Points(Arc::new(points))))
        },
    );
    d.description = "Sample the union of filled paths/text on a regular grid and return flat Points for Instance on Points. Applies upstream transforms, respects path holes; spacing and origin are in input coordinates. Each cell identity depends on grid coordinates, so retained cells keep identity after shape edits. Shape/spacing edits can add/remove cells; ordering is row-major. Ignores stroke paint; image effects/masks are unsupported. Outline flattening tolerance is 0.25 input pixels. Limits: 65536 candidate cells, 16384 occupied points.";
    d
}
