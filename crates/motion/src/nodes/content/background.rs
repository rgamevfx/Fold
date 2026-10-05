//! A content-aware rectangle used by editable label/badge assets.
use crate::{
    evaluation::content,
    geometry::{Content, Element, Geometry, IDENTITY, multiply, shapes},
    nodes::*,
};
use std::sync::Arc;
pub fn definition_background() -> Definition {
    definition(
        "fold.motion.fit_background",
        "Fit Background",
        "Content",
        vec![
            Socket::required("content", Kind::Content),
            vector("padding", [16., 10.], "px"),
            scalar("radius", 10., "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            let padding = e.vector(n, "padding")?;
            if padding.iter().any(|v| *v < 0.) {
                return Err("Padding must be nonnegative".into());
            }
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            measure(&source, IDENTITY, &mut bounds, 0, &mut e.budget)?;
            if !bounds.iter().all(|v| v.is_finite()) {
                bounds = [0.; 4];
            }
            let width = bounds[2] - bounds[0] + padding[0] * 2.;
            let height = bounds[3] - bounds[1] + padding[1] * 2.;
            super::dimensions(width, height)?;
            let radius = e.scalar(n, "radius")?;
            if radius < 0. {
                return Err("Corner radius must be nonnegative".into());
            }
            let mut shape = Element::new(
                n.seed(),
                Geometry::Path(Arc::new(shapes::rectangle(width, height, radius))),
            );
            shape.transform[4] = bounds[0] - padding[0];
            shape.transform[5] = bounds[1] - padding[1];
            Ok(content(Arc::new(vec![shape])))
        },
    )
}
fn measure(
    content: &Content,
    parent: [f64; 6],
    bounds: &mut [f64; 4],
    depth: usize,
    budget: &mut crate::fields::Budget,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Background source nesting exceeds limit".into());
    }
    for item in content.iter() {
        budget.spend()?;
        let m = multiply(parent, item.transform);
        match &item.geometry {
            Geometry::Group(c) | Geometry::Instance(c) => measure(c, m, bounds, depth + 1, budget)?,
            Geometry::Path(p) | Geometry::Glyph { path: p, .. } => {
                for segment in p.iter() {
                    use fold_render::vector::Segment::*;
                    let points: &[[f64; 2]] = match segment {
                        Move(p) | Line(p) => std::slice::from_ref(p),
                        Quad(a, b) => {
                            for p in [a, b] {
                                include(m, *p, bounds);
                            }
                            continue;
                        }
                        Cubic(a, b, c) => {
                            for p in [a, b, c] {
                                include(m, *p, bounds);
                            }
                            continue;
                        }
                        Close => &[],
                    };
                    for p in points {
                        include(m, *p, bounds);
                    }
                }
            }
            Geometry::Point => {}
        }
    }
    Ok(())
}
fn include(m: [f64; 6], p: [f64; 2], bounds: &mut [f64; 4]) {
    let x = m[0] * p[0] + m[2] * p[1] + m[4];
    let y = m[1] * p[0] + m[3] * p[1] + m[5];
    bounds[0] = bounds[0].min(x);
    bounds[1] = bounds[1].min(y);
    bounds[2] = bounds[2].max(x);
    bounds[3] = bounds[3].max(y);
}
