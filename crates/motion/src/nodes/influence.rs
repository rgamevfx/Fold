use super::*;
use crate::{evaluation::field, fields::Field};
fn weight(distance: f64, inner: f64, outer: f64, smooth: bool, invert: bool) -> f64 {
    let mut t = ((distance - inner) / (outer - inner)).clamp(0., 1.);
    if smooth {
        t = t * t * (3. - 2. * t);
    }
    if invert { t } else { 1. - t }
}
pub fn definitions() -> Vec<Definition> {
    vec![
        definition(
            "fold.motion.range_falloff",
            "Range Falloff",
            "Influence",
            vec![
                scalar("coordinate", 0., ""),
                scalar("start", 0., ""),
                scalar("end", 1., ""),
                boolean("smooth", true),
                boolean("invert", false),
            ],
            vec![("value", Kind::Scalar)],
            |e, n| {
                let coordinate = e.field(n, "coordinate")?;
                let a = e.scalar(n, "start")?;
                let b = e.scalar(n, "end")?;
                if a >= b {
                    return Err("falloff start must precede end".into());
                }
                let smooth = e.uniform(n, "smooth")?.boolean()?;
                let invert = e.uniform(n, "invert")?.boolean()?;
                Ok(field(coordinate.map(Kind::Scalar, move |v| {
                    Ok(Datum::Scalar(weight(v.scalar()?, a, b, smooth, invert)))
                })))
            },
        ),
        definition(
            "fold.motion.radial_falloff",
            "Radial Falloff",
            "Influence",
            vec![
                vector("position", [0., 0.], "px"),
                vector("center", [0., 0.], "px"),
                scalar("inner", 0., "px"),
                scalar("outer", 100., "px"),
                boolean("smooth", true),
                boolean("invert", false),
            ],
            vec![("value", Kind::Scalar)],
            |e, n| {
                let p = e.field(n, "position")?;
                let c = e.field(n, "center")?;
                let inner = e.scalar(n, "inner")?;
                let outer = e.scalar(n, "outer")?;
                if inner < 0. || inner >= outer {
                    return Err("radial falloff requires 0 <= inner < outer".into());
                }
                let smooth = e.uniform(n, "smooth")?.boolean()?;
                let invert = e.uniform(n, "invert")?.boolean()?;
                Ok(field(Field::new(
                    Kind::Scalar,
                    p.varying || c.varying,
                    move |s, b| {
                        let p = p.sample(s, b)?.vector()?;
                        let c = c.sample(s, b)?.vector()?;
                        Ok(Datum::Scalar(weight(
                            (p[0] - c[0]).hypot(p[1] - c[1]),
                            inner,
                            outer,
                            smooth,
                            invert,
                        )))
                    },
                )))
            },
        ),
    ]
}
