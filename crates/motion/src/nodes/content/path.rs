use crate::{evaluation::content, fields::Kind, graph::definition::Definition};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub segments: Vec<fold_render::vector::Segment>,
}
pub fn definition() -> Definition {
    let mut d = crate::nodes::definition(
        "fold.motion.path",
        "Path",
        "Content",
        vec![],
        vec![("content", Kind::Content)],
        |_, n| Ok(content(super::shape(n, n.settings::<Settings>()?.segments))),
    );
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.validate = |n| {
        let s = n.settings::<Settings>()?;
        fold_render::vector::validate(&[fold_render::vector::Drawing {
            path: std::sync::Arc::new(s.segments),
            transform: crate::geometry::IDENTITY,
            fill: None,
            stroke: None,
            even_odd: false,
        }])
    };
    d
}
