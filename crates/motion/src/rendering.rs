//! Snapshot-bound rendering preparation. No GPU state or document mutation.
mod image;
mod plane;
mod screen;
use crate::{
    evaluation::{self, GeometryData, Value},
    execution::{Budget, Stop},
    model::Construction,
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::Snapshot;
use fold_render::spatial as render;
pub use image::image;
pub use screen::{ScreenFrame, screen};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Evaluation(evaluation::Failure),
    Budget,
    Cancelled,
}
pub struct Limits {
    pub evaluation: evaluation::Limits,
    pub conversion_steps: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            evaluation: evaluation::Limits::default(),
            conversion_steps: 2_000_000,
        }
    }
}
pub struct SpatialFrame {
    /// Retains layout/font/source provenance alongside the exact evaluated scene.
    pub native: Arc<evaluation::Scene>,
    pub scene: render::Scene,
    pub binding: render::Binding,
}
/// Prepare a spatial-only output from one immutable project snapshot. Screen
/// entries are rejected, never dropped or implicitly projected into world space.
pub fn spatial(
    snapshot: &Snapshot,
    document: DocumentId,
    output: ObjectId,
    time: Time,
    dimensions: [u32; 2],
    limits: Limits,
    cancellation: &dyn Fn() -> Result<(), String>,
) -> Result<SpatialFrame, Error> {
    let evaluated = evaluate(
        snapshot,
        document,
        output,
        time,
        dimensions,
        limits.evaluation,
        cancellation,
    )?;
    let mut budget = Budget::new(limits.conversion_steps, cancellation);
    prepare_spatial(evaluated, snapshot, document, time, dimensions, &mut budget)
}
fn prepare_spatial(
    evaluated: Evaluated,
    snapshot: &Snapshot,
    document: DocumentId,
    time: Time,
    dimensions: [u32; 2],
    budget: &mut Budget<'_>,
) -> Result<SpatialFrame, Error> {
    let native = evaluated.native;
    let duration = evaluated.duration;
    let camera = native
        .camera
        .as_ref()
        .ok_or_else(|| Error::Invalid("Spatial rendering requires an active camera".into()))?;
    let mut step = || conversion_step(budget);
    step()?;
    let camera = camera_input(camera);
    let binding = render::Binding {
        document,
        revision: snapshot.revision().0,
        camera: camera.id,
        camera_content: camera.identity(),
        time,
        duration,
        dimensions,
    };
    let mut scene = render::Scene {
        meshes: BTreeMap::new(),
        materials: BTreeMap::new(),
        instances: vec![],
        camera,
        lights: vec![],
    };
    if native.lights.len() > 8 {
        return Err(Error::Budget);
    }
    for light in &native.lights {
        step()?;
        scene.lights.push(render::DirectionalLight {
            id: resource(b"foldlite", light.id),
            direction: light.light.direction.map(|v| v as f32),
            color: light.light.color.map(|v| v as f32),
            illuminance: light.light.illuminance as f32,
        });
    }
    let mut vertices = 0usize;
    let mut indices = 0usize;
    let mut planes = plane::Preparation::default();
    for entry in &native.entries {
        step()?;
        let placement = &entry.placement;
        if let evaluation::placement::Transform::CameraPlane(plane) = &placement.transform {
            planes.append(
                entry,
                plane,
                [evaluated.canvas, dimensions],
                &mut scene,
                &mut step,
            )?;
            continue;
        }
        let evaluation::placement::Transform::World(world) = &placement.transform else {
            return Err(Error::Invalid(
                "Screen entries require mixed-pass rendering".into(),
            ));
        };
        let material = placement
            .geometry
            .appearance
            .material
            .as_ref()
            .ok_or_else(|| {
                Error::Invalid("Spatial geometry requires an explicit Material".into())
            })?;
        let (prototype, copies) =
            if let GeometryData::Instances(collection) = &placement.geometry.data {
                if collection.instances.len() > 4096 {
                    return Err(Error::Budget);
                }
                let mut copies = Vec::with_capacity(collection.instances.len());
                for instance in &collection.instances {
                    step()?;
                    copies.push((
                        Some(instance.id),
                        instance.position,
                        instance.scale,
                        instance.rotation_z,
                        instance.color.as_ref().map(|color| color.value),
                    ));
                }
                (collection.prototype.as_ref(), copies)
            } else {
                (
                    placement.geometry.as_ref(),
                    vec![(None, [0.; 3], 1., 0., None)],
                )
            };
        let mut parts = vec![];
        match &prototype.data {
            GeometryData::Mesh(mesh) => {
                parts.push((prototype.id, prototype.id, &mesh.prepared.mesh, [0.; 3]));
            }
            GeometryData::Solids(solids) => {
                for (glyph, topology) in solids.prepared.glyphs.iter().zip(&solids.glyphs) {
                    step()?;
                    let (Some(solid), Some(topology_value)) = (&glyph.solid, &topology.topology)
                    else {
                        continue;
                    };
                    if glyph.transform[..4] != [1., 0., 0., 1.] {
                        return Err(Error::Invalid(
                            "Spatial glyph affine transform is not qualified".into(),
                        ));
                    }
                    parts.push((
                        topology.id,
                        topology_value.id,
                        &solid.mesh,
                        [glyph.transform[4], glyph.transform[5], 0.],
                    ));
                }
            }
            _ => return Err(Error::Invalid("Unsupported spatial geometry".into())),
        }
        if parts
            .len()
            .saturating_mul(copies.len())
            .saturating_add(scene.instances.len())
            > 4096
        {
            return Err(Error::Budget);
        }
        for (copy_id, position, scale, rotation_z, instance_color) in copies {
            let material_id = match (copy_id, instance_color) {
                (Some(copy), Some(_)) => {
                    copied_resource(b"fold.motion.render.material.v1", entry.id, copy, 0)
                }
                _ => resource(b"foldmatl", entry.id),
            };
            if !scene.materials.contains_key(&material_id) {
                if scene.materials.len() == 1024 {
                    return Err(Error::Budget);
                }
                let mut color = instance_color.unwrap_or(material.color.value);
                color[3] *= placement.opacity;
                scene.materials.insert(
                    material_id,
                    render::Material {
                        color: color.map(|v| v as f32),
                        unlit: material.unlit,
                        roughness: material.roughness as f32,
                        reflectance: material.reflectance as f32,
                    },
                );
            }
            for &(part_id, topology_id, mesh, translation) in &parts {
                step()?;
                let mesh_id = resource(b"foldmesh", topology_id);
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    scene.meshes.entry(mesh_id)
                {
                    vertices = vertices.saturating_add(mesh.positions.len());
                    indices = indices.saturating_add(mesh.indices.len());
                    if vertices > 1_000_000 || indices > 3_000_000 {
                        return Err(Error::Budget);
                    }
                    for _ in &mesh.positions {
                        step()?;
                    }
                    for _ in &mesh.indices {
                        step()?;
                    }
                    entry.insert(mesh.clone());
                }
                if scene.meshes.len() > 1024 || scene.instances.len() == 4096 {
                    return Err(Error::Budget);
                }
                scene.instances.push(render::Instance {
                    id: match copy_id {
                        Some(copy) => copied_resource(
                            b"fold.motion.render.instance.v1",
                            entry.id,
                            copy,
                            part_id,
                        ),
                        None => pair(entry.id, part_id),
                    },
                    mesh: mesh_id,
                    material: material_id,
                    transform: instance_transform(world, position, scale, rotation_z, translation),
                });
            }
        }
    }
    step()?;
    scene.validate(&binding).map_err(Error::Invalid)?;
    step()?;
    Ok(SpatialFrame {
        native,
        scene,
        binding,
    })
}
fn instance_transform(
    world: &evaluation::placement::World,
    position: [f64; 3],
    scale: f64,
    angle: f64,
    translation: [f64; 3],
) -> render::Transform {
    let (half_sin, half_cos) = (angle * 0.5).sin_cos();
    let copy =
        fold_foundation::affine::Affine::trs(position, [0., 0., half_sin, half_cos], [scale; 3]);
    if world.affine.is_none() && (angle == 0. || world.scale[0] == world.scale[1]) {
        // Keep the exact TRS representation when composition introduces no shear.
        let [x, y, z, w] = world.rotation;
        render::Transform {
            affine: None,
            translation: world.point(copy.point(translation)).map(|v| v as f32),
            rotation: [
                x * half_cos + y * half_sin,
                y * half_cos - x * half_sin,
                z * half_cos + w * half_sin,
                w * half_cos - z * half_sin,
            ]
            .map(|v| v as f32),
            scale: world.scale.map(|v| (v * scale) as f32),
        }
    } else {
        render::Transform {
            affine: Some(world.matrix().compose(copy).compose(
                fold_foundation::affine::Affine::trs(translation, [0., 0., 0., 1.], [1.; 3]),
            )),
            ..Default::default()
        }
    }
}
fn camera_input(camera: &evaluation::spatial::Camera) -> render::Camera {
    render::Camera {
        id: resource(b"foldcamr", camera.id),
        position: camera.position.map(|v| v as f32),
        target: camera.target.map(|v| v as f32),
        up: camera.up.map(|v| v as f32),
        projection: match camera.projection {
            evaluation::spatial::Projection::Perspective { vertical_fov } => {
                render::Projection::Perspective {
                    vertical_fov: vertical_fov as f32,
                }
            }
            evaluation::spatial::Projection::Orthographic { vertical_size } => {
                render::Projection::Orthographic {
                    vertical_size: vertical_size as f32,
                }
            }
        },
        near: camera.near as f32,
        far: camera.far as f32,
        ev100: camera.ev100 as f32,
    }
}
fn conversion_step(budget: &mut Budget<'_>) -> Result<(), Error> {
    budget.step().map_err(|_| match budget.stop() {
        Some(Stop::Cancelled) => Error::Cancelled,
        _ => Error::Budget,
    })
}
fn resource(domain: &[u8; 8], id: u64) -> ObjectId {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(domain);
    bytes[8..].copy_from_slice(&id.to_be_bytes());
    ObjectId::from_bytes(bytes)
}
fn copied_resource(domain: &[u8], placement: u64, copy: u64, part: u64) -> ObjectId {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(domain);
    for id in [placement, copy, part] {
        hash.update(id.to_le_bytes());
    }
    let digest = hash.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    ObjectId::from_bytes(bytes)
}
fn pair(occurrence: u64, glyph: u64) -> ObjectId {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&occurrence.to_be_bytes());
    bytes[8..].copy_from_slice(&glyph.to_be_bytes());
    ObjectId::from_bytes(bytes)
}

