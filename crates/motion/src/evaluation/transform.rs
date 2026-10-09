//! Native geometry transforms. Element identities survive; the output geometry
//! identity names this transformed branch. Positive scales preserve winding.
//! Screen transforms change path coordinates; Stroke widths stay in authored canvas units.
use super::{Geometry, GeometryData, Scope, State, Value, placement::World};
use crate::{
    registry::Builtin,
    types::{Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};

impl State<'_> {
    pub(super) fn transform_geometry(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
        mode: Builtin,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(source)) = args.get("geometry") else {
            return Err("Transform requires geometry".into());
        };
        let screen = mode == Builtin::TransformScreen;
        let (space, unit, dimensions) = if screen {
            (Space::Screen, Unit::Canvas, 2)
        } else {
            (Space::World, Unit::Meters, 3)
        };
        if source.space != space || source.unit != unit {
            return Err("Transform space must match its geometry".into());
        }
        let primitive = |name| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let vector = |name, expected| -> Result<[f64; 3], String> {
            let Literal::Vector {
                value,
                unit,
                space: actual,
            } = primitive(name)?
            else {
                return Err(format!("Expected {name} vector"));
            };
            if *unit != expected
                || *actual != space
                || value.len() != dimensions
                || value.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
            {
                return Err(format!("Invalid Transform {name}"));
            }
            Ok([
                value[0],
                value[1],
                if dimensions == 3 { value[2] } else { 0. },
            ])
        };
        let translation = vector("translation", unit)?;
        let pivot = vector("pivot", unit)?;
        let scale = if mode == Builtin::TransformInstances {
            let Literal::Scalar { value } = primitive("scale")? else {
                return Err("Expected uniform scale".into());
            };
            [*value; 3]
        } else {
            let mut scale = vector("scale", Unit::Unitless)?;
            if screen {
                scale[2] = 1.;
            }
            scale
        };
        if scale
            .iter()
            .any(|v| !v.is_finite() || !(1e-6..=1e6).contains(v))
        {
            return Err("Transform scale must be in [1e-6,1e6]".into());
        }
        let mut angle = 0.;
        let rotation = if mode == Builtin::TransformMesh {
            let Literal::Rotation {
                value,
                space: Space::World,
            } = primitive("rotation")?
            else {
                return Err("Expected World rotation".into());
            };
            primitive("rotation")?.validate()?;
            *value
        } else {
            let Literal::Scalar { value } = primitive("angle")? else {
                return Err("Expected rotation angle".into());
            };
            if !value.is_finite() || value.abs() > 1e6 {
                return Err("Transform angle exceeds supported range".into());
            }
            angle = *value;
            let (s, c) = (angle * 0.5).sin_cos();
            [0., 0., s, c]
        };
        let mut transform = World {
            affine: None,
            position: [0.; 3],
            rotation,
            scale,
        };
        let mapped_pivot = transform.point(pivot);
        transform.position = std::array::from_fn(|i| translation[i] + pivot[i] - mapped_pivot[i]);
        let data = match (&source.data, mode) {
            (GeometryData::Contours(contours), Builtin::TransformScreen) => {
                self.reserve(contours.len() + contours.iter().map(|c| c.points.len()).sum::<usize>())?;
                let mut contours = contours.as_ref().clone();
                for contour in &mut contours {
                    for point in &mut contour.points {
                        self.budget.step()?;
                        let value = transform.point([point.position[0], point.position[1], 0.]);
                        if value.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
                            return Err("Transformed contour exceeds supported extent".into());
                        }
                        point.position = [value[0], value[1]];
                    }
                }
                GeometryData::Contours(Arc::new(contours))
            }
            (GeometryData::Outlines(outlines), Builtin::TransformScreen) => {
                let mut outlines = outlines.as_ref().clone();
                self.reserve(outlines.curves.glyphs.len())?;
                let mut curves = outlines.curves.as_ref().clone();
                for glyph in &mut curves.glyphs {
                    self.reserve(glyph.path.len())?;
                    let [a, b, c, d, x, y] = glyph.transform;
                    let point = |p: [f64; 2]| -> Result<[f64; 2], String> {
                        let value = transform.point([a * p[0] + c * p[1] + x, b * p[0] + d * p[1] + y, 0.]);
                        if value.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
                            return Err("Transformed outline exceeds supported extent".into());
                        }
                        Ok([value[0], value[1]])
                    };
                    let mut path = Vec::with_capacity(glyph.path.len());
                    for segment in glyph.path.iter() {
                        self.budget.step()?;
                        use fold_render::vector::Segment;
                        path.push(match *segment {
                            Segment::Move(p) => Segment::Move(point(p)?),
                            Segment::Line(p) => Segment::Line(point(p)?),
                            Segment::Quad(p, q) => Segment::Quad(point(p)?, point(q)?),
                            Segment::Cubic(p, q, r) => Segment::Cubic(point(p)?, point(q)?, point(r)?),
                            Segment::Close => Segment::Close,
                        });
                    }
                    glyph.path = Arc::new(path);
                    glyph.transform = [1., 0., 0., 1., 0., 0.];
                }
                outlines.curves = Arc::new(curves);
                GeometryData::Outlines(Arc::new(outlines))
            }
            (GeometryData::Mesh(mesh), Builtin::TransformMesh) => {
                self.reserve(mesh.prepared.mesh.positions.len())?;
                let mut mesh = mesh.as_ref().clone();
                let mut prepared = mesh.prepared.mesh.as_ref().clone();
                let normal_transform = World { affine: None, position: [0.; 3], rotation, scale: scale.map(|s| 1. / s) };
                for (point, normal) in prepared.positions.iter_mut().zip(&mut prepared.normals) {
                    self.budget.step()?;
                    *point = transform.point(point.map(f64::from)).map(|v| v as f32);
                    let n = normal_transform.point(normal.map(f64::from));
                    let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
                    *normal = n.map(|v| (v / length) as f32);
                    if point.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
                        return Err("Transformed mesh exceeds supported extent".into());
                    }
                }
                mesh.prepared.mesh = Arc::new(prepared);
                GeometryData::Mesh(Arc::new(mesh))
            }
            (GeometryData::Instances(collection), Builtin::TransformInstances) => {
                self.reserve(collection.instances.len())?;
                let mut collection = collection.as_ref().clone();
                for instance in &mut collection.instances {
                    self.budget.step()?;
                    instance.position = transform.point(instance.position);
                    instance.scale *= scale[0];
                    instance.rotation_z = (instance.rotation_z + angle).rem_euclid(std::f64::consts::TAU);
                    if instance.position.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
                        || !(1e-6..=1e6).contains(&instance.scale)
                    {
                        return Err("Transformed instance exceeds supported extent".into());
                    }
                }
                GeometryData::Instances(Arc::new(collection))
            }
            _ => return Err("Transform mode requires contours/Outline Text, a metric mesh, or instances respectively".into()),
        };
        Ok(Value::Geometry(Arc::new(Geometry {
            id: self.identity(scope, id, source.id, b"transform")?,
            space,
            unit,
            data,
            appearance: source.appearance.clone(),
        })))
    }
}
