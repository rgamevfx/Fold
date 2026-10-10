//! Editable roto belongs to Compositor. Evaluation lowers immutable paths to the
//! shared vector renderer; no Motion graph or renderer state is persisted here.
mod geometry;
mod properties;
mod render;
use fold_foundation::{ObjectId, Time};
use fold_project::Metadata;
pub use geometry::{Cubic, add, map, mul, sub};
pub(crate) use render::compile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spline {
    #[default]
    Bezier,
    BSpline,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Combine {
    #[default]
    Over,
    Add,
    Subtract,
    Intersect,
    Replace,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub id: ObjectId,
    pub position: [f64; 2],
    /// Relative tangent vectors. Feather tangent vectors follow these tangents.
    pub incoming: [f64; 2],
    pub outgoing: [f64; 2],
    /// Offset from the automatically offset feather point, in shape pixels.
    pub feather: [f64; 2],
    pub linked: bool,
    #[serde(default)]
    pub extensions: Metadata,
}
impl Point {
    pub fn new(position: [f64; 2]) -> Self {
        Self {
            id: ObjectId::new(),
            position,
            incoming: [0.; 2],
            outgoing: [0.; 2],
            feather: [0.; 2],
            linked: true,
            extensions: Metadata::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub spline: Spline,
    pub points: Vec<Point>,
    pub closed: bool,
    pub feather: f64,
    pub falloff: f64,
    #[serde(default)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: ObjectId,
    pub name: String,
    pub enabled: bool,
    pub locked: bool,
    pub opacity: f64,
    pub invert: bool,
    pub combine: Combine,
    pub translate: [f64; 2],
    pub scale: [f64; 2],
    pub rotate: f64,
    pub center: [f64; 2],
    /// Inclusive start and exclusive end, in exact document time.
    pub lifetime: Option<[Time; 2]>,
    /// None is an isolated group; Some is a shape and cannot have children.
    pub shape: Option<Shape>,
    pub children: Vec<Layer>,
    #[serde(default)]
    pub extensions: Metadata,
}
impl Layer {
    pub fn group(name: &str) -> Self {
        Self {
            id: ObjectId::new(),
            name: name.into(),
            enabled: true,
            locked: false,
            opacity: 1.,
            invert: false,
            combine: Combine::Over,
            translate: [0.; 2],
            scale: [1.; 2],
            rotate: 0.,
            center: [0.; 2],
            lifetime: None,
            shape: None,
            children: vec![],
            extensions: Metadata::new(),
        }
    }
    pub fn curve(spline: Spline, points: Vec<Point>) -> Self {
        let mut layer = Self::group(if spline == Spline::Bezier {
            "Bezier"
        } else {
            "B-spline"
        });
        layer.shape = Some(Shape {
            spline,
            points,
            closed: true,
            feather: 0.,
            falloff: 1.,
            extensions: Metadata::new(),
        });
        layer
    }
    pub fn find(&self, id: ObjectId) -> Option<&Self> {
        if self.id == id {
            Some(self)
        } else {
            self.children.iter().find_map(|v| v.find(id))
        }
    }
    pub fn find_mut(&mut self, id: ObjectId) -> Option<&mut Self> {
        if self.id == id {
            Some(self)
        } else {
            self.children.iter_mut().find_map(|v| v.find_mut(id))
        }
    }
    pub fn editable(&self, id: ObjectId) -> bool {
        !self.locked && (self.id == id || self.children.iter().any(|v| v.editable(id)))
    }
    pub fn matrix(&self) -> fold_render::Affine {
        let (s, c) = self.rotate.to_radians().sin_cos();
        let (a, b, c, d) = (
            c * self.scale[0],
            s * self.scale[0],
            -s * self.scale[1],
            c * self.scale[1],
        );
        fold_render::Affine {
            a,
            b,
            c,
            d,
            tx: self.center[0] + self.translate[0] - a * self.center[0] - c * self.center[1],
            ty: self.center[1] + self.translate[1] - b * self.center[0] - d * self.center[1],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Roto {
    pub root: Layer,
    pub channels: Vec<fold_render::channels::ChannelName>,
    pub clip_to_format: bool,
    pub premultiply: bool,
    pub motion_samples: u32,
    pub shutter: Time,
    pub shutter_offset: crate::transform::ShutterOffset,
    #[serde(default)]
    pub extensions: Metadata,
}
impl Default for Roto {
    fn default() -> Self {
        Self {
            root: Layer::group("Root"),
            channels: vec!["rgba.alpha".to_owned().try_into().unwrap()],
            clip_to_format: true,
            premultiply: false,
            motion_samples: 0,
            shutter: Time::new(1, 2).unwrap(),
            shutter_offset: crate::transform::ShutterOffset::Center,
            extensions: Metadata::new(),
        }
    }
}
impl Roto {
    pub fn validate(&self) -> Result<(), String> {
        fn layer(
            l: &Layer,
            depth: usize,
            ids: &mut BTreeSet<ObjectId>,
            count: &mut usize,
        ) -> Result<(), String> {
            *count += 1;
            if depth > 16 || *count > 256 || !ids.insert(l.id) {
                return Err(
                    "Roto requires unique identities, at most 256 layers and 16 group levels"
                        .into(),
                );
            }
            if l.name.len() > 256
                || !l.opacity.is_finite()
                || !(0. ..=1.).contains(&l.opacity)
                || l.translate
                    .iter()
                    .chain(&l.scale)
                    .chain(&l.center)
                    .chain([&l.rotate])
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
                || l.scale.iter().any(|v| v.abs() < 1e-6)
                || l.lifetime.is_some_and(|[a, b]| a >= b)
            {
                return Err("Invalid Roto layer controls or lifetime".into());
            }
            if let Some(shape) = &l.shape {
                if !l.children.is_empty()
                    || shape.points.len() > 4096
                    || !shape.feather.is_finite()
                    || !(-10000. ..=10000.).contains(&shape.feather)
                    || !shape.falloff.is_finite()
                    || !(0.01..=100.).contains(&shape.falloff)
                {
                    return Err("Invalid Roto shape or feather".into());
                }
                for p in &shape.points {
                    if !ids.insert(p.id)
                        || p.position
                            .iter()
                            .chain(&p.incoming)
                            .chain(&p.outgoing)
                            .chain(&p.feather)
                            .any(|v| !v.is_finite() || v.abs() > 1e6)
                    {
                        return Err("Roto points require unique IDs and finite coordinates".into());
                    }
                }
            }
            for child in &l.children {
                layer(child, depth + 1, ids, count)?;
            }
            Ok(())
        }
        layer(&self.root, 0, &mut BTreeSet::new(), &mut 0)?;
        if !self.clip_to_format {
            return Err("Roto requires Clip to format with the current compositor raster".into());
        }
        if self.channels.len() > 256
            || self.motion_samples > 64
            || self.shutter < Time::ZERO
            || self.shutter > Time::new(2, 1).unwrap()
        {
            return Err("Invalid Roto channels or shutter".into());
        }
        Ok(())
    }
    pub fn sample_times(&self, time: Time, rate: [u32; 2]) -> Result<Vec<Time>, String> {
        crate::Transform {
            motion_samples: self.motion_samples,
            shutter: self.shutter,
            shutter_offset: self.shutter_offset,
            ..Default::default()
        }
        .sample_times(time, rate)
    }
    pub fn transform_for(&self, id: ObjectId) -> Option<fold_render::Affine> {
        fn find(
            l: &Layer,
            id: ObjectId,
            parent: fold_render::Affine,
        ) -> Option<fold_render::Affine> {
            let m = geometry::compose(parent, l.matrix());
            if l.id == id {
                Some(m)
            } else {
                l.children.iter().find_map(|c| find(c, id, m))
            }
        }
        find(&self.root, id, fold_render::Affine::IDENTITY)
    }
}
