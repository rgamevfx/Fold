//! Authored scene hierarchy. Objects own graphs; hierarchy is never a synthetic graph.
pub mod animation;
pub mod package;
pub mod video;
use crate::{
    binding::Binding,
    evaluation::{self, Value},
    model::{Canvas, Construction},
    types::{Clock, Literal, Space, Type, Unit},
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{Document, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const TYPE: &str = "fold.motion.native.scene";
pub const SCHEMA: u32 = 1;
pub const EDIT: &str = "fold.motion.native.scene.edit";
pub const GRAPH_EDIT: &str = "fold.motion.native.scene.graph.edit";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub canvas: Canvas,
    pub output: ObjectId,
    pub active_camera: ObjectId,
    pub objects: Vec<Object>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    pub name: String,
    pub parent: Option<ObjectId>,
    pub visible: bool,
    pub locked: bool,
    pub clock: Clock,
    /// Scene-local placement, separate from procedural geometry coordinates.
    pub position: Binding,
    pub rotation: Binding,
    pub scale: Binding,
    pub graph: Construction,
    pub output: ObjectId,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Object {
    pub fn new(name: impl Into<String>, graph: Construction, output: ObjectId) -> Self {
        Self {
            id: ObjectId::new(),
            name: name.into(),
            parent: None,
            visible: true,
            locked: false,
            clock: Clock::default(),
            position: Binding::Literal {
                value: Literal::Vector {
                    value: vec![0.; 3],
                    unit: Unit::Meters,
                    space: Space::World,
                },
            },
            rotation: Binding::Literal {
                value: Literal::Rotation {
                    value: [0., 0., 0., 1.],
                    space: Space::World,
                },
            },
            scale: Binding::Literal {
                value: Literal::Vector {
                    value: vec![1.; 3],
                    unit: Unit::Unitless,
                    space: Space::World,
                },
            },
            graph,
            output,
            extensions: Default::default(),
        }
    }
}
impl Scene {
    pub fn object(&self, id: ObjectId) -> Result<&Object, String> {
        self.objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| "Missing scene object".into())
    }
    pub fn locked(&self, id: ObjectId) -> bool {
        self.ancestors(id)
            .map_or(true, |objects| objects.iter().any(|o| o.locked))
    }
    pub fn ancestors(&self, id: ObjectId) -> Result<Vec<&Object>, String> {
        let mut result = vec![];
        let mut seen = BTreeSet::new();
        let mut next = Some(id);
        while let Some(id) = next {
            if !seen.insert(id) || seen.len() > 256 {
                return Err("Scene hierarchy cycle or depth limit".into());
            }
            let object = self.object(id)?;
            result.push(object);
            next = object.parent;
        }
        result.reverse();
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.canvas.width == 0
            || self.canvas.height == 0
            || self.canvas.width > 16384
            || self.canvas.height > 16384
            || self.canvas.rate.contains(&0)
            || self.canvas.end <= self.canvas.start
        {
            return Err("Invalid scene canvas or time range".into());
        }
        fold_graph::model::extension_keys(
            &self.extensions,
            &["canvas", "output", "active_camera", "objects"],
        )?;
        if self.objects.is_empty() || self.objects.len() > 4096 {
            return Err("Scene requires 1..4096 objects".into());
        }
        let mut ids = BTreeSet::from([self.output]);
        for object in &self.objects {
            if !ids.insert(object.id)
                || object.name.is_empty()
                || object.name.len() > 256
                || object.name.chars().any(char::is_control)
            {
                return Err("Invalid or duplicate scene object identity/name".into());
            }
            fold_graph::model::extension_keys(
                &object.extensions,
                &[
                    "id", "name", "parent", "visible", "locked", "clock", "position", "rotation",
                    "scale", "graph", "output",
                ],
            )?;
            object.graph.validate_structure()?;
            let mut transform_ids = BTreeSet::new();
            for binding in [&object.position, &object.rotation, &object.scale] {
                if let Binding::Track { track } = binding {
                    track.validate()?;
                    if !transform_ids.insert(track.id)
                        || track.keys.iter().any(|key| !transform_ids.insert(key.id))
                    {
                        return Err("Duplicate scene transform track or key identity".into());
                    }
                }
            }
            let output = object
                .graph
                .definition(object.graph.root.definition)?
                .outputs
                .iter()
                .find(|o| o.id == object.output)
                .ok_or("Missing object graph output")?;
            if !matches!(
                output.ty,
                Type::Geometry | Type::Placement | Type::Camera | Type::Light
            ) {
                return Err("Object graph must output geometry, placement, camera or light".into());
            }
            self.ancestors(object.id)?;
            local_transform(object, self.canvas.start)?;
        }
        let camera = self.object(self.active_camera)?;
        let output = camera
            .graph
            .definition(camera.graph.root.definition)?
            .outputs
            .iter()
            .find(|o| o.id == camera.output)
            .ok_or("Missing camera output")?;
        if output.ty != Type::Camera {
            return Err("Active scene camera must reference a camera object".into());
        }
        Ok(())
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > 16 * 1024 * 1024 {
            return Err("Scene exceeds 16 MiB".into());
        }
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: TYPE.into(),
            schema_version: SCHEMA,
            revision: Revision::default(),
            dependencies: vec![],
            assets: vec![],
            payload,
            extensions: Default::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != TYPE
            || document.schema_version != SCHEMA
            || document.payload.len() > 16 * 1024 * 1024
        {
            return Err("Unsupported scene document".into());
        }
        if !document.dependencies.is_empty() || !document.assets.is_empty() {
            return Err("Scene resource declarations do not match its embedded graphs".into());
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn replace(&self, original: &Document) -> Result<Document, String> {
        let before = Self::from_document(original)?;
        self.validate()?;
        for old in &before.objects {
            let after = self.objects.iter().find(|o| o.id == old.id);
            if before.locked(old.id) && after != Some(old) {
                return Err("Scene object or ancestor is locked".into());
            }
            if let Some(after) = after {
                crate::protection::validate(&old.graph, &after.graph).map_err(|e| e.reason)?;
            } else if !crate::protection::locked_ids(&old.graph)
                .map_err(|e| e.reason)?
                .is_empty()
            {
                return Err("Cannot remove an object containing protected graph controls".into());
            }
        }
        let mut document = self.document(original.id)?;
        document.revision = original.revision;
        document.extensions = original.extensions.clone();
        Ok(document)
    }
    pub fn evaluate(
        &self,
        time: Time,
        limits: evaluation::Limits,
        cancel: &dyn Fn() -> Result<(), String>,
    ) -> Result<evaluation::Scene, String> {
        self.validate()?;
        cancel()?;
        let mut result = evaluation::Scene {
            entries: vec![],
            camera: None,
            cameras: vec![],
            lights: vec![],
        };
        let mut transforms = BTreeMap::new();
        let mut object_times = BTreeMap::new();
        for object in &self.objects {
            cancel()?;
            let ancestors = self.ancestors(object.id)?;
            if ancestors.iter().any(|o| !o.visible) {
                continue;
            }
            let mut transform = identity();
            let mut local_time = time;
            for ancestor in ancestors {
                local_time = ancestor.clock.at(local_time)?;
                transform = compose(&transform, &local_transform(ancestor, local_time)?)?;
            }
            transforms.insert(object.id, transform);
            object_times.insert(object.id, local_time);
        }
        // Divide the request budget across objects; no object obtains an unbounded fresh budget.
        let count = self.objects.len();
        let per_object = evaluation::Limits {
            steps: limits.steps / count,
            elements: limits.elements / count,
        };
        for object in &self.objects {
            let Some(transform) = transforms.get(&object.id) else {
                continue;
            };
            cancel()?;
            let value = evaluation::Prepared::new(object.graph.clone())?.evaluate_in_object(
                object.output,
                object_times[&object.id],
                object.id,
                per_object.clone(),
                cancel,
            )?;
            let id = occurrence(object.id);
            match value {
                Value::Geometry(geometry) => {
                    if geometry.space != Space::World || geometry.unit != Unit::Meters {
                        return Err("Scene objects require world/meter geometry".into());
                    }
                    result.entries.push(evaluation::SceneEntry {
                        id,
                        placement: Arc::new(evaluation::Placement {
                            id,
                            geometry,
                            transform: evaluation::placement::Transform::World(transform.clone()),
                            opacity: 1.,
                        }),
                    });
                }
                Value::Placement(placement) => {
                    let evaluation::placement::Transform::World(world) = &placement.transform
                    else {
                        return Err("Scene placement must be world-space".into());
                    };
                    let mut placement = (*placement).clone();
                    placement.id = id;
                    placement.transform =
                        evaluation::placement::Transform::World(compose(transform, world)?);
                    result.entries.push(evaluation::SceneEntry {
                        id,
                        placement: Arc::new(placement),
                    });
                }
                Value::Camera(camera) => {
                    let mut camera = (*camera).clone();
                    camera.id = id;
                    camera.position = transform.point(camera.position);
                    camera.target = transform.point(camera.target);
                    camera.up = direction(transform, camera.up);
                    let camera = Arc::new(camera);
                    if object.id == self.active_camera {
                        result.camera = Some(camera.clone());
                    }
                    result
                        .cameras
                        .push(evaluation::spatial::CameraEntry { id, camera });
                }
                Value::Light(light) => {
                    let mut light = (*light).clone();
                    light.id = id;
                    light.direction = direction(transform, light.direction);
                    result.lights.push(evaluation::spatial::LightEntry {
                        id,
                        light: Arc::new(light),
                    });
                }
                _ => return Err("Unsupported scene object output".into()),
            }
        }
        if result.camera.is_none() {
            return Err("Active camera is hidden or unavailable".into());
        }
        Ok(result)
    }
}
fn sample(binding: &Binding, time: Time) -> Result<Literal, String> {
    match binding {
        Binding::Literal { value } => {
            value.validate()?;
            Ok(value.clone())
        }
        Binding::Track { track } => track.sample(time),
        _ => Err("Scene transform requires a literal or native key track".into()),
    }
}
fn local_transform(object: &Object, time: Time) -> Result<evaluation::placement::World, String> {
    let Literal::Vector {
        value,
        unit: Unit::Meters,
        space: Space::World,
    } = sample(&object.position, time)?
    else {
        return Err("Object position requires world meters".into());
    };
    let Literal::Rotation {
        value: rotation,
        space: Space::World,
    } = sample(&object.rotation, time)?
    else {
        return Err("Object orientation requires a world rotation".into());
    };
    let scale = match sample(&object.scale, time)? {
        Literal::Scalar { value } => vec![value; 3],
        Literal::Vector {
            value,
            unit: Unit::Unitless,
            space: Space::World,
        } => value,
        _ => return Err("Object scale requires a scalar or dimensionless world vector".into()),
    };
    if value.len() != 3
        || value.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
        || scale.len() != 3
        || scale
            .iter()
            .any(|v| !v.is_finite() || !(1e-6..=1e6).contains(v))
    {
        return Err("Scene transform outside supported range".into());
    }
    Ok(evaluation::placement::World {
        affine: None,
        position: [value[0], value[1], value[2]],
        rotation,
        scale: [scale[0], scale[1], scale[2]],
    })
}
fn identity() -> evaluation::placement::World {
    evaluation::placement::World {
        affine: None,
        position: [0.; 3],
        rotation: [0., 0., 0., 1.],
        scale: [1.; 3],
    }
}
fn direction(t: &evaluation::placement::World, v: [f64; 3]) -> [f64; 3] {
    let v = t.matrix().vector(v);
    let length = v.iter().map(|v| v * v).sum::<f64>().sqrt();
    v.map(|v| v / length)
}
fn compose(
    a: &evaluation::placement::World,
    b: &evaluation::placement::World,
) -> Result<evaluation::placement::World, String> {
    let matrix = a.matrix().compose(b.matrix());
    matrix.validate()?;
    Ok(evaluation::placement::World {
        position: matrix.0[3],
        rotation: [0., 0., 0., 1.],
        scale: [1.; 3],
        affine: Some(matrix),
    })
}
fn occurrence(id: ObjectId) -> u64 {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(id.to_string().as_bytes());
    u64::from_le_bytes(d[..8].try_into().unwrap())
}
