//! Bounded arc-length sampling and typed attributes on one authored path.
use super::{Content, Geometry, IDENTITY, multiply};
use crate::fields::Datum;
use fold_render::vector::Segment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub position: f64,
    pub value: Datum,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
pub type Attributes = BTreeMap<String, Vec<Stop>>;
pub fn validate(attributes: &Attributes) -> Result<(), String> {
    if attributes.len() > 64 {
        return Err("path supports at most 64 named attributes".into());
    }
    for (name, stops) in attributes {
        if name.is_empty() || name.len() > 256 || stops.is_empty() || stops.len() > 1024 {
            return Err("attributes need a name and 1..1024 samples".into());
        }
        for (i, stop) in stops.iter().enumerate() {
            stop.value.validate()?;
            if !(0. ..=1.).contains(&stop.position)
                || stop.value.kind() != stops[0].value.kind()
                || (i > 0 && stops[i - 1].position >= stop.position)
            {
                return Err(
                    "attribute samples must share a type and have increasing positions in 0..1"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
pub fn attribute(stops: &[Stop], u: f64) -> Result<Datum, String> {
    let first = stops.first().ok_or("empty attribute")?;
    if u <= first.position {
        return Ok(first.value.clone());
    }
    for pair in stops.windows(2) {
        if u <= pair[1].position {
            let t = (u - pair[0].position) / (pair[1].position - pair[0].position);
            return match pair[0].value {
                Datum::Scalar(_) | Datum::Vector(_) | Datum::Color(_) => {
                    pair[0].value.mix(&pair[1].value, t)
                }
                _ => Ok(if t < 1. {
                    &pair[0].value
                } else {
                    &pair[1].value
                }
                .clone()),
            };
        }
    }
    Ok(stops.last().unwrap().value.clone())
}
pub struct Path {
    edges: Vec<([f64; 2], [f64; 2], f64)>,
    length: f64,
    pub attributes: Attributes,
}
impl Path {
    pub fn from_content(content: &Content) -> Result<Self, String> {
        let mut found = vec![];
        fn walk(
            content: &Content,
            parent: [f64; 6],
            attrs: &Attributes,
            found: &mut Vec<(Vec<Segment>, [f64; 6], Attributes)>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 64 {
                return Err("path nesting limit".into());
            }
            for e in content.iter() {
                let matrix = multiply(parent, e.transform);
                let mut attrs = attrs.clone();
                attrs.extend(e.path_attributes.clone());
                match &e.geometry {
                    Geometry::Path(p) => found.push((p.as_ref().clone(), matrix, attrs)),
                    Geometry::Group(c) | Geometry::Instance(c) => {
                        walk(c, matrix, &attrs, found, depth + 1)?
                    }
                    _ => return Err("path target must contain a path".into()),
                }
                if found.len() > 1 {
                    return Err("path target must contain exactly one path".into());
                }
            }
            Ok(())
        }
        walk(content, IDENTITY, &Attributes::new(), &mut found, 0)?;
        let (segments, matrix, attributes) = found.pop().ok_or("empty path")?;
        validate(&attributes)?;
        let point = |p: [f64; 2]| {
            [
                matrix[0] * p[0] + matrix[2] * p[1] + matrix[4],
                matrix[1] * p[0] + matrix[3] * p[1] + matrix[5],
            ]
        };
        let mut result = Self {
            edges: vec![],
            length: 0.,
            attributes,
        };
        let mut current = [0.; 2];
        let mut start = current;
        let mut moved = false;
        for segment in segments {
            match segment {
                Segment::Move(p) => {
                    if moved {
                        return Err("path motion requires one continuous contour".into());
                    }
                    moved = true;
                    current = p;
                    start = p;
                }
                Segment::Line(p) => {
                    result.edge(point(current), point(p))?;
                    current = p;
                }
                Segment::Close => {
                    result.edge(point(current), point(start))?;
                    current = start;
                }
                Segment::Quad(a, b) => {
                    let from = current;
                    for i in 1..=64 {
                        let t = i as f64 / 64.;
                        let q = std::array::from_fn(|j| {
                            (1. - t).powi(2) * from[j] + 2. * (1. - t) * t * a[j] + t * t * b[j]
                        });
                        result.edge(point(current), point(q))?;
                        current = q;
                    }
                }
                Segment::Cubic(a, b, c) => {
                    let from = current;
                    for i in 1..=96 {
                        let t = i as f64 / 96.;
                        let q = std::array::from_fn(|j| {
                            (1. - t).powi(3) * from[j]
                                + 3. * (1. - t).powi(2) * t * a[j]
                                + 3. * (1. - t) * t * t * b[j]
                                + t.powi(3) * c[j]
                        });
                        result.edge(point(current), point(q))?;
                        current = q;
                    }
                }
            }
        }
        if result.length <= 1e-9 {
            return Err("path has no length".into());
        }
        Ok(result)
    }
    fn edge(&mut self, a: [f64; 2], b: [f64; 2]) -> Result<(), String> {
        if self.edges.len() >= 16384 {
            return Err("path sampling budget exceeded".into());
        }
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length > 1e-9 {
            self.length += length;
            self.edges.push((a, b, self.length));
        }
        Ok(())
    }
    pub fn sample(&self, u: f64) -> ([f64; 2], f64) {
        let distance = u.clamp(0., 1.) * self.length;
        let index = self
            .edges
            .partition_point(|e| e.2 < distance)
            .min(self.edges.len() - 1);
        let (a, b, end) = self.edges[index];
        let start = if index == 0 {
            0.
        } else {
            self.edges[index - 1].2
        };
        let t = (distance - start) / (end - start);
        (
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees(),
        )
    }
}
