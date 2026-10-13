//! Authoritative spatial documents. Render snapshots contain evaluated values,
//! never editable document state, 2D paths, scripts or ECS entities.
use crate::{PACKAGE, SCHEMA, TYPE};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::{
    binding::Binding,
    model::{Parameter, extension_keys},
    types::{Literal, Space, Unit},
};
use fold_media::VideoInfo;
use fold_project::{Document, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub info: VideoInfo,
    pub roots: Vec<ObjectId>,
    pub objects: BTreeMap<ObjectId, Object>,
    pub geometry: BTreeMap<ObjectId, Geometry>,
    pub materials: BTreeMap<ObjectId, Material>,
    pub active_camera: ObjectId,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Object {
    pub name: String,
    pub visible: bool,
    pub protected: bool,
    pub transform: Transform,
    pub content: Content,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Object {
    pub fn new(name: &str, content: Content) -> Self {
        Self {
            name: name.into(),
            visible: true,
            protected: false,
            transform: Transform::default(),
            content,
            extensions: Default::default(),
        }
    }
    pub fn camera() -> Self {
        let mut object = Self::new(
            "Camera",
            Content::Camera {
                target: [0., 0., -1.],
                up: [0., 1., 0.],
                projection: Projection::Perspective {
                    vertical_fov: std::f32::consts::FRAC_PI_4,
                },
                near: 0.01,
                far: 100.,
                ev100: 8.,
                extensions: Default::default(),
            },
        );
        object.transform.position.value = serde_json::to_value(Binding::Literal {
            value: Literal::Vector {
                value: vec![0., 0., 5.],
                unit: Unit::Meters,
                space: Space::World,
            },
        })
        .expect("serializable literal");
        object
    }
    pub fn light() -> Self {
        Self::new(
            "Light",
            Content::DirectionalLight {
                direction: [-1.; 3],
                color: [1.; 3],
                illuminance: 1000.,
                extensions: Default::default(),
            },
        )
    }
    pub fn named_parameters(&self) -> [(&'static str, &Parameter); 3] {
        [
            ("Position", &self.transform.position),
            ("Rotation", &self.transform.rotation),
            ("Scale", &self.transform.scale),
        ]
    }
    pub fn parameter(&self, id: ObjectId) -> Result<&Parameter, String> {
        self.named_parameters()
            .into_iter()
            .map(|(_, value)| value)
            .find(|value| value.id == id)
            .ok_or("Missing 3D parameter".into())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Transform {
    /// Frozen parent motion, multiplied before the authored local channels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_space: Vec<ParentTransform>,
    pub position: Parameter,
    pub rotation: Parameter,
    pub scale: Parameter,
    #[serde(flatten)]
    pub extensions: Metadata,
}
/// One immutable factor in a parent-space compensation. Tracks have independent
/// identities and retain their exact-time interpolation, including Bezier handles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ParentTransform {
    pub inverse: bool,
    /// Position, rotation, scale, in that order.
    pub channels: [Binding; 3],
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Default for Transform {
    fn default() -> Self {
        let parameter =
            |value| Parameter::new(Binding::Literal { value }).expect("serializable literal");
        Self {
            parent_space: vec![],
            position: parameter(Literal::Vector {
                value: vec![0.; 3],
                unit: Unit::Meters,
                space: Space::World,
            }),
            rotation: parameter(Literal::Rotation {
                value: [0., 0., 0., 1.],
                space: Space::World,
            }),
            scale: parameter(Literal::Vector {
                value: vec![1.; 3],
                unit: Unit::Unitless,
                space: Space::World,
            }),
            extensions: Default::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Group {
        children: Vec<ObjectId>,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Instance {
        geometry: ObjectId,
        material: ObjectId,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Camera {
        target: [f32; 3],
        up: [f32; 3],
        projection: Projection,
        near: f32,
        far: f32,
        ev100: f32,
        #[serde(flatten)]
        extensions: Metadata,
    },
    DirectionalLight {
        direction: [f32; 3],
        color: [f32; 3],
        illuminance: f32,
        #[serde(flatten)]
        extensions: Metadata,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Projection {
    Perspective { vertical_fov: f32 },
    Orthographic { vertical_size: f32 },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    Box {
        size: [f32; 3],
        #[serde(flatten)]
        extensions: Metadata,
    },
    Sphere {
        radius: f32,
        #[serde(flatten)]
        extensions: Metadata,
    },
    Mesh {
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        indices: Vec<u32>,
        #[serde(flatten)]
        extensions: Metadata,
    },
}
impl Default for Geometry {
    fn default() -> Self {
        Self::Box {
            size: [1.; 3],
            extensions: Default::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Material {
    /// Straight scene-linear Rec.709, including explicitly authored alpha.
    pub color: [f32; 4],
    pub unlit: bool,
    pub roughness: f32,
    pub reflectance: f32,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Default for Material {
    fn default() -> Self {
        Self {
            color: [0.1, 0.3, 0.6, 1.],
            unlit: false,
            roughness: 0.5,
            reflectance: 0.5,
            extensions: Default::default(),
        }
    }
}
impl Scene {
    /// A lit native box and a camera. Mesh/material IDs are shared resources;
    /// adding instances does not copy their geometry into the document.
    pub fn new(info: VideoInfo) -> Self {
        let mesh = ObjectId::new();
        let material = ObjectId::new();
        let instance = ObjectId::new();
        let camera = ObjectId::new();
        let light = ObjectId::new();
        Self {
            info,
            roots: vec![instance, camera, light],
            active_camera: camera,
            objects: [
                (
                    instance,
                    Object::new(
                        "Box",
                        Content::Instance {
                            geometry: mesh,
                            material,
                            extensions: Default::default(),
                        },
                    ),
                ),
                (camera, Object::camera()),
                (light, Object::light()),
            ]
            .into(),
            geometry: [(mesh, Geometry::default())].into(),
            materials: [(material, Material::default())].into(),
            extensions: Default::default(),
        }
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > 16 * 1024 * 1024 {
            return Err("3D document exceeds 16 MiB".into());
        }
        Ok(Document {
            id,
            package_id: PACKAGE.into(),
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
        if document.package_id != PACKAGE
            || document.type_id != TYPE
            || document.schema_version != SCHEMA
            || document.payload.len() > 16 * 1024 * 1024
            || !document.dependencies.is_empty()
            || !document.assets.is_empty()
        {
            return Err("Unsupported 3D document".into());
        }
        let scene: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        scene.validate()?;
        Ok(scene)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.info.validate().map_err(|e| e.to_string())?;
        extension_keys(
            &self.extensions,
            &[
                "info",
                "roots",
                "objects",
                "geometry",
                "materials",
                "active_camera",
            ],
        )?;
        if self.objects.len() > 4096 || self.geometry.len() > 1024 || self.materials.len() > 1024 {
            return Err("3D document exceeds object/resource limits".into());
        }
        let mut ids = BTreeSet::new();
        let mut insert = |id| {
            if ids.insert(id) {
                Ok(())
            } else {
                Err("Duplicate 3D identity".to_string())
            }
        };
        for (&id, object) in &self.objects {
            insert(id)?;
            if object.name.is_empty()
                || object.name.len() > 256
                || object.name.chars().any(char::is_control)
            {
                return Err("Invalid 3D object name".into());
            }
            extension_keys(
                &object.extensions,
                &["name", "visible", "protected", "transform", "content"],
            )?;
            for parameter in [
                &object.transform.position,
                &object.transform.rotation,
                &object.transform.scale,
            ] {
                insert(parameter.id)?;
                if let Binding::Track { track } = parameter.binding()? {
                    track.validate()?;
                    insert(track.id)?;
                    for key in &track.keys {
                        insert(key.id)?;
                    }
                }
            }
            if object.transform.parent_space.len() > 512 {
                return Err("3D parent compensation exceeds 512 factors".into());
            }
            for factor in &object.transform.parent_space {
                extension_keys(&factor.extensions, &["inverse", "channels"])?;
                for binding in &factor.channels {
                    if let Binding::Track { track } = binding {
                        track.validate()?;
                        insert(track.id)?;
                        for key in &track.keys {
                            insert(key.id)?;
                        }
                    }
                }
            }
            match &object.content {
                Content::Group { extensions, .. } => {
                    extension_keys(extensions, &["kind", "children"])
                }
                Content::Instance { extensions, .. } => {
                    extension_keys(extensions, &["kind", "geometry", "material"])
                }
                Content::Camera { extensions, .. } => extension_keys(
                    extensions,
                    &["kind", "target", "up", "projection", "near", "far", "ev100"],
                ),
                Content::DirectionalLight { extensions, .. } => {
                    extension_keys(extensions, &["kind", "direction", "color", "illuminance"])
                }
            }?;
        }
        for (&id, geometry) in &self.geometry {
            insert(id)?;
            match geometry {
                Geometry::Box { extensions, .. } => extension_keys(extensions, &["kind", "size"]),
                Geometry::Sphere { extensions, .. } => {
                    extension_keys(extensions, &["kind", "radius"])
                }
                Geometry::Mesh { extensions, .. } => {
                    extension_keys(extensions, &["kind", "positions", "normals", "indices"])
                }
            }?;
        }
        for (&id, material) in &self.materials {
            insert(id)?;
            extension_keys(
                &material.extensions,
                &["color", "unlit", "roughness", "reflectance"],
            )?;
        }
        self.evaluate(
            DocumentId::new(),
            Revision::default(),
            Time::ZERO,
            [self.info.width, self.info.height],
            &|| Ok(()),
        )
        .map(|_| ())
    }
}
