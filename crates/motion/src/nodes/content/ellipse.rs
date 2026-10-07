use crate::{
    evaluation::content, fields::Kind, geometry::shapes, graph::definition::Definition, nodes::*,
};
pub fn definition() -> Definition {
    super::super::definition(
        "fold.motion.ellipse",
        "Ellipse",
        "Content",
        vec![
            scalar("width", 100., "units"),
            scalar("height", 100., "units"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let w = e.scalar(n, "width")?;
            let h = e.scalar(n, "height")?;
            super::dimensions(w, h)?;
            Ok(content(super::shape(n, shapes::ellipse(w, h))))
        },
    )
}
