use crate::{
    evaluation::content, fields::Kind, geometry::shapes, graph::definition::Definition, nodes::*,
};
pub fn definition() -> Definition {
    super::super::definition(
        "fold.motion.rectangle",
        "Rectangle",
        "Content",
        vec![
            scalar("width", 100., "px"),
            scalar("height", 100., "px"),
            scalar("radius", 0., "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let w = e.scalar(n, "width")?;
            let h = e.scalar(n, "height")?;
            let r = e.scalar(n, "radius")?;
            super::dimensions(w, h)?;
            if r < 0. {
                return Err("corner radius must be nonnegative".into());
            }
            Ok(content(super::shape(n, shapes::rectangle(w, h, r))))
        },
    )
}
