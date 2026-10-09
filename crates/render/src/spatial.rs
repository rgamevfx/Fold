//! Immutable, feature-neutral inputs. IDs belong to Fold; no ECS state escapes.
pub mod extrude;
pub mod planar;
pub mod plane;
pub mod primitives;
/// Whether this build includes the GPU implementation of spatial image sources.
pub const GPU_ENABLED: bool = cfg!(feature = "spatial");
use fold_foundation::{DocumentId, ObjectId, Time};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub document: DocumentId,
    pub revision: u64,
    pub camera: ObjectId,
    /// SHA-256 of the complete camera snapshot, including exposure and projection.
    pub camera_content: [u8; 32],
    pub time: Time,
    pub duration: Time,
    pub dimensions: [u32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Straight, scene-linear Rec.709. Attachment blending produces premultiplied pixels.
    pub color: [f32; 4],
    pub unlit: bool,
    pub roughness: f32,
    pub reflectance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Complete evaluated matrix when hierarchy introduces shear.
    pub affine: Option<fold_foundation::affine::Affine>,
    pub translation: [f32; 3],
    /// Unit quaternion, xyzw.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            affine: None,
            translation: [0.; 3],
            rotation: [0., 0., 0., 1.],
            scale: [1.; 3],
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: ObjectId,
    pub mesh: ObjectId,
    pub material: ObjectId,
    pub transform: Transform,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Perspective {
        vertical_fov: f32,
    },
    /// Vertical viewport extent in world meters, independent of camera distance.
    Orthographic {
        vertical_size: f32,
    },
}
impl Projection {
    fn valid(self) -> bool {
        match self {
            Self::Perspective { vertical_fov } => {
                vertical_fov.is_finite()
                    && (0.001..std::f32::consts::PI - 0.001).contains(&vertical_fov)
            }
            Self::Orthographic { vertical_size } => {
                vertical_size.is_finite() && (1e-6..=1e6).contains(&vertical_size)
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Camera {
    pub id: ObjectId,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub projection: Projection,
    pub near: f32,
    pub far: f32,
    pub ev100: f32,
}
impl Camera {
    fn validate(&self) -> Result<(), String> {
        let camera = self;
        let position = camera.position;
        let direction = std::array::from_fn(|i| camera.target[i] - position[i]);
        let up = camera.up;
        if !finite(position)
            || !finite(direction)
            || !finite(up)
            || !length_squared(direction).is_finite()
            || !length_squared(up).is_finite()
            || length_squared(direction) < 1e-12
            || length_squared(up) < 1e-12
            || cross_normalized_squared(direction, up) < 1e-6
            || !camera.projection.valid()
            || !camera.near.is_finite()
            || !camera.far.is_finite()
            || camera.near < 1e-6
            || camera.far <= camera.near
            || !(-20.0..=20.).contains(&camera.ev100)
        {
            return Err("Invalid spatial camera".into());
        }
        Ok(())
    }
    pub fn identity(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(b"fold.spatial.camera.v2");
        let (mode, size) = match self.projection {
            Projection::Perspective { vertical_fov } => (0u8, vertical_fov),
            Projection::Orthographic { vertical_size } => (1u8, vertical_size),
        };
        hash.update([mode]);
        hash.update(size.to_bits().to_le_bytes());
        hash.update(self.id.to_bytes());
        for value in self
            .position
            .into_iter()
            .chain(self.target)
            .chain(self.up)
            .chain([self.near, self.far, self.ev100])
        {
            hash.update(value.to_bits().to_le_bytes());
        }
        hash.finalize().into()
    }
}
#[derive(Clone, Debug)]
pub struct DirectionalLight {
    pub id: ObjectId,
    /// Direction light travels, in world coordinates; need not be normalized.
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub illuminance: f32,
}
#[derive(Clone, Debug)]
pub struct Scene {
    pub meshes: BTreeMap<ObjectId, Arc<Mesh>>,
    pub materials: BTreeMap<ObjectId, Material>,
    pub instances: Vec<Instance>,
    pub camera: Camera,
    pub lights: Vec<DirectionalLight>,
}

impl Scene {
    /// Validate the complete immutable request before allocating or changing retained state.
    pub fn validate(&self, binding: &Binding) -> Result<(), String> {
        if binding.duration <= Time::ZERO
            || binding.camera != self.camera.id
            || binding.camera_content != self.camera.identity()
            || binding.dimensions.iter().any(|v| *v == 0 || *v > 4096)
        {
            return Err("Invalid spatial frame binding".into());
        }
        if self.meshes.len() > 1024
            || self.materials.len() > 1024
            || self.instances.len() > 4096
            || self.lights.len() > 8
        {
            return Err("Spatial scene exceeds preparation budget".into());
        }
        let mut vertices = 0usize;
        let mut indices = 0usize;
        for mesh in self.meshes.values() {
            vertices = vertices.saturating_add(mesh.positions.len());
            indices = indices.saturating_add(mesh.indices.len());
            if vertices > 1_000_000 || indices > 3_000_000 {
                return Err("Spatial geometry exceeds preparation budget".into());
            }
            if mesh.positions.is_empty()
                || mesh.normals.len() != mesh.positions.len()
                || mesh.indices.is_empty()
                || !mesh.indices.len().is_multiple_of(3)
                || mesh
                    .positions
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
                || mesh
                    .normals
                    .iter()
                    .any(|v| !finite(*v) || (length_squared(*v) - 1.).abs() > 0.001)
                || mesh
                    .indices
                    .iter()
                    .any(|i| *i as usize >= mesh.positions.len())
            {
                return Err("Invalid spatial triangle mesh".into());
            }
        }
        for material in self.materials.values() {
            if material.color.iter().any(|v| !v.is_finite())
                || material.color[..3]
                    .iter()
                    .any(|v| !(0.0..=65504.).contains(v))
                || !(0.0..=1.).contains(&material.color[3])
                || !(0.0..=1.).contains(&material.roughness)
                || !(0.0..=1.).contains(&material.reflectance)
            {
                return Err("Invalid spatial material".into());
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for instance in &self.instances {
            let t = instance.transform;
            if let Some(matrix) = t.affine {
                matrix.validate()?;
            }
            if !ids.insert(instance.id)
                || !self.meshes.contains_key(&instance.mesh)
                || !self.materials.contains_key(&instance.material)
                || t.translation
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
                || t.scale
                    .iter()
                    .any(|v| !v.is_finite() || *v <= 0. || *v > 1e6)
                || t.rotation.iter().any(|v| !v.is_finite())
                || (t.rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 0.001
            {
                return Err("Invalid spatial instance or resource reference".into());
            }
        }
        self.camera.validate()?;
        ids.clear();
        for light in &self.lights {
            let direction = light.direction;
            if !ids.insert(light.id)
                || !finite(direction)
                || !length_squared(direction).is_finite()
                || length_squared(direction) < 1e-12
                || light
                    .color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.).contains(v))
                || !light.illuminance.is_finite()
                || !(0.0..=100_000.).contains(&light.illuminance)
            {
                return Err("Invalid spatial directional light".into());
            }
        }
        Ok(())
    }
}

fn finite(v: [f32; 3]) -> bool {
    v.into_iter().all(f32::is_finite)
}
fn length_squared(v: [f32; 3]) -> f32 {
    v.into_iter().map(|x| x * x).sum()
}
fn cross_normalized_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let a = a.map(|v| v / length_squared(a).sqrt());
    let b = b.map(|v| v / length_squared(b).sqrt());
    length_squared([
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ])
}
