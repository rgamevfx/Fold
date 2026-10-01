use super::*;
use crate::{
    evaluation::content,
    geometry::{Element, Geometry, transform},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrokeSettings {
    pub cap: fold_render::vector::Cap,
    pub join: fold_render::vector::Join,
}
pub fn definitions() -> Vec<Definition> {
    let mut stroke = definition(
        "fold.motion.stroke",
        "Stroke",
        "Appearance",
        vec![
            Socket::required("content", Kind::Content),
            color("color", [0., 0., 0., 1.]),
            scalar("width", 1., "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let source = e.content(n, "content")?;
            let color = e.uniform(n, "color")?.color()?;
            let width = e.scalar(n, "width")?;
            if !(0. ..=10000.).contains(&width) {
                return Err("stroke width outside 0..=10000".into());
            }
            let settings = n.settings::<StrokeSettings>()?;
            Ok(content(crate::geometry::appearance::paint(
                &source,
                None,
                Some(fold_render::vector::Stroke {
                    color,
                    width,
                    cap: settings.cap,
                    join: settings.join,
                }),
            )?))
        },
    );
    stroke.defaults = || {
        serde_json::to_value(StrokeSettings {
            cap: fold_render::vector::Cap::Butt,
            join: fold_render::vector::Join::Miter,
        })
        .unwrap()
    };
    stroke.validate = |n| n.settings::<StrokeSettings>().map(|_| ());
    vec![
        definition(
            "fold.motion.fill",
            "Fill",
            "Appearance",
            vec![
                Socket::required("content", Kind::Content),
                color("color", [1., 1., 1., 1.]),
                boolean("even_odd", false),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                let color = e.uniform(n, "color")?.color()?;
                let even_odd = e.uniform(n, "even_odd")?.boolean()?;
                Ok(content(crate::geometry::appearance::paint(
                    &source,
                    Some((color, even_odd)),
                    None,
                )?))
            },
        ),
        stroke,
        definition(
            "fold.motion.transform",
            "Transform",
            "Scene",
            vec![
                Socket::required("content", Kind::Content),
                vector("translation", [0., 0.], "px"),
                scalar("rotation", 0., "degrees"),
                vector("scale", [1., 1.], "factor"),
                vector("pivot", [0., 0.], "px"),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let source = e.content(n, "content")?;
                let t = e.vector(n, "translation")?;
                let r = e.scalar(n, "rotation")?;
                let s = e.vector(n, "scale")?;
                let p = e.vector(n, "pivot")?;
                let mut group = Element::new(n.seed(), Geometry::Group(source));
                group.transform = transform(t, r, s, p);
                Ok(content(Arc::new(vec![group])))
            },
        ),
        definition(
            "fold.motion.group",
            "Group",
            "Scene",
            vec![
                Socket::required("back", Kind::Content),
                Socket::required("front", Kind::Content),
            ],
            vec![("content", Kind::Content)],
            |e, n| {
                let back = e.content(n, "back")?;
                let front = e.content(n, "front")?;
                Ok(content(Arc::new(vec![Element::new(
                    n.seed(),
                    Geometry::Group(Arc::new(back.iter().chain(front.iter()).cloned().collect())),
                )])))
            },
        ),
        definition(
            "fold.motion.output",
            "Output",
            "Scene",
            vec![Socket::required("content", Kind::Content)],
            vec![("content", Kind::Content)],
            |e, n| {
                // A newly created empty document is a valid transparent scene.
                if n.inputs.contains_key("content") {
                    Ok(content(e.content(n, "content")?))
                } else {
                    Ok(content(Arc::new(vec![])))
                }
            },
        ),
    ]
}
