//! Matte geometry uses the existing shared vector rasterizer.
use fold_render::vector::{Drawing, Segment};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Rectangle,
    Ellipse,
}
impl Shape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
        }
    }
}
pub(super) fn drawing(shape: Shape, bounds: [f64; 4], scale: [f64; 2]) -> Result<Drawing, String> {
    if bounds.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
        || bounds[0] > bounds[2]
        || bounds[1] > bounds[3]
    {
        return Err("Shape requires finite ordered bounds".into());
    }
    let [x, y, r, b] = bounds;
    let path = match shape {
        Shape::Rectangle => vec![
            Segment::Move([x, y]),
            Segment::Line([r, y]),
            Segment::Line([r, b]),
            Segment::Line([x, b]),
            Segment::Close,
        ],
        Shape::Ellipse => {
            let cx = (x + r) * 0.5;
            let cy = (y + b) * 0.5;
            let rx = (r - x) * 0.5;
            let ry = (b - y) * 0.5;
            let k = 0.5522847498307936;
            vec![
                Segment::Move([r, cy]),
                Segment::Cubic([r, cy + k * ry], [cx + k * rx, b], [cx, b]),
                Segment::Cubic([cx - k * rx, b], [x, cy + k * ry], [x, cy]),
                Segment::Cubic([x, cy - k * ry], [cx - k * rx, y], [cx, y]),
                Segment::Cubic([cx + k * rx, y], [r, cy - k * ry], [r, cy]),
                Segment::Close,
            ]
        }
    };
    Ok(Drawing {
        path: Arc::new(path),
        transform: [scale[0], 0., 0., scale[1], 0., 0.],
        fill: Some([1.; 4]),
        even_odd: false,
        stroke: None,
    })
}