struct Evaluated {
    native: Arc<evaluation::Scene>,
    duration: Time,
    canvas: [u32; 2],
}
fn evaluate(
    snapshot: &Snapshot,
    document: DocumentId,
    output: ObjectId,
    time: Time,
    dimensions: [u32; 2],
    limits: evaluation::Limits,
    cancellation: &dyn Fn() -> Result<(), String>,
) -> Result<Evaluated, Error> {
    cancellation().map_err(|_| Error::Cancelled)?;
    if dimensions.iter().any(|v| *v == 0 || *v > 4096) {
        return Err(Error::Invalid("Unsupported frame dimensions".into()));
    }
    let document_value = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or_else(|| Error::Invalid("Missing motion document".into()))?;
    if document_value.type_id == crate::scene::TYPE {
        let scene = crate::scene::Scene::from_document(document_value).map_err(Error::Invalid)?;
        if output != scene.output {
            return Err(Error::Invalid("Missing scene output".into()));
        }
        let native = Arc::new(
            scene
                .evaluate(time, limits, cancellation)
                .map_err(Error::Invalid)?,
        );
        return Ok(Evaluated {
            native,
            canvas: [scene.canvas.width, scene.canvas.height],
            duration: Time::new(i64::from(scene.canvas.rate[1]), scene.canvas.rate[0])
                .map_err(|e| Error::Invalid(e.to_string()))?,
        });
    }
    let construction = Construction::from_document(document_value).map_err(Error::Invalid)?;
    let duration = Time::new(
        i64::from(construction.canvas.rate[1]),
        construction.canvas.rate[0],
    )
    .map_err(|e| Error::Invalid(e.to_string()))?;
    let canvas = [construction.canvas.width, construction.canvas.height];
    let prepared = evaluation::Prepared::new(construction).map_err(Error::Invalid)?;
    let Value::Scene(native) = prepared
        .evaluate_detailed(output, time, limits, cancellation)
        .map_err(Error::Evaluation)?
    else {
        return Err(Error::Invalid("Rendering requires a Scene output".into()));
    };
    Ok(Evaluated {
        native,
        duration,
        canvas,
    })
}
