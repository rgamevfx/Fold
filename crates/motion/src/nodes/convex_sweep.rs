//! Convex silhouette of a profile translated through a vector, per top-level element.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Geometry, IDENTITY, sampling},
};
use fold_render::vector::Segment;
use std::sync::Arc;
pub fn definition_sweep() -> Definition {
    definition(
        "fold.motion.convex_sweep",
        "Convex Sweep",
        "Geometry",
        vec![
            Socket::required("content", Kind::Content),
            boolean("enabled", true),
            vector("offset", [0., 0.], "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            if !e.uniform(n, "enabled")?.boolean()? {
                return Ok(content(source));
            }
            let offset = e.field(n, "offset")?;
            let mut result = Vec::new();
            for (index, item) in source.iter().enumerate() {
                e.budget.spend()?;
                let v = offset
                    .sample(
                        &super::attributes::sample(item, index, source.len()),
                        &mut e.budget,
                    )?
                    .vector()?;
                if !v.iter().all(|x| x.is_finite() && x.abs() <= 1e6) {
                    return Err("invalid sweep offset".into());
                }
                let drawings = crate::geometry::drawings_with(
                    &Arc::new(vec![item.clone()]),
                    [1., 1.],
                    e.budget.cancel(),
                )?;
                if drawings
                    .windows(2)
                    .any(|pair| pair[0].fill != pair[1].fill || pair[0].stroke != pair[1].stroke)
                {
                    return Err("Convex Sweep requires a uniform profile style; separate differently styled profiles first".into());
                }
                let mut points = Vec::new();
                for drawing in &drawings {
                    let m = drawing.transform;
                    for p in sampling::contours(&drawing.path, 0.1, &mut e.budget)?
                        .into_iter()
                        .flatten()
                    {
                        let p = [
                            m[0] * p[0] + m[2] * p[1] + m[4],
                            m[1] * p[0] + m[3] * p[1] + m[5],
                        ];
                        if points.len() >= 65536 {
                            return Err("Convex Sweep profile exceeds 65536 samples".into());
                        }
                        points.push(p);
                        points.push([p[0] + v[0], p[1] + v[1]]);
                    }
                }
                let hull = sampling::hull(points);
                let mut path = Vec::new();
                if let Some(first) = hull.first() {
                    path.push(Segment::Move(*first));
                    path.extend(hull.iter().skip(1).copied().map(Segment::Line));
                    path.push(Segment::Close);
                }
                let mut out = item.clone();
                out.geometry = Geometry::Path(Arc::new(path));
                out.transform = IDENTITY;
                out.even_odd = false;
                // Flattening must preserve the source appearance, including instance styles.
                if let Some(d) = drawings.first() {
                    out.fill = d.fill;
                    out.stroke = d.stroke.clone();
                    out.opacity = 1.;
                }
                result.push(out);
            }
            Ok(content(Arc::new(result)))
        },
    )
}
