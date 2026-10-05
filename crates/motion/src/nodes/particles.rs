//! Point-state particle primitives. Solver owns time and replay; these operations
//! only initialize or advance the supplied immutable state.
use super::*;
use crate::{
    evaluation::{Value, output},
    geometry::{Content, Element, Geometry, derived_id},
};
use std::sync::Arc;
pub(crate) const TARGET: &str = "particle.target";
pub(crate) const VELOCITY: &str = "velocity";

pub(crate) fn random(seed: u64, index: u64) -> f64 {
    (derived_id(seed, index) >> 11) as f64 / ((1u64 << 53) as f64)
}
pub(crate) fn points(e: &mut Evaluator<'_>, n: &Node) -> Result<Content, String> {
    let Value::Points(points) = e.input(n, "points")? else {
        return Err("Particles require Points".into());
    };
    if points.len() > 16384 {
        return Err("Particles exceed 16384 points".into());
    }
    for point in points.iter() {
        e.budget.spend()?;
        if !matches!(point.geometry, Geometry::Point)
            || point
                .transform
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e9)
        {
            return Err("Particles require finite, flat point geometry".into());
        }
    }
    Ok(points)
}
pub fn definitions() -> Vec<Definition> {
    let mut scatter = definition(
        "fold.motion.scatter_points",
        "Scatter Points",
        "Distribution",
        vec![
            scalar("count", 800., "integer"),
            vector("size", [960., 540.], "px"),
            vector("center", [480., 270.], "px"),
            vector("jitter", [0., 0.], "px"),
            boolean("perimeter", false),
            scalar("seed", 1., "integer"),
        ],
        vec![("points", Kind::Points)],
        |e, n| {
            let count = bounded_count(e.scalar(n, "count")?, 16384)?;
            let size = e.vector(n, "size")?;
            let center = e.vector(n, "center")?;
            let jitter = e.vector(n, "jitter")?;
            if jitter.iter().any(|v| *v < 0. || *v > 1e8)
                || size.iter().any(|v| *v < 0. || *v > 1e8)
                || center.iter().any(|v| v.abs() > 1e8)
            {
                return Err("Scatter Points needs nonnegative size and bounded coordinates".into());
            }
            let perimeter = e.uniform(n, "perimeter")?.boolean()?;
            let seed = e.scalar(n, "seed")?.to_bits();
            let mut result = Vec::with_capacity(count);
            for i in 0..count {
                e.budget.spend()?;
                let id = derived_id(n.seed(), i as u64);
                let u = random(seed, id);
                let v = random(seed.wrapping_add(1), id);
                let mut xy = [u * size[0], v * size[1]];
                if perimeter {
                    let d = u * 2. * (size[0] + size[1]);
                    xy = if d < size[0] {
                        [d, 0.]
                    } else if d < size[0] + size[1] {
                        [size[0], d - size[0]]
                    } else if d < 2. * size[0] + size[1] {
                        [2. * size[0] + size[1] - d, size[1]]
                    } else {
                        [0., 2. * (size[0] + size[1]) - d]
                    };
                }
                let mut point = Element::new(id, Geometry::Point);
                point.transform[4] = center[0] + xy[0] - size[0] * 0.5;
                point.transform[5] = center[1] + xy[1] - size[1] * 0.5;
                for axis in 0..2 {
                    point.transform[4 + axis] +=
                        (random(seed.wrapping_add(2 + axis as u64), id) * 2. - 1.) * jitter[axis];
                }
                result.push(point);
            }
            Ok(output("points", Value::Points(Arc::new(result))))
        },
    );
    scatter.description = "Stable seeded points in a rectangle or uniformly along its perimeter. Changing the seed moves points without changing their identities.";
    let mut step = definition(
        "fold.motion.particle_step",
        "Particle Step",
        "Simulation",
        vec![
            Socket::required("points", Kind::Points),
            scalar("delta_time", 0.04, "seconds"),
            scalar("attraction", 1., "0–1"),
            scalar("frequency", 0.5, "Hz"),
            scalar("variation", 0., "0–1"),
            scalar("damping", 1., "ratio"),
            vector("acceleration", [0., 0.], "px/s²"),
            scalar("turbulence", 0., "px/s²"),
            scalar("seed", 1., "integer"),
        ],
        vec![("points", Kind::Points)],
        advance,
    );
    step.description = "Advance flat points with a stable implicit spring and drag. Reads particle.target and velocity attributes, preserves identity and other attributes. Use inside Solver with Previous State and delta_time. Attraction, frequency, damping and forces are sampled each step; targets are assigned separately.";
    vec![scatter, super::particle_targets::definition_targets(), step]
}

fn advance(e: &mut Evaluator<'_>, n: &Node) -> Result<Outputs, String> {
    let source = points(e, n)?;
    let dt = e.scalar(n, "delta_time")?;
    let attraction = e.scalar(n, "attraction")?;
    let frequency = e.scalar(n, "frequency")?;
    let damping = e.scalar(n, "damping")?;
    let variation = e.scalar(n, "variation")?;
    let acceleration = e.vector(n, "acceleration")?;
    let turbulence = e.scalar(n, "turbulence")?;
    let seed = e.scalar(n, "seed")?.to_bits();
    if !(0. ..=1.).contains(&dt)
        || !(0. ..=1.).contains(&attraction)
        || !(0. ..=1.).contains(&variation)
        || !(0. ..=100.).contains(&frequency)
        || !(0. ..=100.).contains(&damping)
        || !(0. ..=1e6).contains(&turbulence)
        || acceleration.iter().any(|v| v.abs() > 1e6)
    {
        return Err("Particle Step needs dt, attraction and variation in 0–1, frequency/damping in 0–100, and forces within ±1000000".into());
    }
    let time = e.time.numerator() as f64 / e.time.denominator() as f64;
    let mut result = Vec::with_capacity(source.len());
    for point in source.iter() {
        e.budget.spend()?;
        let mut point = point.clone();
        let position = [point.transform[4], point.transform[5]];
        let omega = std::f64::consts::TAU
            * frequency
            * (1. - variation + 2. * variation * random(seed, point.id));
        let stiffness = omega * omega * attraction;
        let drag = 2. * damping * omega;
        let denominator = 1. + drag * dt + stiffness * dt * dt;

        let target = match point.attributes.get(TARGET) {
            Some(value) => value.vector()?,
            None if attraction == 0. => position,
            None => return Err("Assign Particle Targets before enabling attraction".into()),
        };
        let mut velocity = point
            .attributes
            .get(VELOCITY)
            .map(Datum::vector)
            .transpose()?
            .unwrap_or([0.; 2]);
        for axis in 0..2 {
            let phase = random(seed.wrapping_add(axis as u64), point.id) * std::f64::consts::TAU;
            let force = acceleration[axis] + turbulence * (time * 2.3 + phase).sin();
            velocity[axis] = (velocity[axis]
                + dt * (stiffness * (target[axis] - position[axis]) + force))
                / denominator;
            point.transform[4 + axis] += velocity[axis] * dt;
            if !velocity[axis].is_finite()
                || !point.transform[4 + axis].is_finite()
                || point.transform[4 + axis].abs() > 1e9
            {
                return Err("Particle state exceeds coordinate bounds".into());
            }
        }
        point
            .attributes
            .insert(VELOCITY.into(), Datum::Vector(velocity));
        result.push(point);
    }
    Ok(output("points", Value::Points(Arc::new(result))))
}
