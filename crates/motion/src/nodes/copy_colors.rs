//! Palette across ordered copies, preserving source typography and outlines.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Element, Geometry},
};
use std::sync::Arc;
pub fn definition_colors() -> Definition {
    definition(
        "fold.motion.copy_colors",
        "Copy Colors",
        "Instances",
        vec![
            Socket::required("content", Kind::Content),
            boolean("enabled", true),
            color("start", [0.12, 0.08, 0.8, 1.]),
            color("end", [0.6, 0.04, 0.2, 1.]),
            boolean("shapes_only", true),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            if !e.uniform(n, "enabled")?.boolean()? {
                return Ok(content(source));
            }
            let a = e.uniform(n, "start")?.color()?;
            let b = e.uniform(n, "end")?.color()?;
            if !(0. ..=1.).contains(&a[3]) || !(0. ..=1.).contains(&b[3]) {
                return Err("Palette alpha must be in 0..1".into());
            }
            let shapes = e.uniform(n, "shapes_only")?.boolean()?;
            let count = source.len();
            let mut result = Vec::with_capacity(count);
            for (i, item) in source.iter().enumerate() {
                let u = i as f64 / count.saturating_sub(1).max(1) as f64;
                let color = std::array::from_fn(|j| a[j] + (b[j] - a[j]) * u);
                result.push(tint(item, None, color, shapes, 0, &mut e.budget)?);
            }
            Ok(content(Arc::new(result)))
        },
    )
}
fn tint(
    item: &Element,
    inherited: Option<[f64; 4]>,
    color: [f64; 4],
    shapes: bool,
    depth: usize,
    budget: &mut crate::fields::Budget,
) -> Result<Element, String> {
    if depth > 64 {
        return Err("Copy color nesting exceeds limit".into());
    }
    budget.spend()?;
    let mut item = item.clone();
    let fill = inherited.or(item.fill);
    match &item.geometry {
        Geometry::Group(c) | Geometry::Instance(c) => {
            let children = c
                .iter()
                .map(|child| tint(child, fill, color, shapes, depth + 1, budget))
                .collect::<Result<Vec<_>, _>>()?;
            item.geometry = if matches!(item.geometry, Geometry::Instance(_)) {
                Geometry::Instance(Arc::new(children))
            } else {
                Geometry::Group(Arc::new(children))
            };
            // Resolve inherited fill at leaves so it cannot override the palette.
            item.fill = None;
        }
        Geometry::Path(_) => item.fill = Some(color),
        Geometry::Glyph { .. } => item.fill = if shapes { fill } else { Some(color) },
        _ => {}
    }
    Ok(item)
}

/// A reusable color field; the consumer supplies copy index/count.
pub fn definitions() -> Vec<Definition> {
    vec![
        definition_colors(),
        definition(
            "fold.motion.palette",
            "Palette",
            "Values",
            vec![
                color("start", [0.12, 0.08, 0.8, 1.]),
                color("end", [0.6, 0.04, 0.2, 1.]),
            ],
            vec![("value", Kind::Color)],
            |e, n| {
                let a = e.field(n, "start")?;
                let b = e.field(n, "end")?;
                Ok(crate::evaluation::field(crate::fields::Field::new(
                    Kind::Color,
                    true,
                    move |s, budget| {
                        a.sample(s, budget)?.mix(
                            &b.sample(s, budget)?,
                            s.index as f64 / s.count.saturating_sub(1).max(1) as f64,
                        )
                    },
                )))
            },
        ),
        definition(
            "fold.motion.apply_copy_color",
            "Apply Copy Color",
            "Instances",
            vec![
                Socket::required("content", Kind::Content),
                color("color", [1.; 4]),
                boolean("enabled", false),
                boolean("shapes_only", true),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                if !e.uniform(n, "enabled")?.boolean()? {
                    return Ok(content(source));
                }
                let color = e.field(n, "color")?;
                let shapes = e.uniform(n, "shapes_only")?.boolean()?;
                let mut result = Vec::with_capacity(source.len());
                for (index, item) in source.iter().enumerate() {
                    let sample = crate::fields::Sample {
                        index,
                        count: source.len(),
                        id: item.id,
                        position: [item.transform[4], item.transform[5]],
                        time_offset: None,
                    };
                    let color = color.sample(&sample, &mut e.budget)?.color()?;
                    if !(0. ..=1.).contains(&color[3]) {
                        return Err("Copy color alpha must be in 0..1".into());
                    }
                    result.push(tint(item, None, color, shapes, 0, &mut e.budget)?);
                }
                Ok(content(Arc::new(result)))
            },
        ),
    ]
}
