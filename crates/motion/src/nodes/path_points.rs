//! Path distribution is independent of travel; attributes stay on the points.
use super::*;
use crate::{
    evaluation::{Value, output},
    geometry::{Element, Geometry, derived_id, path::Path, transform},
};
use std::sync::Arc;

pub fn definition_points() -> Definition {
    definition(
        "fold.motion.path_points",
        "Path Points",
        "Distribution",
        vec![
            Socket::required("path", Kind::Content),
            scalar("count", 10., "integer"),
            scalar("offset", 0., "factor"),
            scalar("duration", 10., "seconds"),
            boolean("travel", false),
            boolean("closed", true),
            boolean("orient", true),
            scalar("wave", 0., "px"),
            scalar("frequency", 0.4, "Hz"),
            scalar("pulse", 0., "factor"),
        ],
        vec![("points", Kind::Points)],
        |e, n| {
            let path = Path::from_content(&e.content(n, "path")?)?;
            let count = bounded_count(e.scalar(n, "count")?, 1024)?;
            let offset = e.scalar(n, "offset")?;
            let duration = e.scalar(n, "duration")?;
            if duration <= 0. {
                return Err("Loop duration must be greater than zero".into());
            }
            let travel = e.uniform(n, "travel")?.boolean()?;
            let closed = e.uniform(n, "closed")?.boolean()?;
            let orient = e.uniform(n, "orient")?.boolean()?;
            let wave = e.scalar(n, "wave")?;
            let frequency = e.scalar(n, "frequency")?;
            let pulse = e.scalar(n, "pulse")?;
            if !(0. ..=1.).contains(&pulse) {
                return Err("Pulse must be between zero and one".into());
            }
            let seconds = e.time.numerator() as f64 / e.time.denominator() as f64;
            let progress = offset + if travel { seconds / duration } else { 0. };
            // Traveling copies wrap as a train even on an open curve. Clamping
            // individual copies would collapse spacing as speed changes.
            let looping = closed || travel;
            let denominator = if looping {
                count.max(1)
            } else {
                count.saturating_sub(1).max(1)
            } as f64;
            let mut result = Vec::with_capacity(count);
            for i in 0..count {
                e.budget.spend()?;
                let fraction = i as f64 / denominator;
                let u = progress + fraction;
                let u = if looping {
                    u.rem_euclid(1.)
                } else {
                    u.clamp(0., 1.)
                };
                let (mut p, angle) = path.sample(u);
                let phase = (seconds * frequency - fraction) * std::f64::consts::TAU;
                p[0] -= angle.to_radians().sin() * wave * phase.sin();
                p[1] += angle.to_radians().cos() * wave * phase.sin();
                let scale = 1. + pulse * phase.sin();
                let mut point = Element::new(derived_id(n.seed(), i as u64), Geometry::Point);
                point.transform =
                    transform(p, if orient { angle } else { 0. }, [scale; 2], [0.; 2]);
                for (name, stops) in &path.attributes {
                    point
                        .attributes
                        .insert(name.clone(), crate::geometry::path::attribute(stops, u)?);
                }
                point.attributes.insert("path_u".into(), Datum::Scalar(u));
                point.attributes.insert("index".into(), Datum::Id(i as u64));
                result.push(point);
            }
            Ok(output("points", Value::Points(Arc::new(result))))
        },
    )
}
