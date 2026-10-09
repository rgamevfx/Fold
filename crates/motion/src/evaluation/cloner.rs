//! Shared spatial prototypes placed on immutable upstream point streams.
use super::{Geometry, GeometryData, Scope, State, Value};
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
    pub orient: [f64; 4],
    /// Optional instance-domain replacement over the collection material color.
    pub color: Option<super::style::Color>,
}
impl Instance {
    pub fn point(&self, point: [f64; 3]) -> [f64; 3] {
        let point = super::placement::World {
            affine: None,
            position: [0.; 3],
            rotation: self.orient,
            scale: [1.; 3],
        }
        .point(point);
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
    pub distribution: Arc<Geometry>,
    pub instances: Vec<Instance>,
}
/// Account for owned attribute values copied to the instance domain, including strings.
pub(super) fn attribute_cost(points: &Geometry, names: &str) -> usize {
    names
        .split_whitespace()
        .filter_map(|name| {
            points
                .attributes
                .get(super::attributes::Domain::Point, name)
        })
        .flat_map(|a| &a.values)
        .fold(0usize, |cost, value| {
            cost.saturating_add(super::attributes::value_bytes(value).div_ceil(128))
        })
}
/// Native nodes and Wrangle use the same copy semantics and shared prototype storage.
pub fn copy(
    prototype: Arc<Geometry>,
    points: Arc<Geometry>,
    names: &str,
    seed: u64,
    step: &mut impl FnMut() -> Result<(), String>,
) -> Result<Geometry, String> {
    use super::attributes::{Domain, stable_id};
    use crate::types::Literal;
    if prototype.space != Space::World
        || prototype.unit != Unit::Meters
        || !matches!(
            prototype.data,
            GeometryData::Mesh(_) | GeometryData::Solids(_)
        )
    {
        return Err("Copy to Points requires a World metric mesh or text-solid prototype".into());
    }
    if points.space != Space::World || points.unit != Unit::Meters {
        return Err("Copy targets must use World meters".into());
    }
    let count = points.count(Domain::Point)?;
    let mut attributes = super::attributes::Attributes::default();
    for name in names.split_whitespace() {
        if let Some(a) = points.attributes.get(Domain::Point, name) {
            attributes.put(
                Domain::Instance,
                name,
                a.value_type.clone(),
                a.values.clone(),
            );
        }
    }
    let mut instances = Vec::with_capacity(count);
    for index in 0..count {
        step()?;
        let point = points.element_id(Domain::Point, index)?;
        let orient = match points.attributes.get(Domain::Point, "orient") {
            Some(a) => match a.values[index] {
                Literal::Rotation { value, .. } => value,
                _ => return Err("orient requires quaternion".into()),
            },
            None => [0., 0., 0., 1.],
        };
        let scale = match points.attributes.get(Domain::Point, "pscale") {
            Some(a) => match a.values[index] {
                Literal::Scalar { value } if value.is_finite() && (1e-6..=1e6).contains(&value) => {
                    value
                }
                _ => return Err("pscale requires a positive scalar".into()),
            },
            None => 1.,
        };
        instances.push(Instance {
            id: stable_id(seed, point),
            source_point: point,
            position: points.position(Domain::Point, index)?,
            scale,
            rotation_z: 0.,
            orient,
            color: None,
        });
    }
    Ok(Geometry {
        attributes,
        id: seed,
        space: Space::World,
        unit: Unit::Meters,
        appearance: prototype.appearance.clone(),
        data: GeometryData::Instances(Arc::new(Instances {
            prototype,
            distribution: points,
            instances,
        })),
    })
}
impl State<'_> {
    pub(super) fn cloner(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(prototype)) = args.get("prototype") else {
            return Err("Copy to Points requires prototype geometry".into());
        };
        let Some(Value::Geometry(points)) = args.get("points") else {
            return Err("Copy to Points requires point-bearing geometry".into());
        };
        let names = super::attribute_nodes::text(args, "attributes")?;
        self.reserve(
            1usize
                .saturating_add(points.count(super::attributes::Domain::Point)?)
                .saturating_add(attribute_cost(points, names)),
        )?;
        let seed = self.identity(scope, id, points.id, b"instances")?;
        Ok(Value::Geometry(Arc::new(copy(
            prototype.clone(),
            points.clone(),
            names,
            seed,
            &mut || self.budget.step(),
        )?)))
    }
}
