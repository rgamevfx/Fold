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

pub fn definition_fit() -> Definition {
    let mut d = definition(
        "fold.motion.fit_content",
        "Fit Content",
        "Geometry",
        vec![
            Socket::required("content", Kind::Content),
            vector("origin", [0., 0.], "px"),
            vector("size", [640., 360.], "px"),
            vector("anchor", [0.5, 0.5], "fraction"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            let origin = e.vector(n, "origin")?;
            let size = e.vector(n, "size")?;
            let anchor = e.vector(n, "anchor")?;
            if origin.iter().any(|v| !v.is_finite() || v.abs() > 1e9)
                || size.iter().any(|v| !v.is_finite() || *v <= 0. || *v > 1e9)
                || anchor
                    .iter()
                    .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            {
                return Err("Fit Content requires finite positive size and anchor in 0..1".into());
            }
            let Some(b) = bounds::content_with_budget(&source, &mut e.budget)? else {
                return Ok(content(source));
            };
            let dimensions = [b[2] - b[0], b[3] - b[1]];
            let scale = (0..2)
                .filter(|&i| dimensions[i] > f64::EPSILON)
                .map(|i| size[i] / dimensions[i])
                .reduce(f64::min)
                .unwrap_or(1.);
            let mut group = Element::new(n.seed(), Geometry::Group(source));
            group.transform = [
                scale,
                0.,
                0.,
                scale,
                origin[0] + (size[0] - dimensions[0] * scale) * anchor[0] - b[0] * scale,
                origin[1] + (size[1] - dimensions[1] * scale) * anchor[1] - b[1] * scale,
            ];
            Ok(content(Arc::new(vec![group])))
        },
    );
    d.description = "Uniform contain fit of analytic outlines into a box in input coordinates. Recomputes for text/font/size edits at exact time; preserves child identity. Anchor controls placement of remaining space. Excludes stroke, masks and effects; empty input passes through.";
    d
}
