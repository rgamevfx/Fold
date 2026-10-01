//! Distributions generate point identity separately from current ordering.
use super::*;
use crate::{
    evaluation::{Value, content, output},
    geometry::{Content, Element, Geometry, derived_id, multiply, transform},
};
use std::sync::Arc;
fn points(n: &Node, positions: impl Iterator<Item = ([f64; 2], f64)>) -> Outputs {
    output(
        "points",
        Value::Points(Arc::new(
            positions
                .enumerate()
                .map(|(i, (p, r))| {
                    let mut e = Element::new(derived_id(n.seed(), i as u64), Geometry::Point);
                    e.transform = transform(p, r, [1., 1.], [0., 0.]);
                    e
                })
                .collect(),
        )),
    )
}
pub fn definitions() -> Vec<Definition> {
    vec![
        definition(
            "fold.motion.line_points",
            "Line Points",
            "Distribution",
            vec![
                scalar("count", 5., "integer"),
                vector("start", [0., 0.], "px"),
                vector("end", [400., 0.], "px"),
            ],
            vec![("points", Kind::Points)],
            |e, n| {
                let count = bounded_count(e.scalar(n, "count")?, 16384)?;
                let a = e.vector(n, "start")?;
                let b = e.vector(n, "end")?;
                Ok(points(
                    n,
                    (0..count).map(|i| {
                        let t = i as f64 / count.saturating_sub(1).max(1) as f64;
                        ([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], 0.)
                    }),
                ))
            },
        ),
        definition(
            "fold.motion.grid_points",
            "Grid Points",
            "Distribution",
            vec![
                scalar("columns", 4., "integer"),
                scalar("rows", 4., "integer"),
                vector("spacing", [100., 100.], "px"),
                vector("origin", [0., 0.], "px"),
            ],
            vec![("points", Kind::Points)],
            |e, n| {
                let cols = bounded_count(e.scalar(n, "columns")?, 16384)?;
                let rows = bounded_count(e.scalar(n, "rows")?, 16384)?;
                let count = cols
                    .checked_mul(rows)
                    .filter(|n| *n <= 16384)
                    .ok_or("grid exceeds 16384 points")?;
                let step = e.vector(n, "spacing")?;
                let origin = e.vector(n, "origin")?;
                let content = points(
                    n,
                    (0..count).map(|i| {
                        (
                            [
                                origin[0] + (i % cols) as f64 * step[0],
                                origin[1] + (i / cols) as f64 * step[1],
                            ],
                            0.,
                        )
                    }),
                );
                Ok(content)
            },
        ),
        definition(
            "fold.motion.radial_points",
            "Radial Points",
            "Distribution",
            vec![
                scalar("count", 12., "integer"),
                vector("center", [0., 0.], "px"),
                scalar("radius", 200., "px"),
                scalar("start", 0., "degrees"),
                scalar("sweep", 360., "degrees"),
                boolean("orient", true),
            ],
            vec![("points", Kind::Points)],
            |e, n| {
                let count = bounded_count(e.scalar(n, "count")?, 16384)?;
                let center = e.vector(n, "center")?;
                let radius = e.scalar(n, "radius")?;
                if radius < 0. {
                    return Err("radius must be nonnegative".into());
                }
                let start = e.scalar(n, "start")?;
                let sweep = e.scalar(n, "sweep")?;
                let orient = e.uniform(n, "orient")?.boolean()?;
                let closed = sweep.abs() == 360.;
                let denominator = if closed {
                    count.max(1)
                } else {
                    count.saturating_sub(1).max(1)
                };
                Ok(points(
                    n,
                    (0..count).map(|i| {
                        let angle = start + sweep * i as f64 / denominator as f64;
                        let (s, c) = angle.to_radians().sin_cos();
                        (
                            [center[0] + radius * c, center[1] + radius * s],
                            if orient { angle } else { 0. },
                        )
                    }),
                ))
            },
        ),
        definition(
            "fold.motion.instance_on_points",
            "Instance on Points",
            "Instances",
            vec![
                Socket::required("source", Kind::Content),
                Socket::required("points", Kind::Points),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "source")?;
                let points = e.content(n, "points")?;
                Ok(content(Arc::new(
                    points
                        .iter()
                        .map(|p| {
                            let mut item = Element::new(
                                derived_id(n.seed(), p.id),
                                Geometry::Instance(source.clone()),
                            );
                            item.transform = p.transform;
                            item
                        })
                        .collect(),
                )))
            },
        ),
        definition(
            "fold.motion.realize_instances",
            "Realize Instances",
            "Instances",
            vec![Socket::required("content", Kind::Content)],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                let mut result = Vec::new();
                realize(
                    &source,
                    crate::geometry::IDENTITY,
                    n.seed(),
                    0,
                    &mut result,
                    &mut 16384,
                )?;
                Ok(content(Arc::new(result)))
            },
        ),
    ]
}
fn realize(
    source: &Content,
    parent: [f64; 6],
    identity: u64,
    depth: usize,
    result: &mut Vec<Element>,
    remaining: &mut usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("instance nesting exceeds 64".into());
    }
    for source in source.iter() {
        *remaining = remaining
            .checked_sub(1)
            .ok_or("realization exceeds 16384 elements")?;
        let mut item = source.clone();
        item.id = derived_id(identity, source.id);
        item.transform = multiply(parent, source.transform);
        if let Geometry::Instance(children) | Geometry::Group(children) = &source.geometry {
            // Preserve parent appearance via a group instead of losing overrides.
            let mut children_out = Vec::new();
            realize(
                children,
                crate::geometry::IDENTITY,
                item.id,
                depth + 1,
                &mut children_out,
                remaining,
            )?;
            item.geometry = Geometry::Group(Arc::new(children_out));
        }
        result.push(item);
    }
    Ok(())
}
