//! Shape a Text field once per selected point using the embedded font resource.
use super::*;
use crate::{
    evaluation::content,
    fields::Sample,
    geometry::{Element, Geometry, derived_id, multiply, transform},
    nodes::content::text::Settings,
};
use std::sync::Arc;

pub fn definition_labels() -> Definition {
    let mut def = definition(
        "fold.motion.text_on_points",
        "Text on Points",
        "Content",
        vec![
            Socket::required("points", Kind::Points),
            Socket::value("text", Datum::Text("Fold".into()), ""),
            scalar("size", 12., "units"),
            scalar("line_spacing", 1.2, "factor"),
            scalar("alignment", 0., "0 left, 0.5 center, 1 right"),
            vector("offset", [0., -16.], "units"),
            boolean("selection", true),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let id = n
                .settings::<Settings>()?
                .font
                .ok_or("Text on Points requires an embedded font resource")?;
            let font = e
                .motion
                .fonts
                .get(&id)
                .ok_or("missing embedded font resource")?;
            let points = e.content(n, "points")?;
            if points.len() > 16384 {
                return Err("text distribution exceeds 16384 points".into());
            }
            let text = e.field(n, "text")?;
            let size = e.field(n, "size")?;
            let offset = e.field(n, "offset")?;
            let selection = e.field(n, "selection")?;
            let spacing = e.scalar(n, "line_spacing")?;
            let alignment = e.scalar(n, "alignment")?;
            let mut labels = Vec::new();
            let mut glyph_count = 0;
            let mut segment_count = 0;
            for (index, point) in points.iter().enumerate() {
                e.budget.spend()?;
                let sample = Sample {
                    position: [point.transform[4], point.transform[5]],
                    index,
                    id: point.id,
                    count: points.len(),
                    ..Default::default()
                };
                if !selection.sample(&sample, &mut e.budget)?.boolean()? {
                    continue;
                }
                let text = text.sample(&sample, &mut e.budget)?;
                let label_id = derived_id(n.seed(), point.id);
                let glyphs = crate::geometry::text::shape(
                    text.text()?,
                    font,
                    size.sample(&sample, &mut e.budget)?.scalar()?,
                    spacing,
                    alignment,
                    label_id,
                )?;
                glyph_count += glyphs.len();
                for glyph in glyphs.iter() {
                    if let Geometry::Glyph { path, .. } = &glyph.geometry {
                        segment_count += path.len();
                    }
                }
                if glyph_count > 16384 || segment_count > fold_render::vector::MAX_SEGMENTS {
                    return Err("text distribution exceeds glyph or outline budget".into());
                }
                let mut label = Element::new(label_id, Geometry::Instance(glyphs));
                label.transform = multiply(
                    point.transform,
                    transform(
                        offset.sample(&sample, &mut e.budget)?.vector()?,
                        0.,
                        [1., 1.],
                        [0., 0.],
                    ),
                );
                label.opacity = point.opacity;
                label.attributes = point.attributes.clone();
                label.attributes.insert("text".into(), text);
                labels.push(label);
            }
            Ok(content(Arc::new(labels)))
        },
    );
    def.defaults = || serde_json::to_value(Settings::default()).unwrap();
    def.validate = |n| n.settings::<Settings>().map(|_| ());
    def.description = "Shape a Text field at each selected point. Fields sample point index, identity, count and position; labels inherit point transforms. Requires settings.font from this document's embedded fonts. Use Format Text for live numeric labels.";
    def
}
