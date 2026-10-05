use crate::{evaluation::content, fields::Kind, graph::definition::Definition};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub attributes: crate::geometry::path::Attributes,
    pub segments: Vec<fold_render::vector::Segment>,
    #[serde(flatten)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
pub fn definition() -> Definition {
    let mut d = crate::nodes::definition(
        "fold.motion.path",
        "Path",
        "Content",
        vec![],
        vec![("content", Kind::Content)],
        |_, n| {
            let settings = n.settings::<Settings>()?;
            let mut shape = super::shape(n, settings.segments);
            for e in std::sync::Arc::make_mut(&mut shape) {
                e.path_attributes = settings.attributes.clone();
            }
            Ok(content(shape))
        },
    );
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.validate = |n| {
        let s = n.settings::<Settings>()?;
        crate::geometry::path::validate(&s.attributes)?;
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
