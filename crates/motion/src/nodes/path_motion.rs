//! Pure per-copy time offsets, arc-length distribution and named attribute transfer.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Content, Element, Geometry, derived_id, path::Path, transform},
};
use std::{collections::BTreeMap, sync::Arc};

pub fn definitions() -> Vec<Definition> {
    vec![
        definition(
            "fold.motion.path_motion",
            "Path Motion",
            "Distribution",
            vec![
                Socket::required("content", Kind::Content),
                Socket::required("path", Kind::Content),
                scalar("count", 8., "integer"),
                scalar("progress", 0., "factor"),
                scalar("speed", 0.12, "cycles/s"),
                scalar("stagger", 0.5, "seconds"),
                scalar("wave", 0., "px"),
                scalar("frequency", 1., "Hz"),
                scalar("pulse", 0., "factor"),
                boolean("orient", true),
                boolean("loop", true),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                let path = Path::from_content(&e.content(n, "path")?)?;
                let count = bounded_count(e.scalar(n, "count")?, 1024)?;
                let progress = e.scalar(n, "progress")?;
                let speed = e.scalar(n, "speed")?;
                let stagger = e.scalar(n, "stagger")?;
                let wave = e.scalar(n, "wave")?;
                let frequency = e.scalar(n, "frequency")?;
                let pulse = e.scalar(n, "pulse")?;
                let orient = e.uniform(n, "orient")?.boolean()?;
                let looping = e.uniform(n, "loop")?.boolean()?;
                if !(0. ..=1.).contains(&pulse) || stagger < 0. {
                    return Err("pulse must be 0..1 and stagger nonnegative".into());
                }
                let seconds = e.time.numerator() as f64 / e.time.denominator() as f64;
                let mut copies = Vec::with_capacity(count);
                for i in 0..count {
                    e.budget.spend()?;
                    let local = seconds - i as f64 * stagger;
                    let u = progress + local * speed;
                    let u = if looping {
                        u.rem_euclid(1.)
                    } else {
                        u.clamp(0., 1.)
                    };
                    let (mut p, angle) = path.sample(u);
                    let phase = local * frequency * std::f64::consts::TAU;
                    let normal = angle.to_radians();
                    p[0] -= normal.sin() * wave * phase.sin();
                    p[1] += normal.cos() * wave * phase.sin();
                    let scale = 1. + pulse * phase.sin();
                    let mut copy = Element::new(
                        derived_id(n.seed(), i as u64),
                        Geometry::Instance(source.clone()),
                    );
                    copy.transform =
                        transform(p, if orient { angle } else { 0. }, [scale; 2], [0.; 2]);
                    for (name, stops) in &path.attributes {
                        copy.attributes
                            .insert(name.clone(), crate::geometry::path::attribute(stops, u)?);
                    }
                    copy.attributes.insert("path_u".into(), Datum::Scalar(u));
                    copy.attributes.insert("index".into(), Datum::Id(i as u64));
                    copy.attributes.insert("time".into(), Datum::Scalar(local));
                    copies.push(copy);
                }
                Ok(content(Arc::new(copies)))
            },
        ),
        definition(
            "fold.motion.transfer_color",
            "Transfer Attribute to Color",
            "Attributes",
            vec![
                Socket::required("content", Kind::Content),
                Socket::value("attribute", Datum::Text("color".into()), ""),
                boolean("text_only", true),
                boolean("enabled", true),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                if !e.uniform(n, "enabled")?.boolean()? {
                    return Ok(content(source));
                }
                let name = e.uniform(n, "attribute")?.text()?.to_owned();
                let text_only = e.uniform(n, "text_only")?.boolean()?;
                fn walk(
                    source: &Content,
                    inherited: &BTreeMap<String, Datum>,
                    inherited_fill: Option<[f64; 4]>,
                    name: &str,
                    text_only: bool,
                    depth: usize,
                    budget: &mut crate::fields::Budget,
                ) -> Result<Content, String> {
                    if depth > 64 {
                        return Err("attribute transfer nesting limit".into());
                    }
                    let mut result = vec![];
                    for item in source.iter() {
                        budget.spend()?;
                        let mut item = item.clone();
                        let mut attrs = inherited.clone();
                        attrs.extend(item.attributes.clone());
                        let fill = inherited_fill.or(item.fill);
                        item.fill = fill;
                        match &item.geometry {
                            Geometry::Group(c) => {
                                item.geometry = Geometry::Group(walk(
                                    c,
                                    &attrs,
                                    fill,
                                    name,
                                    text_only,
                                    depth + 1,
                                    budget,
                                )?)
                            }
                            Geometry::Instance(c) => {
                                item.geometry = Geometry::Instance(walk(
                                    c,
                                    &attrs,
                                    fill,
                                    name,
                                    text_only,
                                    depth + 1,
                                    budget,
                                )?)
                            }
                            Geometry::Glyph { .. } | Geometry::Path(_)
                                if !text_only
                                    || matches!(item.geometry, Geometry::Glyph { .. }) =>
                            {
                                let color = attrs
                                    .get(name)
                                    .ok_or_else(|| format!("missing color attribute: {name}"))?
                                    .color()?;
                                item.fill = Some(color);
                            }
                            _ => {}
                        }
                        if matches!(item.geometry, Geometry::Group(_) | Geometry::Instance(_)) {
                            item.fill = None;
                        }
                        result.push(item);
                    }
                    Ok(Arc::new(result))
                }
                Ok(content(walk(
                    &source,
                    &BTreeMap::new(),
                    None,
                    &name,
                    text_only,
                    0,
                    &mut e.budget,
                )?))
            },
        ),
    ]
}
