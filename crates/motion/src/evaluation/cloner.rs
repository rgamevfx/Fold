//! Shared spatial prototypes placed on immutable upstream point streams.
use super::{Geometry, GeometryData, Scope, State, Value, distribution::Points};
use crate::types::{Space, Unit};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: u64,
    pub source_point: u64,
    pub position: [f64; 3],
    pub scale: f64,
    /// Rotation about the prototype origin, around local Z, in radians.
    pub rotation_z: f64,
    /// Optional instance-domain replacement over the collection material color.
    pub color: Option<super::style::Color>,
}
impl Instance {
    pub fn point(&self, point: [f64; 3]) -> [f64; 3] {
        let (sin, cos) = self.rotation_z.sin_cos();
        let rotated = [
            cos * point[0] - sin * point[1],
            sin * point[0] + cos * point[1],
            point[2],
        ];
        std::array::from_fn(|i| rotated[i] * self.scale + self.position[i])
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Instances {
    pub prototype: Arc<Geometry>,
    /// Original distribution provenance. Field consumers sample positions from
    /// their explicit upstream geometry, including any preceding Transform.
    pub distribution: Arc<Points>,
    pub instances: Vec<Instance>,
}
impl State<'_> {
    pub(super) fn cloner(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(prototype)) = args.get("prototype") else {
            return Err("Cloner requires prototype geometry".into());
        };
        let Some(Value::Points(points)) = args.get("points") else {
            return Err("Cloner requires a point stream".into());
        };
        if prototype.space != Space::World
            || prototype.unit != Unit::Meters
            || !matches!(
                prototype.data,
                GeometryData::Mesh(_) | GeometryData::Solids(_)
            )
        {
            return Err("Cloner requires a World metric mesh or text-solid prototype".into());
        }
        if points.space != Space::World || points.unit != Unit::Meters {
            return Err("Cloner distribution must use World meters".into());
        }
        self.reserve(1 + points.points.len())?;
        let mut instances = Vec::with_capacity(points.points.len());
        for point in &points.points {
            self.budget.step()?;
            instances.push(Instance {
                id: self.identity(scope, id, point.id, b"instance")?,
                source_point: point.id,
                position: point.position,
                scale: 1.,
                rotation_z: 0.,
                color: None,
            });
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            id: self.identity(scope, id, points.id, b"instances")?,
            space: Space::World,
            unit: Unit::Meters,
            appearance: prototype.appearance.clone(),
            data: GeometryData::Instances(Arc::new(Instances {
                prototype: prototype.clone(),
                distribution: points.clone(),
                instances,
            })),
        })))
    }
}
