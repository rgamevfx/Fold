#![cfg(feature = "spatial")]
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_render::gpu::{Host, Renderer, spatial::*};
use std::sync::Arc;

#[path = "support/spatial.rs"]
mod support;
use support::fixture;

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn retained_text_mesh_and_extrusion_match_vector_interiors_and_depth() {
    use fold_render::{
        ImageOp, RenderGraph,
        spatial::{Instance, Transform},
        text::{Service, mesh, outlines},
    };
    let cancel = fold_media::Cancel::default();
    let layout = Service::new(
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        "Noto Sans",
    )
    .unwrap()
    .layout("O B 8 O", 140., None)
    .unwrap();
    let outlines = Arc::new(outlines::prepare(layout.clone(), &cancel).unwrap());
    let prepared = mesh::Cache::default()
        .prepare(outlines.clone(), 1., &cancel)
        .unwrap();
    assert!(Arc::ptr_eq(&prepared.outlines.layout, &layout));
    let cpu = fold_render::render(RenderGraph {
        width: 640,
        height: 220,
        nodes: vec![ImageOp::Vector(Arc::new(
            outlines.drawings(Some([1.; 4]), None),
        ))],
        output: 0,
    })
    .unwrap();
    let (mut scene, mut binding) = fixture();
    binding.dimensions = [640, 220];
    scene.meshes.clear();
    scene.instances.clear();
    let material = *scene.materials.keys().next().unwrap();
    scene.materials.get_mut(&material).unwrap().color = [1.; 4];
    let fold_render::spatial::Projection::Perspective { vertical_fov } = scene.camera.projection
    else {
        panic!("perspective oracle required")
    };
    let unit = 2. * 4. * (vertical_fov / 2.).tan() / 220.;
    for glyph in &prepared.glyphs {
        let Some(mesh) = &glyph.mesh else { continue };
        let id = scene
            .meshes
            .iter()
            .find(|(_, m)| Arc::ptr_eq(m, mesh))
            .map(|(id, _)| *id)
            .unwrap_or_else(ObjectId::new);
        scene.meshes.insert(id, mesh.clone());
        assert_eq!(&glyph.transform[..4], &[1., 0., 0., -1.]);
        scene.instances.push(Instance {
            id: ObjectId::new(),
            mesh: id,
            material,
            transform: Transform {
                translation: [
                    (glyph.transform[4] as f32 - 320.) * unit,
                    (110. - glyph.transform[5] as f32) * unit,
                    0.,
                ],
                scale: [unit, unit, 1.],
                ..Transform::default()
            },
        });
    }
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!(
        "M4 planar spatial text: {info:?}; font={:02x?}; RGBA16F; MSAA off",
        layout.font_hash
    );
    let mut adapter = Adapter::new(host).unwrap();
    let mut frame = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(frame.preparation.meshes, 3);
    assert_eq!(frame.preparation.spawned_instances, 4);
    let pixels = frame.readback(&binding, &cancel).unwrap();
    let depth = frame.read_depth(&binding, &cancel).unwrap();
    let mut solid = 0;
    let mut clear = 0;
    // Only compare interiors: analytic vector coverage and MSAA-off spatial edges differ.
    for y in 1..219 {
        for x in 1..639 {
            let alpha = cpu.pixels()[y * 640 + x][3];
            if alpha != 0. && alpha != 1. {
                continue;
            }
            if !(y - 1..=y + 1)
                .all(|yy| (x - 1..=x + 1).all(|xx| cpu.pixels()[yy * 640 + xx][3] == alpha))
            {
                continue;
            }
            assert_eq!(
                pixels.pixels()[y * 640 + x],
                [alpha; 4],
                "text interior at {x},{y}"
            );
            if alpha == 1. {
                assert!((depth.view_distance(x as u32, y as u32).unwrap() - 4.).abs() < 1e-5);
                solid += 1;
            } else {
                assert_eq!(depth.values()[y * 640 + x], 0.);
                clear += 1;
            }
        }
    }
    assert!(solid > 3000 && clear > 10000);
    let retained = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(retained.preparation.meshes, 0);
    assert_eq!(retained.preparation.updated_instances, 0);
    eprintln!(
        "matched {solid} opaque and {clear} clear interior pixels; axial depth=4; retained three meshes/four glyph instances"
    );
    let solids = fold_render::text::extrude::prepare(
        Arc::new(prepared),
        f64::from(unit),
        0.15,
        3,
        &mut || {
            cancel
                .check()
                .map_err(|_| fold_render::spatial::extrude::Error::Cancelled)
        },
    )
    .unwrap();
    for (instance, glyph) in scene
        .instances
        .iter_mut()
        .zip(solids.glyphs.iter().filter(|g| g.solid.is_some()))
    {
        let solid = glyph.solid.as_ref().unwrap();
        scene.meshes.insert(instance.mesh, solid.mesh.clone());
        instance.transform.scale = [1.; 3];
        instance.transform.translation = [
            (glyph.transform[4] - 320. * f64::from(unit)) as f32,
            (glyph.transform[5] + 110. * f64::from(unit)) as f32,
            0.,
        ];
    }
    binding.revision += 1;
    let mut front = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(front.preparation.meshes, 3);
    assert_eq!(front.preparation.spawned_instances, 0);
    check_extrusion_rays(&scene, &binding, &mut front, &cancel);
    // Back camera is four units from the back cap. Its horizontal axis is reversed.
    scene.camera.position[2] = -4.15;
    scene.camera.target[2] = -0.15;
    binding.camera_content = scene.camera.identity();
    let mut back = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(back.preparation.meshes, 0);
    check_extrusion_rays(&scene, &binding, &mut back, &cancel);
}

