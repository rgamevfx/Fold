#![cfg(feature = "spatial")]
use fold_foundation::ObjectId;
use fold_render::{
    gpu::{Host, spatial::Adapter},
    spatial::{plane::Sizing, *},
};
use std::sync::Arc;
#[path = "support/spatial.rs"]
mod support;

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn camera_planes_match_declared_pixel_bounds_and_share_world_depth() {
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!("Camera-plane mapping/depth: {info:?}; RGBA16F; MSAA off");
    let mut adapter = Adapter::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let (mut scene, mut binding) = support::fixture();
    binding.dimensions = [160, 120];
    scene.meshes.clear();
    scene.materials.clear();
    scene.instances.clear();
    let ids = [ObjectId::new(), ObjectId::new()];
    let colors = [[1., 0., 0., 1.], [0., 1., 0., 1.]];
    let rectangles = [[[50., 25.], [150., 75.]], [[50., 25.], [100., 75.]]];
    let mut planar = planar::Cache::default();
    for i in 0..2 {
        scene.materials.insert(
            ids[i],
            Material {
                color: colors[i],
                unlit: true,
                roughness: 1.,
                reflectance: 0.,
            },
        );
        scene.instances.push(Instance {
            id: ids[i],
            mesh: ids[i],
            material: ids[i],
            transform: Default::default(),
        });
    }
    for projection in [
        Projection::Perspective {
            vertical_fov: std::f32::consts::FRAC_PI_2,
        },
        Projection::Orthographic { vertical_size: 4. },
    ] {
        scene.camera.projection = projection;
        for (position, up) in [([0., 0., 4.], [0., 1., 0.]), ([4., 0., 0.], [0., 0., 1.])] {
            scene.camera.position = position;
            scene.camera.up = up;
            for sizing in [Sizing::Viewport, Sizing::MetersPerUnit(0.0123)] {
                for distance in [4., 6.] {
                    let distances = [distance, 2.];
                    for i in 0..2 {
                        let mapping = scene
                            .camera
                            .plane(distances[i], [200, 100], binding.dimensions, sizing)
                            .unwrap();
                        let [[left, top], [right, bottom]] = rectangles[i];
                        let points = [[left, top], [right, top], [right, bottom], [left, bottom]];
                        let drawing = fold_render::vector::Drawing {
                            feather: None,
                            path: Arc::new(vec![
                                fold_render::vector::Segment::Move(points[0]),
                                fold_render::vector::Segment::Line(points[1]),
                                fold_render::vector::Segment::Line(points[2]),
                                fold_render::vector::Segment::Line(points[3]),
                                fold_render::vector::Segment::Close,
                            ]),
                            transform: [1., 0., 0., 1., 0., 0.],
                            fill: Some(colors[i].map(f64::from)),
                            even_odd: false,
                            stroke: None,
                        };
                        let local = planar
                            .prepare_with(&drawing, 1.2, &mut || Ok(()))
                            .unwrap()
                            .fill
                            .unwrap();
                        scene.meshes.insert(
                            ids[i],
                            Arc::new(Mesh {
                                positions: local
                                    .positions
                                    .iter()
                                    .map(|p| {
                                        mapping.point([f64::from(p[0]), f64::from(p[1])]).unwrap()
                                    })
                                    .collect(),
                                normals: vec![mapping.normal(); local.positions.len()],
                                indices: local
                                    .indices
                                    .chunks_exact(3)
                                    .flat_map(|t| [t[0], t[2], t[1]])
                                    .collect(),
                            }),
                        );
                    }
                    binding.revision += 1;
                    binding.camera_content = scene.camera.identity();
                    let mut frame = adapter.render(&scene, binding.clone(), &cancel).unwrap();
                    let pixels = frame.readback(&binding, &cancel).unwrap();
                    let depth = frame.read_depth(&binding, &cancel).unwrap();
                    // Independent projected bounds use the authored rectangle,
                    // not the mapped mesh positions or its prepared bounds.
                    let contains = |i: usize, x: f64, y: f64| {
                        let project = |point: [f64; 2]| match sizing {
                            Sizing::Viewport => [point[0] * 0.8, point[1] * 1.2],
                            Sizing::MetersPerUnit(scale) => {
                                let height = match projection {
                                    Projection::Perspective { .. } => 2. * distances[i],
                                    Projection::Orthographic { .. } => 4.,
                                };
                                [
                                    80. + (point[0] - 100.) * scale * 120. / height,
                                    60. + (point[1] - 50.) * scale * 120. / height,
                                ]
                            }
                        };
                        let a = project(rectangles[i][0]);
                        let b = project(rectangles[i][1]);
                        x > a[0] && x < b[0] && y > a[1] && y < b[1]
                    };
                    for y in 0..120 {
                        for x in 0..160 {
                            let front = contains(1, x as f64 + 0.5, y as f64 + 0.5);
                            let back = contains(0, x as f64 + 0.5, y as f64 + 0.5);
                            let expected = if front {
                                colors[1]
                            } else if back {
                                colors[0]
                            } else {
                                [0.; 4]
                            };
                            assert_eq!(
                                pixels.pixels()[y * 160 + x],
                                expected,
                                "{projection:?} {position:?} {sizing:?} distance={distance} ({x},{y})"
                            );
                            if front || back {
                                let expected = if front { 2. } else { distance as f32 };
                                assert!(
                                    (depth.view_distance(x as u32, y as u32).unwrap() - expected)
                                        .abs()
                                        < 2e-5
                                );
                            } else {
                                assert_eq!(depth.view_distance(x as u32, y as u32), None);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn camera_plane_oblique_views_match_ray_plane_coverage_and_clipping() {
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!("Oblique Camera Plane: {info:?}; analytic rays; RGBA16F; MSAA off");
    let mut adapter = Adapter::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let (mut scene, mut binding) = support::fixture();
    binding.dimensions = [160, 120];
    scene.meshes.clear();
    scene.materials.clear();
    scene.instances.clear();
    let mut attached = scene.camera.clone();
    attached.position = [0., 0., 4.];
    attached.target = [0., 0., 0.];
    attached.up = [0., 1., 0.];
    let mapping = attached
        .plane(2., [200, 100], [160, 120], Sizing::MetersPerUnit(0.01))
        .unwrap();
    let id = ObjectId::new();
    scene.meshes.insert(
        id,
        Arc::new(Mesh {
            positions: [[0., 0.], [200., 0.], [200., 100.], [0., 100.]]
                .map(|p| mapping.point(p).unwrap())
                .to_vec(),
            normals: vec![[0., 0., 1.]; 4],
            indices: vec![0, 2, 1, 0, 3, 2],
        }),
    );
    scene.materials.insert(
        id,
        Material {
            color: [1., 0., 0., 1.],
            unlit: true,
            roughness: 1.,
            reflectance: 0.,
        },
    );
    scene.instances.push(Instance {
        id,
        mesh: id,
        material: id,
        transform: Default::default(),
    });
    let mut total = 0;
    let mut painted = 0;
    for orthographic in [false, true] {
        for yaw in [0.37f64, 1.13] {
            for z in [4f32, 2.073, 1.927] {
                scene.camera.position = [0., 0., z];
                scene.camera.target = [yaw.sin() as f32, 0., z - yaw.cos() as f32];
                scene.camera.up = [0., 1., 0.];
                scene.camera.near = 0.1;
                scene.camera.far = 50.;
                scene.camera.projection = if orthographic {
                    Projection::Orthographic { vertical_size: 2. }
                } else {
                    Projection::Perspective {
                        vertical_fov: std::f32::consts::FRAC_PI_2,
                    }
                };
                binding.camera_content = scene.camera.identity();
                let mut frame = adapter.render(&scene, binding.clone(), &cancel).unwrap();
                let pixels = frame.readback(&binding, &cancel).unwrap();
                let depth = frame.read_depth(&binding, &cancel).unwrap();
                let dx = f64::from(scene.camera.target[0]);
                let dz = f64::from(scene.camera.target[2]) - f64::from(z);
                let length = dx.hypot(dz);
                let (sin, cos) = (dx / length, -dz / length);
                for y in 0..120 {
                    for x in 0..160 {
                        let nx = (2. * (x as f64 + 0.5) / 160. - 1.) * 4. / 3.;
                        let ny = 1. - 2. * (y as f64 + 0.5) / 120.;
                        let focal = (f64::from(std::f32::consts::FRAC_PI_2) / 2.).tan();
                        let (origin, ray) = if orthographic {
                            ([cos * nx, ny, f64::from(z) + sin * nx], [sin, 0., -cos])
                        } else {
                            (
                                [0., 0., f64::from(z)],
                                [sin + cos * nx * focal, ny * focal, -cos + sin * nx * focal],
                            )
                        };
                        let t = (2. - origin[2]) / ray[2];
                        let p = [origin[0] + t * ray[0], origin[1] + t * ray[1]];
                        let visible = ray[2] < 0.
                            && t >= f64::from(scene.camera.near)
                            && t <= 50.
                            && p[0].abs() < 1.
                            && p[1].abs() < 0.5;
                        let expected = if visible { [1., 0., 0., 1.] } else { [0.; 4] };
                        assert_eq!(
                            pixels.pixels()[y * 160 + x],
                            expected,
                            "ortho={orthographic} yaw={yaw} z={z} ({x},{y}) p={p:?} t={t}"
                        );
                        if visible {
                            painted += 1;
                            // Raster vertices quantize to subpixels. On a sloped
                            // plane that perturbs interpolated depth; bound it
                            // with independent rays one 1/256 pixel either side.
                            let delta = 2. / (160. * 256.) * 4. / 3.;
                            let at = |offset: f64| {
                                if orthographic {
                                    (2. - f64::from(z) - sin * (nx + offset)) / -cos
                                } else {
                                    (2. - f64::from(z)) / (-cos + sin * (nx + offset) * focal)
                                }
                            };
                            let interval = [at(-delta), at(delta)];
                            let actual =
                                f64::from(depth.view_distance(x as u32, y as u32).unwrap());
                            assert!(
                                actual >= interval[0].min(interval[1]) - 2e-5
                                    && actual <= interval[0].max(interval[1]) + 2e-5,
                                "depth ortho={orthographic} yaw={yaw} z={z} ({x},{y}) expected={t} interval={interval:?} actual={actual}"
                            );
                        } else {
                            assert_eq!(depth.view_distance(x as u32, y as u32), None);
                        }
                        total += 1;
                    }
                }
            }
        }
    }
    assert!(painted > 1000);
    eprintln!("Oblique ray coverage: {total} pixels; {painted} painted");
}
