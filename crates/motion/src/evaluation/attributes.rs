//! Typed domain attributes shared by native nodes and the embedded Wrangle.
//! P and N address mesh storage directly; there is no second position authority.
use super::{Geometry, GeometryData};
use crate::types::{Encoding, Literal, Space, Type, Unit};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Point,
    Primitive,
    Corner,
    Edge,
    Detail,
    Instance,
}
impl Domain {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "point" => Ok(Self::Point),
            "primitive" => Ok(Self::Primitive),
            "corner" => Ok(Self::Corner),
            "edge" => Ok(Self::Edge),
            "detail" => Ok(Self::Detail),
            "instance" => Ok(Self::Instance),
            _ => Err(format!("Unknown attribute domain {name}")),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Attribute {
    pub domain: Domain,
    pub name: String,
    pub value_type: Type,
    pub encoding: Option<Encoding>,
    pub values: Vec<Literal>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Attributes(pub Arc<Vec<Attribute>>);
impl Attributes {
    pub fn get(&self, domain: Domain, name: &str) -> Option<&Attribute> {
        self.0.iter().find(|a| a.domain == domain && a.name == name)
    }
    pub fn put(&mut self, domain: Domain, name: &str, value_type: Type, values: Vec<Literal>) {
        if let Some(a) = Arc::make_mut(&mut self.0)
            .iter_mut()
            .find(|a| a.domain == domain && a.name == name)
        {
            a.value_type = value_type;
            a.values = values;
        } else {
            Arc::make_mut(&mut self.0).push(Attribute {
                domain,
                name: name.into(),
                value_type,
                encoding: (name == "Cd").then_some(Encoding::LinearRec709),
                values,
            });
        }
    }
}
pub fn vector(v: [f64; 3], unit: Unit) -> Literal {
    Literal::Vector {
        value: v.to_vec(),
        unit,
        space: Space::World,
    }
}
pub fn xyz(v: &Literal) -> Result<[f64; 3], String> {
    match v {
        Literal::Vector { value, .. } if value.len() == 3 => Ok([value[0], value[1], value[2]]),
        _ => Err("Expected a three-component vector".into()),
    }
}
impl Geometry {
    pub fn edges(&self) -> Result<Vec<[u32; 2]>, String> {
        let GeometryData::Mesh(mesh) = &self.data else {
            return Err("Edge domain requires a mesh".into());
        };
        let mut edges = std::collections::BTreeSet::new();
        for face in mesh.prepared.mesh.indices.chunks_exact(3) {
            for [a, b] in [[face[0], face[1]], [face[1], face[2]], [face[2], face[0]]] {
                edges.insert([a.min(b), a.max(b)]);
            }
        }
        Ok(edges.into_iter().collect())
    }
    pub fn count(&self, domain: Domain) -> Result<usize, String> {
        if domain == Domain::Detail {
            return Ok(1);
        }
        match (&self.data, domain) {
            (GeometryData::Mesh(m), Domain::Point) => Ok(m.vertex_ids.len()),
            (GeometryData::Mesh(m), Domain::Primitive) => Ok(m.face_ids.len()),
            (GeometryData::Mesh(m), Domain::Corner) => Ok(m.prepared.mesh.indices.len()),
            (GeometryData::Mesh(_), Domain::Edge) => Ok(self.edges()?.len()),
            (GeometryData::Instances(m), Domain::Instance) => Ok(m.instances.len()),
            _ => Err(format!("Geometry does not expose {domain:?} elements")),
        }
    }
    pub fn element_id(&self, domain: Domain, index: usize) -> Result<u64, String> {
        if index >= self.count(domain)? {
            return Err("Element index out of range".into());
        }
        match (&self.data, domain) {
            (_, Domain::Detail) => Ok(self.id),
            (GeometryData::Mesh(m), Domain::Point) => Ok(m.vertex_ids[index]),
            (GeometryData::Mesh(m), Domain::Primitive) => Ok(m.face_ids[index]),
            (GeometryData::Mesh(m), Domain::Corner) => {
                Ok(stable_id(m.face_ids[index / 3], (index % 3) as u64))
            }
            (GeometryData::Mesh(m), Domain::Edge) => {
                let [a, b] = self.edges()?[index];
                let a = m.vertex_ids[a as usize];
                let b = m.vertex_ids[b as usize];
                Ok(stable_id(a.min(b), a.max(b)))
            }
            (GeometryData::Instances(m), Domain::Instance) => Ok(m.instances[index].id),
            _ => Err("Unsupported element identity".into()),
        }
    }
    pub fn position(&self, domain: Domain, index: usize) -> Result<[f64; 3], String> {
        if index >= self.count(domain)? {
            return Err("Element index out of range".into());
        }
        match (&self.data, domain) {
            (GeometryData::Mesh(m), Domain::Point) => {
                Ok(m.prepared.mesh.positions[index].map(f64::from))
            }
            (GeometryData::Mesh(m), Domain::Corner) => Ok(m.prepared.mesh.positions
                [m.prepared.mesh.indices[index] as usize]
                .map(f64::from)),
            (GeometryData::Mesh(m), Domain::Primitive) => {
                let f = &m.prepared.mesh.indices[index * 3..index * 3 + 3];
                Ok(std::array::from_fn(|axis| {
                    f.iter()
                        .map(|p| m.prepared.mesh.positions[*p as usize][axis] as f64)
                        .sum::<f64>()
                        / 3.
                }))
            }
            (GeometryData::Mesh(m), Domain::Edge) => {
                let e = self.edges()?[index];
                Ok(std::array::from_fn(|axis| {
                    (m.prepared.mesh.positions[e[0] as usize][axis] as f64
                        + m.prepared.mesh.positions[e[1] as usize][axis] as f64)
                        * 0.5
                }))
            }
            (GeometryData::Instances(m), Domain::Instance) => Ok(m.instances[index].position),
            _ => Err("Domain has no sampling position".into()),
        }
    }
    pub fn read_attribute(
        &self,
        domain: Domain,
        name: &str,
        index: usize,
    ) -> Result<Literal, String> {
        if index >= self.count(domain)? {
            return Err("Attribute index out of range".into());
        }
        if name == "P" && matches!(domain, Domain::Point | Domain::Instance) {
            return Ok(vector(self.position(domain, index)?, self.unit));
        }
        if name == "N"
            && domain == Domain::Point
            && let GeometryData::Mesh(m) = &self.data
        {
            return Ok(vector(
                m.prepared.mesh.normals[index].map(f64::from),
                Unit::Unitless,
            ));
        }
        if domain == Domain::Instance
            && let GeometryData::Instances(c) = &self.data
        {
            if name == "orient" {
                return Ok(Literal::Rotation {
                    value: c.instances[index].orient,
                    space: Space::World,
                });
            }
            if name == "pscale" {
                return Ok(Literal::Scalar {
                    value: c.instances[index].scale,
                });
            }
        }
        self.attributes
            .get(domain, name)
            .and_then(|a| a.values.get(index))
            .cloned()
            .ok_or_else(|| format!("Missing {domain:?} attribute {name}"))
    }
    pub fn set_attribute(
        &mut self,
        domain: Domain,
        name: &str,
        values: Vec<Literal>,
    ) -> Result<(), String> {
        let ty = values
            .first()
            .map(Literal::ty)
            .or_else(|| {
                self.attributes
                    .get(domain, name)
                    .map(|a| a.value_type.clone())
            })
            .ok_or("Empty attributes require an explicit type")?;
        self.set_attribute_typed(domain, name, ty, values)
    }
    pub fn set_attribute_typed(
        &mut self,
        domain: Domain,
        name: &str,
        ty: Type,
        values: Vec<Literal>,
    ) -> Result<(), String> {
        if values.len() != self.count(domain)? {
            return Err("Attribute element count mismatch".into());
        }
        ty.validate()?;
        if self
            .attributes
            .get(domain, name)
            .is_some_and(|a| a.value_type != ty)
        {
            return Err(format!("Attribute {name} type conflict"));
        }
        for value in &values {
            validate_value(name, value, self.unit)?;
            if value.ty() != ty {
                return Err("Attribute values must have one type".into());
            }
        }
        if values.is_empty() {
            self.attributes.put(domain, name, ty, values);
            return Ok(());
        }
        let intrinsic = (domain == Domain::Point && matches!(name, "P" | "N"))
            || (domain == Domain::Instance && matches!(name, "P" | "orient" | "pscale"));
        if !intrinsic {
            self.attributes.put(domain, name, ty, values);
            return Ok(());
        }
        // Validate the whole batch before changing any output storage.
        for (index, value) in values.into_iter().enumerate() {
            self.set_attribute_element(domain, name, index, value)?
        }
        Ok(())
    }
    pub fn set_attribute_element(
        &mut self,
        domain: Domain,
        name: &str,
        index: usize,
        value: Literal,
    ) -> Result<(), String> {
        validate_value(name, &value, self.unit)?;
        let count = self.count(domain)?;
        if index >= count {
            return Err("Attribute index out of range".into());
        }
        if self
            .attributes
            .get(domain, name)
            .is_some_and(|a| a.value_type != value.ty())
        {
            return Err(format!("Attribute {name} type conflict"));
        }
        if matches!(name, "P" | "N") && domain == Domain::Point {
            let GeometryData::Mesh(m) = &mut self.data else {
                return Err("Point attribute requires a mesh".into());
            };
            let m = Arc::make_mut(&mut Arc::make_mut(m).prepared.mesh);
            let mut v = xyz(&value)?;
            if name == "N" {
                let length = v.iter().map(|v| v * v).sum::<f64>().sqrt();
                v = v.map(|v| v / length);
                m.normals[index] = v.map(|v| v as f32);
            } else {
                m.positions[index] = v.map(|v| v as f32);
            }
            return Ok(());
        }
        if domain == Domain::Instance {
            let GeometryData::Instances(c) = &mut self.data else {
                return Err("Expected instances".into());
            };
            let instance = &mut Arc::make_mut(c).instances[index];
            match (&value, name) {
                (_, "P") => {
                    instance.position = xyz(&value)?;
                    return Ok(());
                }
                (Literal::Rotation { value, .. }, "orient") => {
                    instance.orient = *value;
                    return Ok(());
                }
                (Literal::Scalar { value }, "pscale") => {
                    instance.scale = *value;
                    return Ok(());
                }
                _ => {}
            }
        }
        if self.attributes.get(domain, name).is_none() {
            if self.attributes.0.len() >= 256 {
                return Err("Geometry supports at most 256 stored attributes".into());
            }
            self.attributes.put(
                domain,
                name,
                value.ty(),
                vec![default_for(name, &value.ty())?; count],
            );
        }
        let a = Arc::make_mut(&mut self.attributes.0)
            .iter_mut()
            .find(|a| a.domain == domain && a.name == name)
            .expect("inserted attribute");
        a.values[index] = value;
        Ok(())
    }
}
pub fn validate_value(name: &str, value: &Literal, unit: Unit) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 {
        return Err("Attribute names require 1–128 bytes".into());
    }
    value.validate()?;
    match name {
        "orient"
            if !matches!(
                value,
                Literal::Rotation {
                    space: Space::World,
                    ..
                }
            ) =>
        {
            return Err("orient requires a World quaternion".into());
        }
        "pscale" if !matches!(value,Literal::Scalar{value} if (1e-6..=1e6).contains(value)) => {
            return Err("pscale requires a positive scalar in [1e-6,1e6]".into());
        }
        "P" | "N" | "Cd" => {
            let v = xyz(value)?;
            let expected = if name == "P" { unit } else { Unit::Unitless };
            if !matches!(value,Literal::Vector{unit,space:Space::World,..} if *unit==expected) {
                return Err(format!("Invalid {name} units/space"));
            }
            if v.iter().any(|x| !x.is_finite() || x.abs() > 1e6) {
                return Err("Attribute exceeds finite supported extent".into());
            }
            if name == "Cd" && v.iter().any(|x| !(0. ..=65504.).contains(x)) {
                return Err("Cd requires nonnegative linear Rec.709 RGB".into());
            }
            if name == "N" && v.iter().map(|x| x * x).sum::<f64>() < 1e-24 {
                return Err("Normal must be nonzero".into());
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn stable_id(parent: u64, slot: u64) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"fold.geometry.element.v1");
    h.update(parent.to_le_bytes());
    h.update(slot.to_le_bytes());
    u64::from_le_bytes(h.finalize()[..8].try_into().expect("eight bytes"))
}

/// Conservative storage accounting, including owned strings and vector arrays.
pub fn value_bytes(value: &Literal) -> usize {
    std::mem::size_of::<Literal>()
        + match value {
            Literal::Text { value } => value.len(),
            Literal::Vector { value, .. } => value.len() * 8,
            _ => 0,
        }
}
pub fn check_storage(count: usize, value: &Literal) -> Result<(), String> {
    if count.saturating_mul(value_bytes(value)) > 64 * 1024 * 1024 {
        return Err("Attribute storage exceeds 64 MiB".into());
    }
    Ok(())
}

impl Geometry {
    pub fn element_ids(&self, domain: Domain) -> Result<Vec<u64>, String> {
        match (&self.data, domain) {
            (GeometryData::Mesh(m), Domain::Point) => Ok(m.vertex_ids.clone()),
            (GeometryData::Mesh(m), Domain::Primitive) => Ok(m.face_ids.clone()),
            (GeometryData::Mesh(m), Domain::Corner) => Ok(m
                .face_ids
                .iter()
                .flat_map(|id| (0..3).map(move |corner| stable_id(*id, corner)))
                .collect()),
            (GeometryData::Mesh(m), Domain::Edge) => Ok(self
                .edges()?
                .into_iter()
                .map(|[a, b]| {
                    let a = m.vertex_ids[a as usize];
                    let b = m.vertex_ids[b as usize];
                    stable_id(a.min(b), a.max(b))
                })
                .collect()),
            (_, Domain::Detail) => Ok(vec![self.id]),
            (GeometryData::Instances(c), Domain::Instance) => {
                Ok(c.instances.iter().map(|i| i.id).collect())
            }
            _ => Err("Unsupported attribute domain".into()),
        }
    }
}
pub fn default_for(name: &str, ty: &Type) -> Result<Literal, String> {
    if name == "N" {
        return Ok(vector([0., 0., 1.], Unit::Unitless));
    }
    if name == "pscale" {
        return Ok(Literal::Scalar { value: 1. });
    }
    Ok(match ty {
        Type::Scalar => Literal::Scalar { value: 0. },
        Type::Integer => Literal::Integer { value: 0 },
        Type::Bool => Literal::Bool { value: false },
        Type::Text => Literal::Text {
            value: String::new(),
        },
        Type::Vector {
            dimensions,
            unit,
            space,
        } => Literal::Vector {
            value: vec![0.; *dimensions as usize],
            unit: *unit,
            space: *space,
        },
        Type::Rotation { space } => Literal::Rotation {
            value: [0., 0., 0., 1.],
            space: *space,
        },
        Type::Distance { unit } => Literal::Distance {
            value: 0.,
            unit: *unit,
        },
        Type::Duration => Literal::Duration {
            value: fold_foundation::Time::ZERO,
        },
        _ => return Err("Attribute type requires an explicit topology default".into()),
    })
}
