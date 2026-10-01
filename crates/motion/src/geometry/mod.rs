//! Native content independent of graph authoring and presentation.
pub mod appearance;
pub mod shapes;
pub mod text;
use fold_render::vector::{Drawing, MAX_DRAWINGS, Segment, Stroke};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub type Content = Arc<Vec<Element>>;
pub const IDENTITY: [f64; 6] = [1., 0., 0., 1., 0., 0.];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Domain {
    Points,
    Instances,
    Glyphs,
    Children,
}
#[derive(Clone, Debug)]
pub enum Geometry {
    Path(Arc<Vec<Segment>>),
    Glyph {
        path: Arc<Vec<Segment>>,
        cluster: u32,
    },
    Point,
    Instance(Content),
    Group(Content),
}
#[derive(Clone, Debug)]
pub struct Element {
    pub id: u64,
    pub geometry: Geometry,
    pub transform: [f64; 6],
    pub fill: Option<[f64; 4]>,
    pub stroke: Option<Stroke>,
    pub opacity: f64,
    pub even_odd: bool,
}
impl Element {
    pub fn new(id: u64, geometry: Geometry) -> Self {
        Self {
            id,
            geometry,
            transform: IDENTITY,
            fill: None,
            stroke: None,
            opacity: 1.,
            even_odd: false,
        }
    }
    pub fn matches(&self, domain: Domain) -> bool {
        match domain {
            Domain::Children => true,
            Domain::Points => matches!(self.geometry, Geometry::Point),
            Domain::Instances => matches!(self.geometry, Geometry::Instance(_)),
            Domain::Glyphs => matches!(self.geometry, Geometry::Glyph { .. }),
        }
    }
}
pub fn multiply(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}
pub fn transform(
    translation: [f64; 2],
    rotation: f64,
    scale: [f64; 2],
    pivot: [f64; 2],
) -> [f64; 6] {
    let (s, c) = rotation.to_radians().sin_cos();
    let a = c * scale[0];
    let b = s * scale[0];
    let x = -s * scale[1];
    let d = c * scale[1];
    [
        a,
        b,
        x,
        d,
        translation[0] + pivot[0] - a * pivot[0] - x * pivot[1],
        translation[1] + pivot[1] - b * pivot[0] - d * pivot[1],
    ]
}
/// Stable generated identity, independent of process hashing and playback order.
pub fn derived_id(source: u64, index: u64) -> u64 {
    let mut v = source ^ index.wrapping_add(0x9e3779b97f4a7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    v ^ (v >> 31)
}
pub fn drawings(content: &Content, scale: [f64; 2]) -> Result<Vec<Drawing>, String> {
    let mut result = Vec::new();
    fn walk(
        content: &Content,
        parent: [f64; 6],
        fill: Option<[f64; 4]>,
        stroke: Option<Stroke>,
        opacity: f64,
        depth: usize,
        result: &mut Vec<Drawing>,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("scene nesting exceeds 64".into());
        }
        for e in content.iter() {
            let matrix = multiply(parent, e.transform);
            let fill = fill.or(e.fill);
            let stroke = stroke.clone().or_else(|| e.stroke.clone());
            let opacity = opacity * e.opacity;
            match &e.geometry {
                Geometry::Path(path) | Geometry::Glyph { path, .. } => {
                    if result.len() >= MAX_DRAWINGS {
                        return Err("scene drawing budget exceeded".into());
                    }
                    result.push(Drawing {
                        path: path.clone(),
                        transform: matrix,
                        fill: Some({
                            let mut c = fill.unwrap_or([0., 0., 0., 1.]);
                            c[3] *= opacity;
                            c
                        }),
                        stroke: stroke.map(|mut s| {
                            s.color[3] *= opacity;
                            s
                        }),
                        even_odd: e.even_odd,
                    });
                }
                Geometry::Group(children) | Geometry::Instance(children) => {
                    walk(children, matrix, fill, stroke, opacity, depth + 1, result)?
                }
                Geometry::Point => return Err("points must be instanced before rendering".into()),
            }
        }
        Ok(())
    }
    walk(
        content,
        [scale[0], 0., 0., scale[1], 0., 0.],
        None,
        None,
        1.,
        0,
        &mut result,
    )?;
    fold_render::vector::validate(&result)?;
    Ok(result)
}
