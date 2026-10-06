//! Whole-content alignment recomputes from native outlines at evaluation time.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Element, Geometry, bounds},
};
use std::sync::Arc;
pub fn definition_align() -> Definition {
    let mut d = definition(
        "fold.motion.align_content",
        "Align Content",
        "Geometry",
        vec![
            Socket::required("content", Kind::Content),
            vector("target", [0., 0.], "input coordinates"),
            vector("anchor", [0.5, 0.5], "fraction of outline bounds"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            let target = e.vector(n, "target")?;
            let anchor = e.vector(n, "anchor")?;
            if target.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
                || anchor
                    .iter()
                    .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            {
                return Err(
                    "Align Content requires finite target coordinates and anchor in 0..1".into(),
                );
            }
            let Some(bounds) = bounds::content_with_budget(&source, &mut e.budget)? else {
                return Ok(content(source));
            };
            let mut group = Element::new(n.seed(), Geometry::Group(source));
            for axis in 0..2 {
                group.transform[4 + axis] = target[axis]
                    - (bounds[axis] + anchor[axis] * (bounds[axis + 2] - bounds[axis]));
            }
            Ok(content(Arc::new(vec![group])))
        },
    );
    d.description = "Align the whole input's analytic outline bounds to target at every exact evaluation time. Anchor [0,0] is top-left, [0.5,0.5] center, [1,1] bottom-right. Preserves child/glyph relationships in a wrapper; follows text/font/size changes. Coordinates are those of the input; a downstream object transform still applies. Excludes stroke, masks and effects; empty geometry passes through. Whole-content operation, not per-glyph alignment.";
    d
}
