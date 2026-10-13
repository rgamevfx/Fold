//! Exact-time spatial evaluation into immutable render inputs.
use crate::{
    geometry,
    model::{Content, Geometry, Projection, Scene, Transform},
};
use fold_foundation::{DocumentId, ObjectId, Time, affine::Affine};
use fold_graph::{
    binding::Binding,
    model::{Parameter, extension_keys},
    types::{Literal, Space, Unit},
};
use fold_project::Revision;
use fold_render::spatial;
use std::{collections::BTreeMap, sync::Arc};
pub(crate) fn sample(parameter: &Parameter, time: Time) -> Result<Literal, String> {
    extension_keys(&parameter.extensions, &["id", "value", "protected"])?;
    sample_binding(&parameter.binding()?, time)
}
fn sample_binding(binding: &Binding, time: Time) -> Result<Literal, String> {
    match binding {
        Binding::Literal { value } => {
            value.validate()?;
            Ok(value.clone())
        }
        Binding::Track { track } => {
            track.validate()?;
            track.sample(time)
        }
        _ => Err("3D transform requires a literal or keyframe track".into()),
    }
}
pub(crate) fn local(transform: &Transform, time: Time) -> Result<Affine, String> {
    extension_keys(
        &transform.extensions,
        &["position", "rotation", "scale", "parent_space"],
    )?;
    let channels = [
        sample(&transform.position, time)?,
        sample(&transform.rotation, time)?,
        sample(&transform.scale, time)?,
    ];
    Ok(parent_space(transform, time)?.compose(channel_matrix(channels)?))
}
pub(crate) fn parent_space(transform: &Transform, time: Time) -> Result<Affine, String> {
    if transform.parent_space.len() > 512 {
        return Err("3D parent compensation exceeds 512 factors".into());
    }
    let mut result = Affine::IDENTITY;
    for factor in &transform.parent_space {
        result = result.compose(parent_factor(factor, time)?);
        result.validate()?;
    }
    Ok(result)
}
pub(crate) fn parent_factor(
    factor: &crate::model::ParentTransform,
    time: Time,
) -> Result<Affine, String> {
    let [position, rotation, scale] = &factor.channels;
    let matrix = channel_matrix([
        sample_binding(position, time)?,
        sample_binding(rotation, time)?,
        sample_binding(scale, time)?,
    ])?;
    if factor.inverse {
        matrix.inverse()
    } else {
        Ok(matrix)
    }
}
fn channel_matrix([position, rotation, scale]: [Literal; 3]) -> Result<Affine, String> {
    let vector = |literal, unit| match literal {
        Literal::Vector {
            value,
            unit: actual,
            space: Space::World,
        } if value.len() == 3 && actual == unit => Ok([value[0], value[1], value[2]]),
        _ => Err("3D vector has the wrong dimensions, units or space".to_string()),
    };
    let position = vector(position, Unit::Meters)?;
    let scale = vector(scale, Unit::Unitless)?;
    if scale.iter().any(|value| !(1e-6..=1e6).contains(value)) {
        return Err("Invalid 3D scale".into());
    }
    let Literal::Rotation {
        value: rotation,
        space: Space::World,
    } = rotation
    else {
        return Err("3D rotation requires a world quaternion".into());
    };
    let matrix = Affine::trs(position, rotation, scale);
    matrix.validate()?;
    Ok(matrix)
}
impl Scene {
    /// Evaluate an immutable document at exact rational time. Hierarchy retains
    /// shear through affine composition. Every object has exactly one owner.
    pub fn evaluate(
        &self,
        document: DocumentId,
        revision: Revision,
        time: Time,
        dimensions: [u32; 2],
        cancel: &dyn Fn() -> Result<(), String>,
    ) -> Result<(spatial::Scene, spatial::Binding), String> {
        self.info.validate().map_err(|e| e.to_string())?;
        if time < Time::ZERO
            || time
                >= self
                    .info
                    .time(self.info.frames)
                    .map_err(|e| e.to_string())?
        {
            return Err("3D time is outside the document".into());
        }
        let mut matrices = BTreeMap::new();
        let mut visible = BTreeMap::new();
        fn visit(
            scene: &Scene,
            ids: &[ObjectId],
            parent: Affine,
            shown: bool,
            depth: usize,
            matrices: &mut BTreeMap<ObjectId, Affine>,
            visible: &mut BTreeMap<ObjectId, bool>,
            time: Time,
            cancel: &dyn Fn() -> Result<(), String>,
        ) -> Result<(), String> {
            if depth > 128 {
                return Err("3D hierarchy depth exceeds 128".into());
            }
            for id in ids {
                cancel()?;
                if matrices.contains_key(id) {
                    return Err("3D hierarchy cycle or duplicate ownership".into());
                }
                let object = scene.objects.get(id).ok_or("Missing 3D object")?;
                let matrix = parent.compose(local(&object.transform, time)?);
                matrix.validate()?;
                matrices.insert(*id, matrix);
                visible.insert(*id, shown && object.visible);
                if let Content::Group { children, .. } = &object.content {
                    visit(
                        scene,
                        children,
                        matrix,
                        shown && object.visible,
                        depth + 1,
                        matrices,
                        visible,
                        time,
                        cancel,
                    )?;
                }
            }
            Ok(())
        }
        visit(
            self,
            &self.roots,
            Affine::IDENTITY,
            true,
            0,
            &mut matrices,
            &mut visible,
            time,
            cancel,
        )?;
        if matrices.len() != self.objects.len() {
            return Err("Unowned 3D object".into());
        }
        let meshes = self
            .geometry
            .iter()
            .map(|(&id, value)| {
                cancel()?;
                let mesh = match value {
                    Geometry::Box { size, .. } => {
                        geometry::box_mesh(*size)
                            .map_err(|_| "Invalid box size")?
                            .mesh
                    }
                    Geometry::Sphere { radius, .. } => {
                        geometry::sphere_mesh(*radius)
                            .map_err(|_| "Invalid sphere radius")?
                            .0
                    }
                    Geometry::Mesh {
                        positions,
                        normals,
                        indices,
                        ..
                    } => Arc::new(spatial::Mesh {
                        positions: positions.clone(),
                        normals: normals.clone(),
                        indices: indices.clone(),
                    }),
                };
                Ok((id, mesh))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let materials = self
            .materials
            .iter()
            .map(|(&id, value)| {
                (
                    id,
                    spatial::Material {
                        color: value.color,
                        unlit: value.unlit,
                        roughness: value.roughness,
                        reflectance: value.reflectance,
                    },
                )
            })
            .collect();
        let mut instances = vec![];
        let mut lights = vec![];
        let mut camera = None;
        for (&id, object) in &self.objects {
            cancel()?;
            let matrix = matrices[&id];
            // Hidden content still must refer to valid resources.
            if let Content::Instance {
                geometry, material, ..
            } = &object.content
            {
                if !meshes.contains_key(geometry) || !self.materials.contains_key(material) {
                    return Err("Missing 3D geometry or material".into());
                }
            }
            match &object.content {
                Content::Instance {
                    geometry, material, ..
                } if visible[&id] => instances.push(spatial::Instance {
                    id,
                    mesh: *geometry,
                    material: *material,
                    transform: spatial::Transform {
                        affine: Some(matrix),
                        ..Default::default()
                    },
                }),
                Content::Camera {
                    target,
                    up,
                    projection,
                    near,
                    far,
                    ev100,
                    ..
                } => {
                    let projection = match projection {
                        Projection::Perspective { vertical_fov } => {
                            spatial::Projection::Perspective {
                                vertical_fov: *vertical_fov,
                            }
                        }
                        Projection::Orthographic { vertical_size } => {
                            spatial::Projection::Orthographic {
                                vertical_size: *vertical_size,
                            }
                        }
                    };
                    let value = spatial::Camera {
                        id,
                        position: matrix.point([0.; 3]).map(|v| v as f32),
                        target: matrix.point(target.map(f64::from)).map(|v| v as f32),
                        up: matrix.vector(up.map(f64::from)).map(|v| v as f32),
                        projection,
                        near: *near,
                        far: *far,
                        ev100: *ev100,
                    };
                    value.validate()?;
                    if id == self.active_camera && visible[&id] {
                        camera = Some(value);
                    }
                }
                Content::DirectionalLight {
                    direction,
                    color,
                    illuminance,
                    ..
                } => {
                    let value = spatial::DirectionalLight {
                        id,
                        direction: matrix.vector(direction.map(f64::from)).map(|v| v as f32),
                        color: *color,
                        illuminance: *illuminance,
                    };
                    value.validate()?;
                    if visible[&id] {
                        lights.push(value);
                    }
                }
                _ => {}
            }
        }
        let camera = camera.ok_or("3D active camera is missing or hidden")?;
        let binding = spatial::Binding {
            document,
            revision: revision.0,
            camera: camera.id,
            camera_content: camera.identity(),
            time,
            duration: self.info.time(1).map_err(|e| e.to_string())?,
            dimensions,
        };
        let scene = spatial::Scene {
            meshes,
            materials,
            instances,
            camera,
            lights,
        };
        scene.validate(&binding)?;
        Ok((scene, binding))
    }
}
