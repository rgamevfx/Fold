//! Seeded filled-shape targets are assigned once in Solver's initial state.
use super::*;
use crate::{
    evaluation::{Value, output},
    geometry::sampling,
};
use std::sync::Arc;

pub fn definition_targets() -> Definition {
    let mut d = definition(
        "fold.motion.particle_targets",
        "Particle Targets",
        "Simulation",
        vec![
            Socket::required("points", Kind::Points),
            Socket::required("attractor", Kind::Content),
            scalar("seed", 1., "integer"),
        ],
        vec![("points", Kind::Points)],
        |e, n| {
            let points = super::particles::points(e, n)?;
            if points.is_empty() {
                return Ok(output("points", Value::Points(points)));
            }
            let attractor = e.content(n, "attractor")?;
            let seed = e.scalar(n, "seed")?.to_bits();
            let mut shapes = vec![];
            let mut area = 0.;
            for drawing in sampling::filled_shapes(&attractor, &mut e.budget)? {
                let [x0, y0, x1, y1] = drawing.bounds;
                let min = [x0, y0];
                let max = [x1, y1];
                let contours = drawing.contours;
                let weight = (max[0] - min[0]) * (max[1] - min[1]);
                if weight.is_finite() && weight > 0. {
                    area += weight;
                    shapes.push((area, contours, min, max, drawing.even_odd));
                }
            }
            if shapes.is_empty() {
                return Err("Attractor needs nonempty filled paths or text".into());
            }
            let mut result = Vec::with_capacity(points.len());
            for point in points.iter() {
                let mut point = point.clone();
                let mut target = None;
                for attempt in 0..256u64 {
                    e.budget.spend()?;
                    let salt = crate::geometry::derived_id(point.id, attempt);
                    let choose = super::particles::random(seed, salt) * area;
                    let index = shapes
                        .partition_point(|s| s.0 <= choose)
                        .min(shapes.len() - 1);
                    let (_, contours, min, max, even_odd) = &shapes[index];
                    let p = std::array::from_fn(|i| {
                        min[i]
                            + (max[i] - min[i])
                                * super::particles::random(seed.wrapping_add(i as u64 + 1), salt)
                    });
                    if sampling::contains(contours, p, *even_odd, &mut e.budget)? {
                        target = Some(p);
                        break;
                    }
                }
                let target = target.ok_or(
                    "Attractor is too thin or empty to sample; use a filled shape with more area",
                )?;
                point
                    .attributes
                    .insert(super::particles::TARGET.into(), Datum::Vector(target));
                result.push(point);
            }
            Ok(output("points", Value::Points(Arc::new(result))))
        },
    );
    d.description = "Assign a stable random target inside transformed filled paths/text to every input point. Preserves point identity, velocity and other attributes. Connect before Solver initial: shape edits rebuild targets; animated shape motion after Solver start is not followed. Holes respect the fill rule. Strokes, effects and collisions are not sampled.";
    d
}