#[path = "support/spatial_rays.rs"]
mod spatial_rays;
use spatial_rays::check_extrusion_rays;

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn spatial_retains_resources_and_immutable_revision_bound_frames() {
    let (host, info) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    eprintln!("M4 Fold spatial adapter: {info:?}");
    let mut adapter = Adapter::new(host.clone()).unwrap();
    let (mut scene, binding) = fixture();
    let cancel = fold_media::Cancel::default();
    let mut held = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(held.preparation.meshes, 1);
    assert_eq!(held.preparation.materials, 1);
    assert_eq!(held.preparation.spawned_instances, 1);
    assert_eq!(held.image(&binding).unwrap().statistics.readback_bytes, 0);
    let first = held.readback(&binding, &cancel).unwrap();
    assert_eq!(first.pixels()[32 * 64 + 32], [2., 0.25, 0.125, 1.]);
    assert_eq!(first.pixels()[0], [0.; 4]);
    let mut next = binding.clone();
    next.revision += 1;
    next.time = Time::new(-1, 24).unwrap();
    scene.materials.values_mut().next().unwrap().color = [0.125, 0.25, 2., 0.5];
    let mut updated = adapter.render(&scene, next.clone(), &cancel).unwrap();
    assert_eq!(
        updated.preparation.meshes, 0,
        "material changes preserve geometry"
    );
    assert_eq!(updated.preparation.materials, 1);
    assert_eq!(updated.preparation.spawned_instances, 0);
    assert!(updated.image(&binding).is_err());
    assert!(held.image(&next).is_err());
    let pixels = updated.readback(&next, &cancel).unwrap();
    assert_eq!(pixels.pixels()[32 * 64 + 32], [0.0625, 0.125, 1., 0.5]);
    assert_eq!(
        held.readback(&binding, &cancel).unwrap().pixels(),
        first.pixels(),
        "held frame must not be overwritten"
    );
    // Reconstructed snapshots with equal mesh content also retain GPU geometry.
    for mesh in scene.meshes.values_mut() {
        *mesh = Arc::new((**mesh).clone());
    }
    let current = adapter.render(&scene, next.clone(), &cancel).unwrap();
    assert_eq!(current.preparation.meshes, 0);
    assert_eq!(current.preparation.materials, 0);
    assert_eq!(current.preparation.spawned_instances, 0);
    assert_eq!(current.preparation.updated_instances, 0);
    let mut moved_camera = scene.camera.clone();
    moved_camera.position[0] += 1.;
    assert!(
        current
            .image(&Binding {
                camera_content: moved_camera.identity(),
                ..next.clone()
            })
            .is_err()
    );
    // Exact rational time, camera, dimensions and document all participate in publication.
    for bad in [
        Binding {
            document: DocumentId::new(),
            ..next.clone()
        },
        Binding {
            camera: ObjectId::new(),
            ..next.clone()
        },
        Binding {
            dimensions: [32, 64],
            ..next.clone()
        },
        Binding {
            time: Time::ZERO,
            ..next.clone()
        },
    ] {
        assert!(current.image(&bad).is_err());
    }
    let mut invalid = scene.clone();
    invalid.instances[0].transform.rotation = [0.; 4];
    assert!(adapter.render(&invalid, next.clone(), &cancel).is_err());
    invalid = scene.clone();
    invalid.instances[0].mesh = ObjectId::new();
    assert!(adapter.render(&invalid, next.clone(), &cancel).is_err());
    let cancelled = fold_media::Cancel::default();
    cancelled.cancel();
    assert!(adapter.render(&scene, next.clone(), &cancelled).is_err());
    let mut good = adapter.render(&scene, next.clone(), &cancel).unwrap();
    assert_eq!(good.preparation.meshes, 0);
    assert_eq!(good.preparation.materials, 0);
    assert_eq!(
        good.readback(&next, &cancel).unwrap().pixels(),
        pixels.pixels()
    );
    for (x, time) in [(2., Time::new(5, 24).unwrap()), (0., next.time)] {
        scene.instances[0].transform.translation[0] = x;
        let sample = Binding {
            time,
            ..next.clone()
        };
        let mut frame = adapter.render(&scene, sample.clone(), &cancel).unwrap();
        assert_eq!(frame.preparation.meshes, 0);
        assert_eq!(frame.preparation.updated_instances, 1);
        assert_eq!(frame.preparation.spawned_instances, 0);
        let center = frame.readback(&sample, &cancel).unwrap().pixels()[32 * 64 + 32];
        assert_eq!(
            center,
            if x == 0. {
                [0.0625, 0.125, 1., 0.5]
            } else {
                [0.; 4]
            }
        );
    }
    // The existing output converter consumes the same host/frame path.
    let mut output = Renderer::new(host.clone()).unwrap();
    let displayed = output
        .output(good.image(&next).unwrap(), None, &cancel)
        .unwrap()
        .readback(&cancel)
        .unwrap();
    let expected = pixels.over_black().to_display().unwrap();
    assert!(
        displayed
            .rgba()
            .iter()
            .zip(expected.rgba())
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    // The adapter's lit path uses the same explicit exposure as the probe.
    let material = scene.materials.values_mut().next().unwrap();
    material.color = [0.5, 0.5, 0.5, 1.];
    material.unlit = false;
    scene.lights = vec![DirectionalLight {
        id: ObjectId::new(),
        direction: [0., 0., -1.],
        color: [1.; 3],
        illuminance: 1000.,
    }];
    let lit_binding = Binding {
        revision: 3,
        ..next.clone()
    };
    let mut lit = adapter
        .render(&scene, lit_binding.clone(), &cancel)
        .unwrap();
    let center = lit.readback(&lit_binding, &cancel).unwrap().pixels()[32 * 64 + 32];
    let expected_gray = 1000. * 0.5 / std::f32::consts::PI * 2f32.powf(-8.) / 1.2;
    assert!((center[0] - expected_gray).abs() <= 0.002);
    assert_eq!(center[0], center[1]);
    assert_eq!(center[1], center[2]);
    scene.camera.ev100 = -20.;
    scene.lights[0].illuminance = 100_000.;
    let overflow_binding = Binding {
        revision: 4,
        camera_content: scene.camera.identity(),
        ..next.clone()
    };
    let mut overflow = adapter
        .render(&scene, overflow_binding.clone(), &cancel)
        .unwrap();
    assert!(
        overflow
            .readback(&overflow_binding, &cancel)
            .err()
            .unwrap()
            .contains("RGBA16F")
    );
    scene.camera.ev100 = 8.;
    scene.lights[0].illuminance = 1000.;
    let mut recovered = adapter
        .render(&scene, lit_binding.clone(), &cancel)
        .unwrap();
    assert_eq!(
        recovered.readback(&lit_binding, &cancel).unwrap().pixels()[32 * 64 + 32],
        center
    );
    let mesh = Arc::make_mut(scene.meshes.values_mut().next().unwrap());
    for position in &mut mesh.positions {
        position[0] += 3.;
    }
    let geometry_binding = Binding {
        revision: 5,
        ..next.clone()
    };
    let mut changed = adapter
        .render(&scene, geometry_binding.clone(), &cancel)
        .unwrap();
    assert_eq!(changed.preparation.meshes, 1);
    assert_eq!(changed.preparation.spawned_instances, 0);
    assert_eq!(
        changed
            .readback(&geometry_binding, &cancel)
            .unwrap()
            .pixels()[32 * 64 + 32],
        [0.; 4]
    );
    scene.instances.clear();
    scene.meshes.clear();
    scene.materials.clear();
    let empty_binding = Binding {
        revision: 6,
        ..next.clone()
    };
    let mut empty = adapter
        .render(&scene, empty_binding.clone(), &cancel)
        .unwrap();
    assert_eq!(empty.preparation.removed_instances, 1);
    assert!(
        empty
            .readback(&empty_binding, &cancel)
            .unwrap()
            .pixels()
            .iter()
            .all(|pixel| *pixel == [0.; 4])
    );
    drop(adapter);
    assert_eq!(
        held.readback(&binding, &cancel).unwrap().pixels(),
        first.pixels()
    );
    host.device().destroy();
    let _ = host.poll();
    assert!(held.image(&binding).unwrap().is_ready().is_err());
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn spatial_depth_is_bound_immutable_and_uses_declared_reverse_z() {
    let (host, info) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    eprintln!("M4 depth adapter: {info:?}");
    let mut adapter = Adapter::new(host.clone()).unwrap();
    let (mut scene, mut binding) = fixture();
    binding.dimensions = [19, 13]; // exercises padded readback rows
    let mut farther = scene.instances[0].clone();
    farther.id = ObjectId::new();
    farther.transform.translation[2] = -1.;
    scene.instances.push(farther);
    let cancel = fold_media::Cancel::default();
    let mut held = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    assert_eq!(held.image(&binding).unwrap().statistics.readback_bytes, 0);
    assert_eq!(
        host.memory().working,
        19 * 13 * (8 + 8 + 4),
        "host accounts both color targets and retained depth"
    );
    let first = held.read_depth(&binding, &cancel).unwrap();
    assert_eq!(first.binding(), &binding);
    assert_eq!(first.values().len(), 19 * 13);
    assert!((first.values()[6 * 19 + 9] - 0.1 / 4.).abs() < 1e-6);
    assert!((first.view_distance(9, 6).unwrap() - 4.).abs() < 1e-5);
    assert_eq!(first.view_distance(0, 0), None);
    assert_eq!(first.view_distance(19, 6), None);
    assert_eq!(
        held.image(&binding).unwrap().statistics.readback_bytes,
        256 * 13
    );
    scene.instances[0].transform.translation[0] = 4.;
    let farther_binding = Binding {
        revision: 2,
        ..binding.clone()
    };
    let mut farther = adapter
        .render(&scene, farther_binding.clone(), &cancel)
        .unwrap();
    let second = farther.read_depth(&farther_binding, &cancel).unwrap();
    assert!((second.values()[6 * 19 + 9] - 0.1 / 5.).abs() < 1e-6);
    assert!((second.view_distance(9, 6).unwrap() - 5.).abs() < 1e-5);
    assert!(held.read_depth(&farther_binding, &cancel).is_err());
    assert_eq!(
        held.read_depth(&binding, &cancel).unwrap().values(),
        first.values()
    );

    scene.camera.position[2] = 5.;
    scene.camera.near = 0.2;
    let camera_binding = Binding {
        camera_content: scene.camera.identity(),
        dimensions: [23, 17],
        ..farther_binding.clone()
    };
    let mut changed = adapter
        .render(&scene, camera_binding.clone(), &cancel)
        .unwrap();
    let changed_depth = changed.read_depth(&camera_binding, &cancel).unwrap();
    assert!((changed_depth.view_distance(11, 8).unwrap() - 6.).abs() < 1e-5);
    assert!((changed_depth.values()[8 * 23 + 11] - 0.2 / 6.).abs() < 1e-6);
    assert!(farther.read_depth(&camera_binding, &cancel).is_err());
    assert_eq!(
        held.read_depth(&binding, &cancel).unwrap().values(),
        first.values()
    );
    // Blended surfaces test against opaque depth; they do not claim visibility IDs
    // or overwrite this opaque-surface depth plane.
    scene.materials.values_mut().next().unwrap().color[3] = 0.5;
    let transparent_binding = Binding {
        revision: 3,
        ..camera_binding.clone()
    };
    let mut transparent = adapter
        .render(&scene, transparent_binding.clone(), &cancel)
        .unwrap();
    assert!(
        transparent
            .read_depth(&transparent_binding, &cancel)
            .unwrap()
            .values()
            .iter()
            .all(|value| *value == 0.)
    );
    let cancelled = fold_media::Cancel::default();
    cancelled.cancel();
    assert!(held.read_depth(&binding, &cancelled).is_err());
    drop(adapter);
    assert_eq!(
        held.read_depth(&binding, &cancel).unwrap().values(),
        first.values()
    );
    host.device().destroy();
    let _ = host.poll();
    assert!(held.read_depth(&binding, &cancel).is_err());
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn spatial_depth_follows_camera_updates() {
    let (host, _) = pollster::block_on(Host::headless(8 * 1024 * 1024)).unwrap();
    let mut adapter = Adapter::new(host).unwrap();
    let (mut scene, binding) = fixture();
    let cancel = fold_media::Cancel::default();
    for (near, z, dimensions) in [
        (0.1, 4., [19, 13]),
        (0.2, 4., [19, 13]),
        (0.2, 5., [19, 13]),
        (0.2, 5., [23, 17]),
        (0.01, 5., [23, 17]),
    ] {
        scene.camera.near = near;
        scene.camera.position[2] = z;
        let request = Binding {
            camera_content: scene.camera.identity(),
            dimensions,
            ..binding.clone()
        };
        let mut frame = adapter.render(&scene, request.clone(), &cancel).unwrap();
        let depth = frame.read_depth(&request, &cancel).unwrap();
        let actual = depth
            .view_distance(dimensions[0] / 2, dimensions[1] / 2)
            .unwrap();
        assert!(
            (actual - z).abs() < 1e-5,
            "near={near}, camera_z={z}, dimensions={dimensions:?}: distance={actual}"
        );
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn spatial_image_source_composites_without_readback_and_retains_source_precision() {
    use fold_render::{ImageOp, RenderGraph};
    let (scene, binding) = fixture();
    let source = ImageOp::Spatial {
        scene: Arc::new(scene.clone()),
        binding: binding.clone(),
    };
    let source_graph = RenderGraph {
        width: 64,
        height: 64,
        nodes: vec![source.clone()],
        output: 0,
    };
    assert!(
        fold_render::render(source_graph.clone())
            .unwrap_err()
            .contains("GPU")
    );
    let graph = RenderGraph {
        width: 64,
        height: 64,
        nodes: vec![
            ImageOp::Solid {
                rgba: [0., 0., 1., 1.],
            },
            source,
            ImageOp::Over {
                foreground: 1,
                background: 0,
            },
            ImageOp::Solid {
                rgba: [0., 0.25, 0., 0.25],
            },
            ImageOp::Over {
                foreground: 3,
                background: 2,
            },
        ],
        output: 4,
    };
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("Spatial image IR: {info:?}; retained adapter; source RGBA16F");
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut held = renderer
        .evaluate(graph.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(held.statistics.readback_bytes, 0);
    assert_eq!(held.statistics.cpu_adapter_nodes, 0);
    let pixels = held.readback(&cancel).unwrap();
    assert_eq!(pixels.pixels()[32 * 64 + 32], [1.5, 0.4375, 0.09375, 1.]);
    assert_eq!(pixels.pixels()[0], [0., 0.25, 0.75, 1.]);
    let mut only = renderer
        .evaluate(source_graph.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(
        only.readback(&cancel).unwrap().pixels()[32 * 64 + 32],
        [2., 0.25, 0.125, 1.]
    );
    let mut crop = renderer
        .evaluate_region(
            source_graph.clone(),
            fold_render::region::Region {
                x: 28,
                y: 28,
                width: 8,
                height: 8,
            },
            None,
            &mut decoder,
            &cancel,
            None,
        )
        .unwrap();
    assert_eq!(crop.dimensions(), [8, 8]);
    assert!(
        crop.readback(&cancel)
            .unwrap()
            .pixels()
            .iter()
            .all(|p| *p == [2., 0.25, 0.125, 1.])
    );
    assert!(held.readback(&cancel).unwrap().pixels() == pixels.pixels());
    // Preparing a second scene through the retained adapter must not overwrite
    // the first source's image before graph composition reads it.
    let mut second_scene = scene.clone();
    second_scene.materials.values_mut().next().unwrap().color = [0., 1., 0., 0.5];
    let mut pair = renderer
        .evaluate(
            RenderGraph {
                width: 64,
                height: 64,
                nodes: vec![
                    source_graph.nodes[0].clone(),
                    ImageOp::Spatial {
                        scene: Arc::new(second_scene),
                        binding: binding.clone(),
                    },
                    ImageOp::Over {
                        foreground: 1,
                        background: 0,
                    },
                ],
                output: 2,
            },
            None,
            &mut decoder,
            &cancel,
        )
        .unwrap();
    assert_eq!(pair.statistics.readback_bytes, 0);
    assert_eq!(pair.statistics.cpu_adapter_nodes, 0);
    let pair_pixels = pair.readback(&cancel).unwrap();
    assert_eq!(pair_pixels.pixels()[32 * 64 + 32], [1., 0.625, 0.0625, 1.]);
    assert_eq!(pair_pixels.pixels()[0], [0.; 4]);
    let mut invalid = source_graph.clone();
    invalid.width = 32;
    assert!(
        renderer
            .evaluate(invalid, None, &mut decoder, &cancel)
            .err()
            .unwrap()
            .contains("dimensions")
    );
    let mut saturated = scene;
    saturated.materials.values_mut().next().unwrap().color[0] = 65504.;
    let mut overflow = renderer
        .evaluate(
            RenderGraph {
                width: 64,
                height: 64,
                nodes: vec![
                    ImageOp::Spatial {
                        scene: Arc::new(saturated),
                        binding,
                    },
                    ImageOp::Solid { rgba: [0.; 4] },
                    ImageOp::Over {
                        foreground: 0,
                        background: 1,
                    },
                ],
                output: 2,
            },
            None,
            &mut decoder,
            &cancel,
        )
        .unwrap();
    assert!(
        overflow
            .readback(&cancel)
            .err()
            .unwrap()
            .contains("RGBA16F")
    );
    let mut recovered = renderer
        .evaluate(graph, None, &mut decoder, &cancel)
        .unwrap();
    assert!(recovered.readback(&cancel).unwrap().pixels() == pixels.pixels());
    eprintln!(
        "Spatial IR: ordered over, HDR/alpha, source-only half-float, crop, held image, two retained sources, binding dimensions, descendant overflow and recovery passed"
    );
}

#[test]
#[ignore = "requires native Vulkan and the private OCIO package"]
fn spatial_aces_conversion_matches_cpu_processor_and_preserves_alpha() {
    use fold_render::{ImageOp, RenderGraph, WorkingSpace};
    let root = std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("FOLD_TEST_COLOR_ROOT"),
    );
    let runtime = fold_color::Runtime::at(&root).unwrap();
    let config = runtime.bundled().unwrap();
    let processor = config
        .conversion(fold_color::LINEAR_SRGB, fold_color::WORKING_SPACE)
        .unwrap();
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!(
        "Spatial ACES: {info:?}; config={}",
        config.identity().content_sha256
    );
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for color in [
        [2., 0.25, 0.125, 1.],
        [0.125, 2., 0.5, 0.5],
        [1., 0., 0., 0.],
    ] {
        let (mut scene, binding) = fixture();
        scene.materials.values_mut().next().unwrap().color = color;
        let graph = RenderGraph {
            width: 64,
            height: 64,
            nodes: vec![ImageOp::Spatial {
                scene: Arc::new(scene),
                binding,
            }],
            output: 0,
        };
        let mut linear = renderer
            .evaluate(graph.clone(), None, &mut decoder, &cancel)
            .unwrap();
        let linear_pixels = linear.readback(&cancel).unwrap();
        let mut expected = linear_pixels.pixels().to_vec();
        processor.apply(&mut expected).unwrap();
        let mut aces = renderer
            .evaluate(graph, Some(&config), &mut decoder, &cancel)
            .unwrap();
        assert_eq!(aces.statistics.cpu_adapter_nodes, 0);
        assert_eq!(aces.statistics.readback_bytes, 0);
        let actual = aces.readback(&cancel).unwrap();
        assert_eq!(actual.working_space(), WorkingSpace::AcesCg);
        assert_eq!(
            actual.config_identity(),
            Some(config.identity().content_sha256.as_str())
        );
        let mut max_error = 0f32;
        for (i, (a, e)) in actual.pixels().iter().zip(&expected).enumerate() {
            assert_eq!(a[3], e[3], "alpha changed at {i}");
            for channel in 0..3 {
                max_error = max_error.max((a[channel] - e[channel]).abs());
            }
        }
        assert!(max_error <= 0.00001, "ACES conversion error {max_error}");
        let display = config
            .display(fold_color::WORKING_SPACE, &Default::default())
            .unwrap();
        let cpu = actual.over_black().to_output(&display).unwrap();
        let mut gpu = renderer.output(&aces, Some(&display), &cancel).unwrap();
        let gpu = gpu.readback(&cancel).unwrap();
        assert_eq!(gpu.transform_identity(), cpu.transform_identity());
        assert!(
            gpu.rgba()
                .iter()
                .zip(cpu.rgba())
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
        assert!(linear.readback(&cancel).unwrap().pixels() == linear_pixels.pixels());
        eprintln!(
            "Spatial ACES {color:?}: max CPU/GPU error {max_error}; exact alpha; display <=1 byte; retained Rec.709 source"
        );
    }
    // A gamut matrix can turn saturated source red into finite, smaller channel
    // values. It must still retain the source's failed precision validation.
    let (mut scene, binding) = fixture();
    scene.materials.values_mut().next().unwrap().color = [65504., 0., 0., 1.];
    let mut overflow = renderer
        .evaluate(
            RenderGraph {
                width: 64,
                height: 64,
                nodes: vec![ImageOp::Spatial {
                    scene: Arc::new(scene),
                    binding,
                }],
                output: 0,
            },
            Some(&config),
            &mut decoder,
            &cancel,
        )
        .unwrap();
    assert!(
        overflow
            .readback(&cancel)
            .err()
            .unwrap()
            .contains("RGBA16F")
    );
    eprintln!("Spatial ACES retains upstream half-float saturation errors after gamut conversion");
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn orthographic_projected_bounds_depth_clipping_and_retained_frames() {
    let (mut scene, mut binding) = fixture();
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!("Orthographic projection/depth: {info:?}; RGBA16F; MSAA off");
    let cancel = fold_media::Cancel::default();
    let mut adapter = Adapter::new(host).unwrap();
    let mut held: Option<(Frame, Binding, Vec<[f32; 4]>)> = None;
    let ortho = |vertical_size| Projection::Orthographic { vertical_size };
    let perspective = Projection::Perspective {
        vertical_fov: std::f32::consts::FRAC_PI_4,
    };
    for (index, (projection, z, dimensions, near, far)) in [
        (perspective, 4., [128, 64], 0.1, 100.),
        (ortho(4.), 4., [128, 64], 0.1, 100.),
        (ortho(4.), 8., [128, 64], 0.1, 100.),
        (ortho(2.), 8., [128, 64], 0.1, 100.),
        (ortho(4.), 4., [128, 96], 0.1, 100.),
        (ortho(4.), 4., [128, 64], 5., 100.),
        (ortho(4.), 4., [128, 64], 0.1, 3.),
        (ortho(4.), 4., [128, 64], 0.1, 100.),
        (perspective, 4., [128, 64], 0.1, 100.),
    ]
    .into_iter()
    .enumerate()
    {
        scene.camera.projection = projection;
        scene.camera.position[2] = z;
        scene.camera.near = near;
        scene.camera.far = far;
        binding.revision = index as u64 + 1;
        binding.dimensions = dimensions;
        binding.camera_content = scene.camera.identity();
        let mut frame = adapter.render(&scene, binding.clone(), &cancel).unwrap();
        assert_eq!(frame.preparation.meshes, usize::from(index == 0));
        assert_eq!(frame.preparation.spawned_instances, usize::from(index == 0));
        let pixels = frame.readback(&binding, &cancel).unwrap();
        let depth = frame.read_depth(&binding, &cancel).unwrap();
        let [width, height] = dimensions;
        let half_extent = match projection {
            Projection::Orthographic { vertical_size } => height as f32 / vertical_size,
            Projection::Perspective { vertical_fov } => {
                height as f32 / (2. * z * (vertical_fov / 2.).tan())
            }
        };
        for y in 0..height {
            for x in 0..width {
                let inside = ((x as f32 + 0.5) - width as f32 / 2.).abs() < half_extent
                    && ((y as f32 + 0.5) - height as f32 / 2.).abs() < half_extent
                    && z >= near
                    && z < far;
                let pixel = pixels.pixels()[(y * width + x) as usize];
                assert!(pixel.iter().all(|v| v.is_finite()));
                assert_eq!(
                    pixel[3],
                    if inside { 1. } else { 0. },
                    "case {index}: ({x},{y})"
                );
                if inside {
                    assert!(
                        (depth.view_distance(x, y).unwrap() - z).abs() < 2e-5,
                        "orthographic depth case {index}"
                    );
                } else {
                    assert_eq!(depth.view_distance(x, y), None);
                }
            }
        }
        if let Some((old, old_binding, old_pixels)) = &mut held {
            assert_eq!(
                old.readback(old_binding, &cancel).unwrap().pixels(),
                old_pixels.as_slice()
            );
            assert!(
                (old.read_depth(old_binding, &cancel)
                    .unwrap()
                    .view_distance(64, 32)
                    .unwrap()
                    - 4.)
                    .abs()
                    < 2e-5
            );
            assert!(old.image(&binding).is_err());
        } else {
            held = Some((frame, binding.clone(), pixels.pixels().to_vec()));
        }
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn affine_shear_matches_explicit_geometry_and_retains_meshes() {
    use fold_foundation::affine::Affine;
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut adapter = Adapter::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let (mut scene, mut binding) = fixture();
    let angle = std::f64::consts::FRAC_PI_8;
    let matrix = Affine::trs([0.1, 0., 0.], [0., 0., 0., 1.], [1.2, 0.6, 1.]).compose(Affine::trs(
        [0.; 3],
        [0., 0., angle.sin(), angle.cos()],
        [1.; 3],
    ));
    scene.instances[0].transform.affine = Some(matrix);
    let mut rendered = adapter.render(&scene, binding.clone(), &cancel).unwrap();
    let actual = rendered.readback(&binding, &cancel).unwrap();
    assert!(actual.pixels().iter().filter(|p| p[3] > 0.5).count() > 200);
    let source_mesh = scene.meshes.values().next().unwrap().clone();
    binding.revision += 1;
    let mut mesh = (*source_mesh).clone();
    for position in &mut mesh.positions {
        *position = matrix.point(position.map(f64::from)).map(|v| v as f32);
    }
    // This XY shear leaves the plane's Z normal unchanged.
    scene.meshes.insert(scene.instances[0].mesh, Arc::new(mesh));
    scene.instances[0].transform = Transform::default();
    let expected = adapter
        .render(&scene, binding.clone(), &cancel)
        .unwrap()
        .readback(&binding, &cancel)
        .unwrap();
    assert_eq!(actual.pixels(), expected.pixels());
    // Matrix-only changes retain the source mesh allocation and update instances.
    scene.meshes.insert(scene.instances[0].mesh, source_mesh);
    binding.revision += 1;
    scene.instances[0].transform.affine = Some(matrix);
    adapter.render(&scene, binding.clone(), &cancel).unwrap();
    binding.revision += 1;
    scene.instances[0].transform.affine =
        Some(Affine::trs([0.2, 0., 0.], [0., 0., 0., 1.], [1.; 3]).compose(matrix));
    let updated = adapter.render(&scene, binding, &cancel).unwrap();
    assert_eq!(updated.preparation.meshes, 0);
    assert_eq!(updated.preparation.updated_instances, 1);
}
