use fold_foundation::{ObjectId, Time};
use fold_render::spatial::*;
use std::sync::Arc;
#[path = "support/spatial.rs"]
mod support;
use support::fixture;

#[test]
fn spatial_image_graph_preserves_camera_extent_and_rejects_invalid_contracts() {
    use fold_render::{
        ImageOp, RenderGraph,
        region::{Plan, Region},
    };
    let (scene, binding) = fixture();
    let graph = RenderGraph {
        width: 64,
        height: 64,
        nodes: vec![ImageOp::Spatial {
            scene: Arc::new(scene),
            binding,
        }],
        output: 0,
    };
    let crop = Region {
        x: 28,
        y: 28,
        width: 8,
        height: 8,
    };
    let planned = Plan::new(graph.clone(), crop, false).unwrap();
    assert_eq!(planned.working, Region::full([64, 64]));
    assert_eq!(planned.output, crop);
    assert_eq!([planned.graph.width, planned.graph.height], [64, 64]);
    assert!(
        fold_render::render(graph.clone())
            .unwrap_err()
            .contains("GPU")
    );
    // Working-space conversion belongs to the GPU color processor, while the
    // source scene retains its declared linear Rec.709 material contract.
    assert!(Plan::new(graph.clone(), crop, true).is_ok());
    let mut wrong_extent = graph.clone();
    wrong_extent.width = 32;
    assert!(
        Plan::new(wrong_extent, Region::full([32, 64]), false)
            .err()
            .unwrap()
            .contains("dimensions")
    );
    let mut stale_camera = graph;
    let ImageOp::Spatial { binding, .. } = &mut stale_camera.nodes[0] else {
        unreachable!()
    };
    binding.camera = ObjectId::new();
    assert!(Plan::new(stale_camera, crop, false).is_err());
}

#[test]
fn spatial_input_contract_rejects_invalid_geometry_camera_and_binding() {
    let (scene, binding) = fixture();
    scene.validate(&binding).unwrap();
    let invalid: [fn(&mut Scene); 8] = [
        |s| Arc::make_mut(s.meshes.values_mut().next().unwrap()).positions[0][0] = f32::NAN,
        |s| Arc::make_mut(s.meshes.values_mut().next().unwrap()).indices[0] = 99,
        |s| Arc::make_mut(s.meshes.values_mut().next().unwrap()).normals[0] = [0.; 3],
        |s| s.instances.push(s.instances[0].clone()),
        |s| s.instances[0].transform.scale[0] = 0.,
        |s| s.camera.up = [0., 0., 1.],
        |s| s.camera.target = [f32::MAX; 3],
        |s| s.materials.values_mut().next().unwrap().color[3] = 1.1,
    ];
    for edit in invalid {
        let mut bad = scene.clone();
        edit(&mut bad);
        let matching = Binding {
            camera_content: bad.camera.identity(),
            ..binding.clone()
        };
        assert!(bad.validate(&matching).is_err());
    }
    for bad in [
        Binding {
            duration: Time::ZERO,
            ..binding.clone()
        },
        Binding {
            dimensions: [0, 64],
            ..binding.clone()
        },
        Binding {
            camera: ObjectId::new(),
            ..binding.clone()
        },
    ] {
        assert!(scene.validate(&bad).is_err());
    }
    let mut moved_camera = scene.clone();
    moved_camera.camera.position[0] += 1.;
    assert!(moved_camera.validate(&binding).is_err());
    moved_camera
        .validate(&Binding {
            camera_content: moved_camera.camera.identity(),
            ..binding.clone()
        })
        .unwrap();
    let mut large = scene.clone();
    large.instances = vec![large.instances[0].clone(); 4097];
    assert!(large.validate(&binding).unwrap_err().contains("budget"));
}

