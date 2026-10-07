//! Uniform format values in logical scene units, independent of raster quality.
use super::*;
use crate::{evaluation::Value, fields::Field};
pub fn definition_format() -> Definition {
    let mut d = definition(
        "fold.motion.format",
        "Format",
        "Layout",
        vec![],
        vec![
            ("size", Kind::Vector),
            ("center", Kind::Vector),
            ("width", Kind::Scalar),
            ("height", Kind::Scalar),
            ("aspect", Kind::Scalar),
            ("pixel_size", Kind::Vector),
        ],
        |e, _| {
            let [w, h] = e.canvas_size()?;
            let format = e.output_format.unwrap_or(e.motion.output_format());
            let matrix = e.motion.canvas_matrix(format, format.dimensions)?;
            Ok([
                ("size", Datum::Vector([w, h])),
                ("center", Datum::Vector([w / 2., h / 2.])),
                ("width", Datum::Scalar(w)),
                ("height", Datum::Scalar(h)),
                ("aspect", Datum::Scalar(w / h)),
                ("pixel_size", Datum::Vector([1. / matrix.a, 1. / matrix.d])),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), Value::Field(Field::constant(v))))
            .collect())
        },
    );
    d.description = "Logical canvas size and center in scene units. Pixel size gives scene units per full-output pixel; multiply pixel distances by it. Preview resolution does not change layout.";
    d
}
pub fn definition_anchor() -> Definition {
    let mut d = definition(
        "fold.motion.anchor",
        "Anchor",
        "Layout",
        vec![
            vector("anchor", [0.5; 2], "normalized"),
            vector("offset", [0.; 2], "%"),
        ],
        vec![("value", Kind::Vector)],
        |e, n| {
            let size = e.canvas_size()?;
            let anchor = e.field(n, "anchor")?;
            let offset = e.field(n, "offset")?;
            Ok(crate::evaluation::field(anchor.zip(
                offset,
                Kind::Vector,
                move |a, b| {
                    let a = a.vector()?;
                    let b = b.vector()?;
                    Ok(Datum::Vector([
                        (a[0] + b[0] / 100.) * size[0],
                        (a[1] + b[1] / 100.) * size[1],
                    ]))
                },
            )))
        },
    );
    d.description = "Position relative to the logical canvas. Connect to an object's translation; offsets are percentages.";
    d
}
