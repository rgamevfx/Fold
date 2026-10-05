pub mod background;
pub mod ellipse;
pub mod path;
pub mod rectangle;
pub mod text;
use crate::{
    geometry::{Content, Element, Geometry},
    graph::Node,
};
use std::sync::Arc;
pub fn shape(node: &Node, path: Vec<fold_render::vector::Segment>) -> Content {
    Arc::new(vec![Element::new(
        node.seed(),
        Geometry::Path(Arc::new(path)),
    )])
}
pub fn dimensions(width: f64, height: f64) -> Result<(), String> {
    if width < 0. || height < 0. || width > 1e6 || height > 1e6 {
        Err("shape dimensions must be in 0..=1000000 pixels".into())
    } else {
        Ok(())
    }
}