#[test]
fn projection_kind_and_size_bind_frames_and_validate_before_publication() {
    let (mut scene, mut binding) = fixture();
    let perspective = scene.camera.identity();
    scene.camera.projection = Projection::Orthographic {
        vertical_size: std::f32::consts::FRAC_PI_4,
    };
    assert_ne!(
        perspective,
        scene.camera.identity(),
        "mode is part of identity even with the same numeric size"
    );
    assert!(scene.validate(&binding).is_err());
    binding.camera_content = scene.camera.identity();
    scene.validate(&binding).unwrap();
    for size in [0., -1., 1e-7, 1e6 + 1., f32::NAN, f32::INFINITY] {
        scene.camera.projection = Projection::Orthographic {
            vertical_size: size,
        };
        binding.camera_content = scene.camera.identity();
        assert!(scene.validate(&binding).is_err());
    }
}

#[test]
fn camera_plane_mapping_has_explicit_sizing_orientation_and_invalid_input_errors() {
    use fold_render::spatial::plane::Sizing;
    let (mut scene, _) = fixture();
    // Axis aligned and off-axis cameras have independently known bases.
    for (position, target, right, up, forward) in [
        (
            [0., 0., 4.],
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., -1.],
        ),
        (
            [4., 0., 0.],
            [0., 0., 0.],
            [0., 0., -1.],
            [0., 1., 0.],
            [-1., 0., 0.],
        ),
        (
            [4., 0., 0.],
            [0., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [-1., 0., 0.],
        ),
    ] {
        scene.camera.position = position;
        scene.camera.target = target;
        scene.camera.up = up.map(|v| v as f32);
        for projection in [
            Projection::Perspective {
                vertical_fov: std::f32::consts::FRAC_PI_2,
            },
            Projection::Orthographic { vertical_size: 4. },
        ] {
            scene.camera.projection = projection;
            for distance in [2., 5.] {
                for sizing in [Sizing::Viewport, Sizing::MetersPerUnit(0.01)] {
                    let mapping = scene
                        .camera
                        .plane(distance, [200, 100], [160, 120], sizing)
                        .unwrap();
                    let height = match (sizing, projection) {
                        (Sizing::MetersPerUnit(_), _) => 1.,
                        (_, Projection::Orthographic { .. }) => 4.,
                        (_, Projection::Perspective { .. }) => 2. * distance,
                    };
                    let width = if sizing == Sizing::Viewport {
                        height * 4. / 3.
                    } else {
                        2.
                    };
                    for (point, x, y) in [
                        ([0., 0.], -0.5, 0.5),
                        ([100., 50.], 0., 0.),
                        ([200., 100.], 0.5, -0.5),
                    ] {
                        let actual = mapping.point(point).unwrap();
                        for i in 0..3 {
                            let expected = f64::from(position[i])
                                + forward[i] * distance
                                + right[i] * width * x
                                + up[i] * height * y;
                            assert!((f64::from(actual[i]) - expected).abs() < 2e-6);
                        }
                    }
                    assert_eq!(mapping.normal(), forward.map(|v| -v as f32));
                    assert!(mapping.point([f64::NAN, 0.]).is_err());
                    assert!(mapping.point([f64::MAX, 0.]).is_err());
                }
            }
        }
    }
    for distance in [0., -1., f64::NAN, f64::INFINITY, 1e6 + 1.] {
        assert!(
            scene
                .camera
                .plane(distance, [200, 100], [160, 120], Sizing::Viewport)
                .is_err()
        );
    }
    for scale in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(
            scene
                .camera
                .plane(2., [200, 100], [160, 120], Sizing::MetersPerUnit(scale))
                .is_err()
        );
    }
    assert!(
        scene
            .camera
            .plane(2., [0, 100], [160, 120], Sizing::Viewport)
            .is_err()
    );
    assert!(
        scene
            .camera
            .plane(2., [200, 100], [0, 120], Sizing::Viewport)
            .is_err()
    );
    scene.camera.up = [0.; 3];
    assert!(
        scene
            .camera
            .plane(2., [200, 100], [160, 120], Sizing::Viewport)
            .is_err()
    );
}

