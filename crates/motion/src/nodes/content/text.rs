use crate::{
    evaluation::content,
    fields::{Datum, Kind},
    graph::definition::{Definition, Socket},
    nodes::{definition, scalar},
};
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub font: Option<ObjectId>,
}
pub fn definition_text() -> Definition {
    let mut d = definition(
        "fold.motion.text",
        "Text",
        "Content",
        vec![
            Socket::value("text", Datum::Text("Fold".into()), ""),
            scalar("size", 72., "px"),
            scalar("line_spacing", 1.2, "factor"),
            scalar("alignment", 0., "0 left, 0.5 center, 1 right"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            let id = settings
                .font
                .ok_or("Text requires an embedded font resource")?;
            let font = e
                .motion
                .fonts
                .get(&id)
                .ok_or("missing embedded font resource")?;
            let text = e.uniform(n, "text")?.text()?.to_owned();
            let size = e.scalar(n, "size")?;
            let spacing = e.scalar(n, "line_spacing")?;
            let alignment = e.scalar(n, "alignment")?;
            Ok(content(crate::geometry::text::shape(
                &text,
                font,
                size,
                spacing,
                alignment,
                n.seed(),
            )?))
        },
    );
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.validate = |n| n.settings::<Settings>().map(|_| ());
    d
}
