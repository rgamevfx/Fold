use super::*;
use crate::{evaluation::field, fields::Field};
pub fn definition_direction() -> Definition {
    definition(
        "fold.motion.direction",
        "Direction",
        "Values",
        vec![
            scalar("angle", 0., "degrees"),
            scalar("length", 1., "units"),
        ],
        vec![("value", Kind::Vector)],
        |e, n| {
            let angle = e.field(n, "angle")?;
            let length = e.field(n, "length")?;
            Ok(field(Field::new(
                Kind::Vector,
                angle.varying || length.varying,
                move |s, b| {
                    let angle = angle.sample(s, b)?.scalar()?.to_radians();
                    let length = length.sample(s, b)?.scalar()?;
                    let value = Datum::Vector([angle.cos() * length, angle.sin() * length]);
                    value.validate()?;
                    Ok(value)
                },
            )))
        },
    )
}