#[test]
fn camera_plane_subdivision_scale_tracks_the_view_camera() {
    use fold_render::spatial::plane::Sizing;
    let mut camera = fixture().0.camera;
    camera.position = [0., 0., 4.];
    camera.target = [0., 0., 0.];
    camera.up = [0., 1., 0.];
    camera.projection = Projection::Perspective {
        vertical_fov: std::f32::consts::FRAC_PI_2,
    };
    let mapping = camera
        .plane(2., [200, 100], [200, 100], Sizing::MetersPerUnit(0.01))
        .unwrap();
    let bounds = [[0., 0.], [200., 100.]];
    let scale = |view: &Camera| mapping.projected_scale(view, [200, 100], bounds).unwrap();
    assert!((scale(&camera) - 0.25).abs() < 1e-7);
    let mut closer = camera.clone();
    closer.position[2] = 3.;
    assert!((scale(&closer) - 0.5).abs() < 1e-7);
    closer.projection = Projection::Orthographic { vertical_size: 2. };
    assert!((scale(&closer) - 0.5).abs() < 1e-7);
    closer.position[2] = 20.;
    assert!((scale(&closer) - 0.5).abs() < 1e-7);
    assert!(mapping.projected_scale(&camera, [0, 100], bounds).is_err());
    assert!(
        mapping
            .projected_scale(&camera, [200, 100], [[1., 0.], [0., 1.]])
            .is_err()
    );
}

#[test]
fn camera_plane_scale_bounds_oblique_and_near_clipped_projection() {
    use fold_render::spatial::plane::Sizing;
    let mut attached = fixture().0.camera;
    attached.position = [0., 0., 4.];
    attached.target = [0., 0., 0.];
    attached.up = [0., 1., 0.];
    let mapping = attached
        .plane(2., [200, 100], [200, 100], Sizing::MetersPerUnit(0.01))
        .unwrap();
    let bounds = [[0., 0.], [200., 100.]];
    for orthographic in [false, true] {
        for yaw in [0.35f64, 1.1, 2.7] {
            for z in [4., 2.05, 1.95] {
                let mut view = attached.clone();
                view.position = [0., 0., z];
                view.target = [yaw.sin() as f32, 0., z - yaw.cos() as f32];
                view.near = 0.1;
                view.projection = if orthographic {
                    Projection::Orthographic { vertical_size: 2. }
                } else {
                    Projection::Perspective {
                        vertical_fov: std::f32::consts::FRAC_PI_2,
                    }
                };
                let bound = mapping.projected_scale(&view, [200, 100], bounds).unwrap();
                assert!(bound.is_finite() && bound > 0.);
                // Independent yaw-only pinhole projection, using the exact
                // authored f32 view direction rather than the scale helper.
                let dx = f64::from(view.target[0]);
                let dz = f64::from(view.target[2]) - f64::from(z);
                let length = dx.hypot(dz);
                let (sin, cos) = (dx / length, -dz / length);
                let project = |p: [f64; 2]| {
                    let x = (p[0] - 100.) * 0.01;
                    let y = (50. - p[1]) * 0.01;
                    let dz = 2. - f64::from(z);
                    let depth = sin * x - cos * dz;
                    if depth < f64::from(view.near) {
                        return None;
                    }
                    let x = cos * x + sin * dz;
                    let scale = if orthographic {
                        50.
                    } else {
                        50. / (f64::from(std::f32::consts::FRAC_PI_2) / 2.).tan() / depth
                    };
                    Some([100. + x * scale, 50. - y * scale])
                };
                let mut checked = 0;
                let mut clipped = 0;
                for x in (1..199).step_by(3) {
                    for y in (1..99).step_by(3) {
                        let p = [f64::from(x), f64::from(y)];
                        let Some(a) = project(p) else {
                            clipped += 1;
                            continue;
                        };
                        for delta in [[0.01, 0.], [0., 0.01], [0.01, 0.01], [-0.01, 0.01]] {
                            let Some(b) = project([p[0] + delta[0], p[1] + delta[1]]) else {
                                continue;
                            };
                            for axis in 0..2 {
                                assert!(
                                    (b[axis] - a[axis]).abs() <= bound * 0.01 * (1. + 1e-6),
                                    "ortho={orthographic} yaw={yaw} z={z} p={p:?} bound={bound}"
                                );
                            }
                            checked += 1;
                        }
                    }
                }
                if z != 4. {
                    assert!(checked > 100 && clipped > 100);
                }
            }
        }
    }
}
