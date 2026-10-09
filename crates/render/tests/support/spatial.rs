use fold_foundation::{DocumentId, ObjectId, Time};
use fold_render::spatial::*;
use std::{collections::BTreeMap, sync::Arc};

pub fn fixture() -> (Scene, Binding) {
    let mesh = ObjectId::new();
    let material = ObjectId::new();
    let camera = ObjectId::new();
    let scene = Scene {
        meshes: BTreeMap::from([(
            mesh,
            Arc::new(Mesh {
                positions: vec![[-1., -1., 0.], [1., -1., 0.], [1., 1., 0.], [-1., 1., 0.]],
                normals: vec![[0., 0., 1.]; 4],
                indices: vec![0, 1, 2, 0, 2, 3],
            }),
        )]),
        materials: BTreeMap::from([(
            material,
            Material {
                color: [2., 0.25, 0.125, 1.],
                unlit: true,
                roughness: 1.,
                reflectance: 0.,
            },
        )]),
        instances: vec![Instance {
            id: ObjectId::new(),
            mesh,
            material,
            transform: Transform::default(),
        }],
        camera: Camera {
            id: camera,
            position: [0., 0., 4.],
            target: [0.; 3],
            up: [0., 1., 0.],
            projection: Projection::Perspective {
                vertical_fov: std::f32::consts::FRAC_PI_4,
            },
            near: 0.1,
            far: 100.,
            ev100: 8.,
        },
        lights: vec![],
    };
    let binding = Binding {
        document: DocumentId::new(),
        revision: 1,
        camera,
        camera_content: scene.camera.identity(),
        time: Time::ZERO,
        duration: Time::new(1, 24).unwrap(),
        dimensions: [64, 64],
    };
    (scene, binding)
}
