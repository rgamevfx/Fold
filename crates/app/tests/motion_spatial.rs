#![cfg(feature = "spatial")]
//! Motion document → snapshot evaluation → shared GPU adapter acceptance.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    model::{Construction, Node, Parameter, Reference},
    registry, rendering,
    types::{Literal, Space, Unit},
};
use fold_project::{EditBatch, Mutation, Project};
use fold_render::gpu::{Host, spatial::Adapter};

#[path = "../../render/tests/support/spatial_rays.rs"]
mod spatial_rays;

fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
fn literal(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn node(key: &str, mode: &str, name: &str, bindings: Vec<(&str, Binding)>) -> Node {
    registry::resolve(&format!("fold.motion.{key}"), 1, mode)
        .unwrap()
        .node(
            name,
            bindings.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )
        .unwrap()
}
fn position(value: [f64; 3]) -> Binding {
    literal(Literal::Vector {
        value: value.to_vec(),
        unit: Unit::Meters,
        space: Space::World,
    })
}
fn fixture(id: DocumentId) -> (Construction, ObjectId, ObjectId) {
    let mut c = Construction::from_document(&fold_motion::package::create(id).unwrap()).unwrap();
    let unit = 2. * 4. * (std::f64::consts::FRAC_PI_4 / 2.).tan() / 220.;
    let text = node(
        "text",
        "packaged",
        "Text",
        vec![(
            "text",
            literal(Literal::Text {
                value: "O B 8 O".into(),
            }),
        )],
    );
    let outline = node(
        "outline_text",
        "curves",
        "Outlines",
        vec![("text", link(text.id))],
    );
    let extrude = node(
        "extrude",
        "straight_text",
        "Extrude",
        vec![
            ("profile", link(outline.id)),
            (
                "canvas_unit",
                literal(Literal::Distance {
                    value: unit,
                    unit: Unit::Meters,
                }),
            ),
            ("segments", literal(Literal::Integer { value: 3 })),
        ],
    );
    let extrude_id = extrude.id;
    let material = node(
        "material",
        "unlit",
        "Surface",
        vec![("geometry", link(extrude.id))],
    );
    let world = node(
        "place",
        "world",
        "World",
        vec![
            ("geometry", link(material.id)),
            ("position", position([-320. * unit, 110. * unit, 0.])),
        ],
    );
    let camera = node(
        "camera",
        "perspective",
        "Camera",
        vec![("position", position([0., 0., 4.]))],
    );
    let camera_id = camera.id;
    let light = node("light", "directional", "Sun", vec![]);
    let scene = node(
        "scene",
        "spatial",
        "Scene",
        vec![
            ("camera", link(camera.id)),
            (
                "children",
                Binding::List {
                    values: vec![link(world.id), link(light.id)],
                },
            ),
        ],
    );
    c.definitions[0].inputs.clear();
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0].nodes = vec![
        text, outline, extrude, material, world, camera, light, scene,
    ];
    (c, extrude_id, camera_id)
}
fn commit(project: &mut Project, c: &Construction, id: DocumentId) {
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
}
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn motion_document_spatial_frames_match_ray_oracle_and_retain_snapshot_binding() {
    let id = DocumentId::new();
    let (mut c, extrude, camera) = fixture(id);
    let output = c.definitions[0].outputs[0].id;
    let mut project = Project::new(8);
    commit(&mut project, &c, id);
    let held_snapshot = project.snapshot();
    let cancel = fold_media::Cancel::default();
    let prepared = rendering::spatial(
        &held_snapshot,
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| cancel.check(),
    )
    .unwrap();
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!(
        "Motion document spatial acceptance: {info:?}; revision={}; time=0/1",
        prepared.binding.revision
    );
    let mut adapter = Adapter::new(host).unwrap();
    let mut held = adapter
        .render(&prepared.scene, prepared.binding.clone(), &cancel)
        .unwrap();
    assert_eq!(held.preparation.meshes, 3);
    assert_eq!(held.preparation.spawned_instances, 4);
    spatial_rays::check_extrusion_rays(&prepared.scene, &prepared.binding, &mut held, &cancel);
    let original = held.readback(&prepared.binding, &cancel).unwrap();
    let again = adapter
        .render(&prepared.scene, prepared.binding.clone(), &cancel)
        .unwrap();
    assert_eq!(again.preparation.meshes, 0);
    assert_eq!(again.preparation.updated_instances, 0);

    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == extrude)
        .unwrap()
        .parameters
        .insert(
            "depth".into(),
            Parameter::new(literal(Literal::Distance {
                value: 0.3,
                unit: Unit::Meters,
            }))
            .unwrap(),
        );
    commit(&mut project, &c, id);
    let changed = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        Time::new(1, 2).unwrap(),
        [640, 220],
        Default::default(),
        &|| cancel.check(),
    )
    .unwrap();
    assert_ne!(changed.binding.revision, prepared.binding.revision);
    assert_ne!(changed.binding.time, prepared.binding.time);
    assert_eq!(changed.scene.instances, prepared.scene.instances);
    let mut front = adapter
        .render(&changed.scene, changed.binding.clone(), &cancel)
        .unwrap();
    assert_eq!(front.preparation.meshes, 3);
    assert_eq!(front.preparation.spawned_instances, 0);
    spatial_rays::check_extrusion_rays(&changed.scene, &changed.binding, &mut front, &cancel);
    assert!(held.readback(&changed.binding, &cancel).is_err());
    let retained = held.readback(&prepared.binding, &cancel).unwrap();
    assert!(
        original.pixels() == retained.pixels(),
        "Held frame pixels changed after a newer revision"
    );
    let retained_scene = rendering::spatial(
        &held_snapshot,
        id,
        output,
        Time::ZERO,
        [640, 220],
        Default::default(),
        &|| cancel.check(),
    )
    .unwrap();
    assert_eq!(retained_scene.binding, prepared.binding);

    let camera = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == camera)
        .unwrap();
    camera.parameters.insert(
        "position".into(),
        Parameter::new(position([0., 0., -4.3])).unwrap(),
    );
    camera.parameters.insert(
        "target".into(),
        Parameter::new(position([0., 0., -0.3])).unwrap(),
    );
    commit(&mut project, &c, id);
    let reversed = rendering::spatial(
        &project.snapshot(),
        id,
        output,
        Time::new(1, 2).unwrap(),
        [640, 220],
        Default::default(),
        &|| cancel.check(),
    )
    .unwrap();
    assert_ne!(
        reversed.binding.camera_content,
        changed.binding.camera_content
    );
    let mut back = adapter
        .render(&reversed.scene, reversed.binding.clone(), &cancel)
        .unwrap();
    assert_eq!(back.preparation.meshes, 0);
    spatial_rays::check_extrusion_rays(&reversed.scene, &reversed.binding, &mut back, &cancel);
    // Exercise authored material, light and exposure edits through fresh snapshots.
    // Coverage is compared to the ray-qualified front view; radiance is checked
    // independently through zero illumination and a one-stop exposure change.
    let camera_node = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Camera")
        .unwrap();
    camera_node.parameters.insert(
        "position".into(),
        Parameter::new(position([0., 0., 4.])).unwrap(),
    );
    camera_node.parameters.insert(
        "target".into(),
        Parameter::new(position([0., 0., 0.])).unwrap(),
    );
    let surface = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Surface")
        .unwrap();
    let surface_id = surface.id;
    *surface = node(
        "material",
        "lit",
        "Surface",
        vec![("geometry", link(extrude))],
    );
    surface.id = surface_id;
    let light = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Sun")
        .unwrap();
    light.parameters.insert(
        "direction".into(),
        Parameter::new(literal(Literal::Vector {
            value: vec![0., 0., -1.],
            unit: Unit::Unitless,
            space: Space::World,
        }))
        .unwrap(),
    );
    let reference = front.readback(&changed.binding, &cancel).unwrap();
    let mut illuminated = None;
    for (lux, ev) in [(0., 8.), (1000., 8.), (1000., 9.)] {
        for (name, key, value) in [("Sun", "illuminance", lux), ("Camera", "ev100", ev)] {
            c.definitions[0]
                .nodes
                .iter_mut()
                .find(|n| n.name == name)
                .unwrap()
                .parameters
                .insert(
                    key.into(),
                    Parameter::new(literal(Literal::Scalar { value })).unwrap(),
                );
        }
        commit(&mut project, &c, id);
        let lit = rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::new(1, 2).unwrap(),
            [640, 220],
            Default::default(),
            &|| cancel.check(),
        )
        .unwrap();
        let mut frame = adapter
            .render(&lit.scene, lit.binding.clone(), &cancel)
            .unwrap();
        assert_eq!(frame.preparation.meshes, 0);
        assert_eq!(frame.preparation.spawned_instances, 0);
        let pixels = frame.readback(&lit.binding, &cancel).unwrap();
        let mut bright = 0;
        for (index, (actual, original)) in
            pixels.pixels().iter().zip(reference.pixels()).enumerate()
        {
            assert_eq!(
                actual[3], original[3],
                "Lighting changed coverage at {index}"
            );
            if lux == 0. {
                assert!(
                    actual[..3].iter().all(|v| v.abs() <= 1e-6),
                    "Zero-light radiance at {index}"
                );
            } else if ev == 8. {
                bright += usize::from(actual[0] > 0.1);
            } else {
                let previous: &Vec<[f32; 4]> = illuminated.as_ref().unwrap();
                for (channel, value) in actual.iter().enumerate().take(3) {
                    assert!(
                        (*value - previous[index][channel] * 0.5).abs() <= 0.002,
                        "One-stop exposure mismatch at {index}, channel {channel}"
                    );
                }
            }
        }
        if lux > 0. && ev == 8. {
            assert!(
                bright > 5000,
                "Insufficient illuminated glyph pixels: {bright}"
            );
            illuminated = Some(pixels.pixels().to_vec());
        }
        eprintln!(
            "Motion lighting passed: {lux} lux, EV100={ev}, revision={}",
            lit.binding.revision
        );
    }
    eprintln!(
        "Motion snapshot frames passed: revisions {}, {}, {}; times 0/1 and 1/2; depth 0.15m and 0.3m; three shared meshes/four glyph instances",
        prepared.binding.revision, changed.binding.revision, reversed.binding.revision
    );
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn registered_mixed_motion_output_preserves_screen_world_order_and_snapshot_frames() {
    registered_mixed(false);
}
#[test]
#[ignore = "requires native Vulkan and the private OCIO package (FOLD_COLOR_ROOT)"]
fn registered_mixed_motion_aces_preserves_screen_world_order() {
    registered_mixed(true);
}
fn registered_mixed(aces: bool) {
    use fold_app::media_workflow::{SceneRequest, evaluate_scene_gpu};
    use fold_motion::types::{Alpha, Encoding};
    use fold_render::gpu::Renderer;
    let id = DocumentId::new();
    let (mut c, _, _) = fixture(id);
    c.canvas.width = 640;
    c.canvas.height = 220;
    let mut project = if aces {
        fold_app::color::new_project(8)
    } else {
        Project::new(8)
    };
    commit(&mut project, &c, id);
    let source = fold_project::DocumentRef {
        document: id,
        output: c.definitions[0].outputs[0].id.to_string(),
        extensions: Default::default(),
    };
    let request = SceneRequest {
        source,
        time: Time::new(1, 24).unwrap(),
        dimensions: [640, 220],
        region: None,
        preview: None,
    };
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("Registered mixed Motion: {info:?}; ACES={aces}");
    let mut renderer = Renderer::new(host).unwrap();
    let spatial_snapshot = project.snapshot();
    let mut world = evaluate_scene_gpu(
        &spatial_snapshot,
        &request,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    let world_pixels = world.readback(&cancel).unwrap();
    assert!(world_pixels.pixels().iter().any(|p| p[3] > 0.9));
    assert!(world_pixels.pixels().iter().any(|p| p[3] == 0.));
    let shape = node(
        "shape",
        "rectangle",
        "Canvas",
        vec![
            (
                "width",
                literal(Literal::Distance {
                    value: 640.,
                    unit: Unit::Canvas,
                }),
            ),
            (
                "height",
                literal(Literal::Distance {
                    value: 220.,
                    unit: Unit::Canvas,
                }),
            ),
        ],
    );
    let paint = |name, color| {
        node(
            "fill",
            "solid",
            name,
            vec![
                ("geometry", link(shape.id)),
                (
                    "color",
                    literal(Literal::Color {
                        value: color,
                        encoding: Encoding::LinearRec709,
                        alpha: Alpha::Straight,
                    }),
                ),
            ],
        )
    };
    let blue = paint("Background paint", [0., 0., 1., 1.]);
    let green = paint("Foreground paint", [0., 1., 0., 0.25]);
    let background = node(
        "place",
        "screen",
        "Background",
        vec![("geometry", link(blue.id))],
    );
    let foreground = node(
        "place",
        "screen",
        "Foreground",
        vec![("geometry", link(green.id))],
    );
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.insert(0, link(background.id));
    values.push(link(foreground.id));
    scene.parameters.insert(
        "children".into(),
        Parameter::new(Binding::List { values }).unwrap(),
    );
    c.definitions[0]
        .nodes
        .extend([shape, blue, green, background, foreground]);
    commit(&mut project, &c, id);
    let mut mixed = evaluate_scene_gpu(
        &project.snapshot(),
        &request,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert_eq!(mixed.statistics.cpu_adapter_nodes, 0);
    assert_eq!(mixed.statistics.readback_bytes, 0);
    assert!(mixed.statistics.vector_instances >= 2);
    let timing = mixed.descriptor().unwrap().timing.unwrap();
    assert_eq!(timing.timestamp, request.time);
    assert_eq!(timing.duration, Time::new(1, 24).unwrap());
    let actual = mixed.readback(&cancel).unwrap();
    assert_eq!(
        actual.working_space(),
        if aces {
            fold_render::WorkingSpace::AcesCg
        } else {
            fold_render::WorkingSpace::LegacyLinearSrgb
        }
    );
    assert_eq!(actual.config_identity().is_some(), aces);
    assert_eq!(actual.config_identity(), world_pixels.config_identity());
    for (i, (world, mixed)) in world_pixels
        .pixels()
        .iter()
        .zip(actual.pixels())
        .enumerate()
    {
        let background = if aces {
            fold_color::authored::linear_rec709_to_aces([0., 0., 1., 1.]).map(|v| v as f32)
        } else {
            [0., 0., 1., 1.]
        };
        let foreground = if aces {
            fold_color::authored::linear_rec709_to_aces([0., 1., 0., 0.25]).map(|v| v as f32)
        } else {
            [0., 1., 0., 0.25]
        };
        let expected = std::array::from_fn::<_, 4, _>(|channel| {
            if channel == 3 {
                1.
            } else {
                (world[channel] + background[channel] * (1. - world[3])) * 0.75
                    + foreground[channel] * 0.25
            }
        });
        assert!(
            mixed
                .iter()
                .zip(expected)
                .all(|(a, e)| (a - e).abs() < 0.00001),
            "Mixed paint order mismatch at {i}: {mixed:?} != {expected:?}"
        );
    }
    assert!(world.readback(&cancel).unwrap().pixels() == world_pixels.pixels());
    let mut old = evaluate_scene_gpu(
        &spatial_snapshot,
        &request,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert!(old.readback(&cancel).unwrap().pixels() == world_pixels.pixels());
    eprintln!(
        "Registered World and Screen/World/Screen outputs passed: all 140800 pixels match analytic Over; exact timing; no CPU image transfer; held image and old snapshot retained"
    );
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn motion_spatial_preview_worker_matches_direct_output_and_keeps_named_routing() {
    use fold_app::{
        media_workflow::{SceneRequest, evaluate_scene_gpu},
        session::Session,
    };
    use fold_platform::{
        desktop::{DesktopClient, PreviewDemand},
        workspace::PanelInstanceId,
    };
    use fold_render::gpu::Renderer;
    use std::time::{Duration, Instant};
    let id = DocumentId::new();
    let (mut c, _, _) = fixture(id);
    c.canvas.width = 640;
    c.canvas.height = 220;
    let mut project = Project::new(8);
    commit(&mut project, &c, id);
    let snapshot = project.snapshot();
    let reference = fold_app::packages::builtins()
        .default_output(&snapshot, id)
        .unwrap();
    assert_eq!(reference.output, c.definitions[0].outputs[0].id.to_string());
    let mut session = Session::new(project);
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("Motion spatial preview worker: {info:?}");
    session.set_render_host(host.clone());
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let time = Time::new(1, 24).unwrap();
    let key = session
        .preview_state(&reference, time)
        .preview_key(1, 1)
        .unwrap();
    assert_eq!(key.output, reference.output);
    assert_eq!(key.target, Some((id, time)));
    let scene = evaluate_scene_gpu(
        &snapshot,
        &SceneRequest {
            source: reference,
            time,
            dimensions: key.dimensions,
            region: None,
            preview: None,
        },
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    let mut display = renderer.output(&scene, None, &cancel).unwrap();
    let expected = display.readback(&cancel).unwrap();
    let demands =
        [(PanelInstanceId(17), 3), (PanelInstanceId(29), 8)].map(|(consumer, generation)| {
            PreviewDemand {
                consumer,
                generation,
                key: key.clone(),
                background: false,
            }
        });
    session.preview_demands(demands.to_vec());
    let deadline = Instant::now() + Duration::from_secs(60);
    let result = loop {
        session.poll();
        if let Some(result) = session.take_gpu_preview() {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "spatial preview worker timed out"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(result.key, key);
    assert_eq!(
        result.consumers,
        [(PanelInstanceId(17), 3), (PanelInstanceId(29), 8)]
    );
    let mut actual = result.frame.unwrap();
    assert_eq!(actual.statistics.cpu_adapter_nodes, 0);
    assert_eq!(actual.statistics.readback_bytes, 0);
    let actual = actual.readback(&cancel).unwrap();
    assert!(
        actual.rgba() == expected.rgba(),
        "worker and direct spatial output differ"
    );
    assert!(session.take_preview().is_none());
    session.preview_demands(vec![]);
    assert!(display.readback(&cancel).unwrap().rgba() == expected.rgba());
    eprintln!(
        "Motion spatial preview worker passed: named output, exact time, two consumer generations, direct/worker display pixels identical, retained image"
    );
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn native_box_gpu_matches_analytic_slabs_and_retains_shared_prototype_frames() {
    let id = DocumentId::new();
    let (mut c, _, camera_id) = fixture(id);
    let prototype = node("mesh", "box", "Box", vec![]);
    let prototype_id = prototype.id;
    let surface = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Surface")
        .unwrap();
    surface.parameters.insert(
        "geometry".into(),
        Parameter::new(link(prototype_id)).unwrap(),
    );
    let surface_id = surface.id;
    let centers = [[-0.04, 0., 0.], [0.04, 0.012, -0.018]];
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "World")
        .unwrap()
        .parameters
        .insert(
            "position".into(),
            Parameter::new(position(centers[0])).unwrap(),
        );
    let second = node(
        "place",
        "world",
        "Second box",
        vec![
            ("geometry", link(surface_id)),
            ("position", position(centers[1])),
        ],
    );
    let scene = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Scene")
        .unwrap();
    let Binding::List { mut values } = scene.parameters["children"].binding().unwrap() else {
        panic!()
    };
    values.push(link(second.id));
    scene.parameters.insert(
        "children".into(),
        Parameter::new(Binding::List { values }).unwrap(),
    );
    c.definitions[0].nodes.extend([prototype, second]);
    let output = c.definitions[0].outputs[0].id;
    let mut project = Project::new(8);
    let cancel = fold_media::Cancel::default();
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("Native box GPU analytic qualification: {info:?}");
    let mut adapter = Adapter::new(host).unwrap();
    let mut retained = None;
    for (index, (size, camera_z)) in [(0.06, 0.3), (0.09, 0.3), (0.09, -0.3)]
        .into_iter()
        .enumerate()
    {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == prototype_id)
            .unwrap()
            .parameters
            .insert("size".into(), Parameter::new(position([size; 3])).unwrap());
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == camera_id)
            .unwrap()
            .parameters
            .insert(
                "position".into(),
                Parameter::new(position([0., 0., camera_z])).unwrap(),
            );
        commit(&mut project, &c, id);
        let prepared = rendering::spatial(
            &project.snapshot(),
            id,
            output,
            Time::new(index as i64, 24).unwrap(),
            [640, 220],
            Default::default(),
            &|| cancel.check(),
        )
        .unwrap();
        assert_eq!(prepared.scene.meshes.len(), 1);
        assert_eq!(prepared.scene.instances.len(), 2);
        let mut frame = adapter
            .render(&prepared.scene, prepared.binding.clone(), &cancel)
            .unwrap();
        assert_eq!(frame.preparation.meshes, usize::from(index < 2));
        assert_eq!(
            frame.preparation.spawned_instances,
            if index == 0 { 2 } else { 0 }
        );
        let pixels = frame.readback(&prepared.binding, &cancel).unwrap();
        let depth = frame.read_depth(&prepared.binding, &cancel).unwrap();
        // Independent analytic box intersections use authored dimensions and centers,
        // never the generated triangles or mesh positions. Adapter inputs are f32.
        let half = f64::from(size as f32 * 0.5);
        let origin_z = f64::from(camera_z as f32);
        let forward = if camera_z > 0. { -1. } else { 1. };
        let fold_render::spatial::Projection::Perspective { vertical_fov } =
            prepared.scene.camera.projection
        else {
            panic!("perspective oracle required")
        };
        let ray_scale = 2. * (f64::from(vertical_fov) * 0.5).tan() / 220.;
        let intersect = |x: f64, y: f64| {
            let direction = [
                -forward * (x - 320.) * ray_scale,
                (110. - y) * ray_scale,
                forward,
            ];
            let origin = [0., 0., origin_z];
            let mut best = f64::INFINITY;
            for center in centers {
                let center = center.map(|v| f64::from(v as f32));
                let mut near = 0f64;
                let mut far = f64::INFINITY;
                for axis in 0..3 {
                    if direction[axis] == 0. {
                        if origin[axis] < center[axis] - half || origin[axis] > center[axis] + half
                        {
                            far = -1.;
                        }
                        continue;
                    }
                    let a = (center[axis] - half - origin[axis]) / direction[axis];
                    let b = (center[axis] + half - origin[axis]) / direction[axis];
                    near = near.max(a.min(b));
                    far = far.min(a.max(b));
                }
                if near <= far {
                    best = best.min(near);
                }
            }
            best
        };
        let mut covered = 0;
        let mut clear = 0;
        let mut excluded = 0;
        let mut max_error = 0f64;
        for y in 0..220 {
            for x in 0..640 {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let expected = intersect(px, py);
                // Only silhouette edge pixels within 1/128 pixel may be excluded.
                let band = 1. / 128.;
                if [(-band, 0.), (band, 0.), (0., -band), (0., band)]
                    .into_iter()
                    .any(|(dx, dy)| intersect(px + dx, py + dy).is_finite() != expected.is_finite())
                {
                    excluded += 1;
                    continue;
                }
                if expected.is_finite() {
                    assert_eq!(pixels.pixels()[y * 640 + x], [1.; 4], "coverage {x},{y}");
                    let actual = f64::from(depth.view_distance(x as u32, y as u32).unwrap());
                    max_error = max_error.max((actual - expected).abs());
                    assert!(
                        (actual - expected).abs() < 2e-5,
                        "depth {x},{y}: {actual} vs {expected}"
                    );
                    covered += 1;
                } else {
                    assert_eq!(pixels.pixels()[y * 640 + x], [0.; 4]);
                    assert_eq!(depth.values()[y * 640 + x], 0.);
                    clear += 1;
                }
            }
        }
        assert!(covered > 1000 && clear > 10000 && excluded < 500);
        eprintln!(
            "Box size={size}, camera_z={camera_z}: {covered} covered, {clear} clear, {excluded} excluded; max axial-depth error={max_error}"
        );
        if let Some((held, binding, original)) = &mut retained {
            let held: &mut fold_render::gpu::spatial::Frame = held;
            assert_eq!(held.readback(binding, &cancel).unwrap().pixels(), original);
            assert!(held.readback(&prepared.binding, &cancel).is_err());
        } else {
            retained = Some((frame, prepared.binding, pixels.pixels().to_vec()));
        }
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn cloner_gpu_shares_mesh_preserves_cells_and_matches_registered_output() {
    qualify_cloner_gpu(false);
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn animated_cloner_gpu_matches_field_scale_palette_and_registered_output() {
    qualify_cloner_gpu(true);
}

fn qualify_cloner_gpu(animated: bool) {
    use fold_app::media_workflow::{SceneRequest, evaluate_scene_gpu};
    use fold_render::gpu::Renderer;
    let id = DocumentId::new();
    let (mut c, _, _) = fixture(id);
    let dimensions = if animated { [1280, 440] } else { [640, 220] };
    c.canvas.width = dimensions[0];
    c.canvas.height = dimensions[1];
    let prototype = node("mesh", "box", "Box", vec![]);
    let grid = node("distribute", "grid", "Grid", vec![]);
    let grid_id = grid.id;
    let cloner = node(
        "cloner",
        "points",
        "Cloner",
        vec![("prototype", link(prototype.id)), ("points", link(grid.id))],
    );
    let decode = |v: f64| {
        let v = v / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let orange = [1., decode(90.), decode(54.), 1.];
    let blue = [0.02, 0.1, 1., 1.];
    let mut geometry = cloner.id;
    let mut random_id = None;
    if animated {
        use fold_motion::{
            ramp::Ramp,
            types::{Alpha, Encoding},
        };
        let center = node(
            "tween",
            "linear",
            "Center",
            vec![
                ("from", position([-1., -0.5, 0.])),
                ("to", position([1., 0.5, 0.])),
                (
                    "duration",
                    literal(Literal::Duration {
                        value: Time::new(2, 1).unwrap(),
                    }),
                ),
            ],
        );
        let field = node(
            "field",
            "radial",
            "Field",
            vec![("center", link(center.id))],
        );
        let response = |name, mode, from, to| {
            node(
                "ramp",
                mode,
                name,
                vec![
                    ("position", link(field.id)),
                    (
                        "stops",
                        Binding::Ramp {
                            ramp: Ramp::two(from, to).unwrap(),
                        },
                    ),
                ],
            )
        };
        let scale = response(
            "Scale ramp",
            "scalar_linear",
            Literal::Scalar { value: 0.2 },
            Literal::Scalar { value: 1. },
        );
        let color = |value| Literal::Color {
            value,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        };
        let palette = response("Palette", "color_linear", color(orange), color(blue));
        let scaled = node(
            "animator",
            "scale_multiply",
            "Scale",
            vec![("geometry", link(cloner.id)), ("factor", link(scale.id))],
        );
        let colored = node(
            "animator",
            "color_replace",
            "Color",
            vec![("geometry", link(scaled.id)), ("color", link(palette.id))],
        );
        let random = node(
            "random",
            "uniform",
            "Rotation variation",
            vec![
                (
                    "maximum",
                    literal(Literal::Scalar {
                        value: std::f64::consts::TAU,
                    }),
                ),
                (
                    "stream",
                    literal(Literal::Text {
                        value: "rotation".into(),
                    }),
                ),
            ],
        );
        random_id = Some(random.id);
        let rotated = node(
            "animator",
            "rotation_z_add",
            "Rotate",
            vec![("geometry", link(colored.id)), ("angle", link(random.id))],
        );
        geometry = rotated.id;
        c.definitions[0].nodes.extend([
            center, field, scale, palette, scaled, colored, random, rotated,
        ]);
    }
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "Surface")
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(geometry)).unwrap());
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.name == "World")
        .unwrap()
        .parameters
        .insert(
            "position".into(),
            Parameter::new(position([0.; 3])).unwrap(),
        );
    c.definitions[0].nodes.extend([prototype, grid, cloner]);
    let output = c.definitions[0].outputs[0].id;
    let mut project = Project::new(8);
    let cancel = fold_media::Cancel::default();
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("Cloner GPU qualification: {info:?}");
    let mut adapter = Adapter::new(host.clone()).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let mut retained = None;
    let mut identities = std::collections::BTreeMap::new();
    let samples = if animated {
        vec![
            (20, Time::new(2, 1).unwrap()),
            (20, Time::ZERO),
            (21, Time::new(1, 1).unwrap()),
            (20, Time::new(2, 1).unwrap()),
            (0, Time::new(3, 1).unwrap()),
        ]
    } else {
        [20, 21, 20, 0]
            .into_iter()
            .enumerate()
            .map(|(i, n)| (n, Time::new(i as i64, 24).unwrap()))
            .collect()
    };
    let mut previous_columns = 0;
    for (index, (columns, time)) in samples.into_iter().enumerate() {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == grid_id)
            .unwrap()
            .parameters
            .insert(
                "columns".into(),
                Parameter::new(literal(Literal::Integer { value: columns })).unwrap(),
            );
        commit(&mut project, &c, id);
        let snapshot = project.snapshot();
        let prepared = rendering::spatial(
            &snapshot,
            id,
            output,
            time,
            dimensions,
            Default::default(),
            &|| cancel.check(),
        )
        .unwrap();
        assert_eq!(prepared.scene.meshes.len(), usize::from(columns != 0));
        assert_eq!(prepared.scene.instances.len(), columns as usize * 12);
        let fold_motion::evaluation::GeometryData::Instances(collection) =
            &prepared.native.entries[0].placement.geometry.data
        else {
            panic!()
        };
        for ((point, instance), rendered) in collection
            .distribution
            .points
            .iter()
            .zip(&collection.instances)
            .zip(&prepared.scene.instances)
        {
            if let Some(old) = identities.insert(point.cell, (instance.id, rendered.id)) {
                assert_eq!(old, (instance.id, rendered.id));
            }
        }
        let mut frame = adapter
            .render(&prepared.scene, prepared.binding.clone(), &cancel)
            .unwrap();
        assert_eq!(frame.preparation.meshes, usize::from(index == 0));
        assert_eq!(
            frame.preparation.spawned_instances,
            (columns - previous_columns).max(0) as usize * 12
        );
        assert_eq!(
            frame.preparation.removed_instances,
            (previous_columns - columns).max(0) as usize * 12
        );
        previous_columns = columns;
        let pixels = frame.readback(&prepared.binding, &cancel).unwrap();
        let depth = frame.read_depth(&prepared.binding, &cancel).unwrap();
        let sample_pixel = |x: f64, y: f64, distance: f64| {
            let world_per_pixel = 2. * distance * (std::f64::consts::FRAC_PI_4 * 0.5).tan()
                / f64::from(dimensions[1]);
            let px = (f64::from(dimensions[0]) * 0.5 + x / world_per_pixel).floor() as usize;
            let py = (f64::from(dimensions[1]) * 0.5 - y / world_per_pixel).floor() as usize;
            assert!(px < dimensions[0] as usize && py < dimensions[1] as usize);
            (px, py)
        };
        let mut max_color_error = 0_f64;
        let mut max_depth_error = 0_f64;
        let mut checked = 0;
        let mut silhouette = [0_usize; 3];
        // Centers and horizontal gaps come from the authored grid formula, not
        // converted instance transforms. Each center ray hits the front box cap.
        for row in 0..12 {
            for column in 0..columns {
                let x = (column as f64 - (columns as f64 - 1.) * 0.5) * 0.12;
                let y = (row as f64 - 5.5) * 0.12;
                let seconds = time.numerator() as f64 / time.denominator() as f64;
                let center = [-1. + seconds.min(2.), -0.5 + seconds.min(2.) * 0.5];
                let influence = if animated {
                    (1. - ((x - center[0]).powi(2) + (y - center[1]).powi(2)).sqrt()).clamp(0., 1.)
                } else {
                    1.
                };
                let scale = if animated { 0.2 + 0.8 * influence } else { 1. };
                let front = 4. - 0.03 * scale;
                let (px, py) = sample_pixel(x, y, front);
                let pixel = pixels.pixels()[py * dimensions[0] as usize + px];
                for axis in 0..4 {
                    let expected = if animated {
                        orange[axis] + (blue[axis] - orange[axis]) * influence
                    } else {
                        1.
                    };
                    let error = (f64::from(pixel[axis]) - expected).abs();
                    max_color_error = max_color_error.max(error);
                    assert!(
                        error <= 0.002,
                        "cell {column},{row}, channel {axis}, error {error}"
                    );
                }
                let distance = depth.view_distance(px as u32, py as u32).unwrap();
                let error = (f64::from(distance) - front).abs();
                max_depth_error = max_depth_error.max(error);
                assert!(error < 2e-4, "cell {column},{row}, depth error {error}");
                if let Some(random_id) = random_id {
                    let instance =
                        &collection.instances[row as usize * columns as usize + column as usize];
                    let angle = std::f64::consts::TAU
                        * fold_motion::execution::random_v1(
                            c.seed,
                            random_id.to_bytes(),
                            Some(instance.id),
                            "rotation",
                        );
                    assert_eq!(instance.rotation_z, angle);
                    // Independent inverse-rotation slab oracle. Only semantic ID
                    // is read from evaluation; placement, scale and rotation come
                    // from authored grid/field/hash formulas above.
                    let (sin, cos) = angle.sin_cos();
                    let half = 0.03 * scale;
                    let pitch = 2. * front * (std::f64::consts::FRAC_PI_4 * 0.5).tan()
                        / f64::from(dimensions[1]);
                    let radius = half * std::f64::consts::SQRT_2;
                    let (front_left, front_bottom) = sample_pixel(x - radius, y - radius, front);
                    let (front_right, front_top) = sample_pixel(x + radius, y + radius, front);
                    let (back_left, back_bottom) = sample_pixel(x - radius, y - radius, 4. + half);
                    let (back_right, back_top) = sample_pixel(x + radius, y + radius, 4. + half);
                    let (left, right) = (front_left.min(back_left), front_right.max(back_right));
                    let (top, bottom) = (front_top.min(back_top), front_bottom.max(back_bottom));
                    for py in top.saturating_sub(1)..=(bottom + 1).min(dimensions[1] as usize - 1) {
                        for px in
                            left.saturating_sub(1)..=(right + 1).min(dimensions[0] as usize - 1)
                        {
                            let ray_x =
                                (px as f64 + 0.5 - f64::from(dimensions[0]) * 0.5) * pitch / front;
                            let ray_y =
                                (f64::from(dimensions[1]) * 0.5 - py as f64 - 0.5) * pitch / front;
                            let origin = [-cos * x - sin * y, sin * x - cos * y, 4.];
                            let direction =
                                [cos * ray_x + sin * ray_y, -sin * ray_x + cos * ray_y, -1.];
                            let intersect = |extent: f64| {
                                let mut near = 0_f64;
                                let mut far = f64::INFINITY;
                                for axis in 0..3 {
                                    if direction[axis].abs() < 1e-15 {
                                        if origin[axis].abs() > extent {
                                            return None;
                                        }
                                    } else {
                                        let a = (-extent - origin[axis]) / direction[axis];
                                        let b = (extent - origin[axis]) / direction[axis];
                                        near = near.max(a.min(b));
                                        far = far.min(a.max(b));
                                    }
                                }
                                (near <= far).then_some(near)
                            };
                            // Exclude only the declared 1/128-pixel world-space
                            // box expansion/contraction band at raster edges.
                            let inner = intersect(half - pitch / 128.);
                            let outer = intersect(half + pitch / 128.);
                            let offset = py * dimensions[0] as usize + px;
                            if inner.is_some() {
                                let distance = intersect(half).unwrap();
                                for axis in 0..4 {
                                    let expected =
                                        orange[axis] + (blue[axis] - orange[axis]) * influence;
                                    assert!(
                                        (f64::from(pixels.pixels()[offset][axis]) - expected).abs()
                                            <= 0.002,
                                        "rotated interior {column},{row} at {px},{py}"
                                    );
                                }
                                assert!(
                                    (f64::from(depth.view_distance(px as u32, py as u32).unwrap())
                                        - distance)
                                        .abs()
                                        < 2e-4,
                                    "rotated depth {column},{row} at {px},{py}"
                                );
                                silhouette[0] += 1;
                            } else if outer.is_none() {
                                assert_eq!(
                                    pixels.pixels()[offset],
                                    [0.; 4],
                                    "rotated exterior {column},{row} at {px},{py}"
                                );
                                silhouette[1] += 1;
                            } else {
                                silhouette[2] += 1;
                            }
                        }
                    }
                }
                checked += 1;
                if column + 1 < columns {
                    let (px, py) = sample_pixel(x + 0.06, y, 4.);
                    assert_eq!(
                        pixels.pixels()[py * dimensions[0] as usize + px],
                        [0.; 4],
                        "gap after {column},{row}"
                    );
                    assert_eq!(depth.values()[py * dimensions[0] as usize + px], 0.);
                }
            }
        }
        if columns == 0 {
            assert!(pixels.pixels().iter().all(|p| *p == [0.; 4]));
        }
        let request = SceneRequest {
            source: fold_project::DocumentRef {
                document: id,
                output: output.to_string(),
                extensions: Default::default(),
            },
            time,
            dimensions,
            region: None,
            preview: None,
        };
        let mut published =
            evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel).unwrap();
        assert_eq!(
            published.descriptor().unwrap().timing.unwrap().timestamp,
            time
        );
        assert_eq!(published.statistics.cpu_adapter_nodes, 0);
        assert_eq!(published.statistics.readback_bytes, 0);
        assert_eq!(
            published.readback(&cancel).unwrap().pixels(),
            pixels.pixels()
        );
        if let Some((held, binding, original)) = &mut retained {
            let held: &mut fold_render::gpu::spatial::Frame = held;
            assert_eq!(held.readback(binding, &cancel).unwrap().pixels(), original);
            assert!(held.readback(&prepared.binding, &cancel).is_err());
        } else {
            retained = Some((frame, prepared.binding, pixels.pixels().to_vec()));
        }
        eprintln!(
            "Cloner animated={animated} time={time:?} columns={columns}: {checked} centers and adjacent gaps checked; max color error={max_color_error}, max depth error={max_depth_error}; rotated interior/exterior/edge={silhouette:?}; direct/registered pixels match; mesh and instance reuse verified"
        );
    }
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn native_camera_planes_and_world_box_match_analytic_pixels_and_depth() {
    use fold_motion::types::{Alpha, Encoding};
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("Native Camera Plane / World: {info:?}; RGBA16F; MSAA off");
    let mut adapter = Adapter::new(host.clone()).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let distance = |value| {
        literal(Literal::Distance {
            value,
            unit: Unit::Meters,
        })
    };
    let canvas_distance = |value| {
        literal(Literal::Distance {
            value,
            unit: Unit::Canvas,
        })
    };
    let color = |value| {
        literal(Literal::Color {
            value,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        })
    };
    for orthographic in [false, true] {
        for physical in [false, true] {
            for rear_distance in [4., 6.] {
                for view_offset in [0., 1.137] {
                    let view_x = if view_offset == 0. { 0. } else { 0.173 };
                    let document = DocumentId::new();
                    let mut c = Construction::from_document(
                        &fold_motion::package::create(document).unwrap(),
                    )
                    .unwrap();
                    c.canvas.width = 200;
                    c.canvas.height = 100;
                    let camera = node(
                        "camera",
                        if orthographic {
                            "orthographic"
                        } else {
                            "perspective"
                        },
                        "Camera",
                        vec![
                            ("position", position([0., 0., 4.])),
                            if orthographic {
                                ("vertical_size", distance(4.))
                            } else {
                                (
                                    "vertical_fov",
                                    literal(Literal::Scalar {
                                        value: std::f64::consts::FRAC_PI_2,
                                    }),
                                )
                            },
                        ],
                    );
                    let mut view = camera.clone();
                    view.id = ObjectId::new();
                    view.name = "View Camera".into();
                    for parameter in view.parameters.values_mut() {
                        parameter.id = ObjectId::new();
                    }
                    view.parameters.insert(
                        "position".into(),
                        Parameter::new(position([view_x, 0., 4. + view_offset])).unwrap(),
                    );
                    view.parameters.insert(
                        "target".into(),
                        Parameter::new(position([view_x, 0., 0.])).unwrap(),
                    );
                    let view_id = if view_offset == 0. {
                        camera.id
                    } else {
                        view.id
                    };
                    let mut nodes = vec![];
                    let mut children = vec![];
                    for (index, width, depth, paint) in [
                        (0, 100., rear_distance, [1., 0., 0., 1.]),
                        (1, 50., 2., [0., 1., 0., 1.]),
                    ] {
                        let shape = node(
                            "shape",
                            "rectangle",
                            &format!("Rectangle {index}"),
                            vec![
                                ("width", canvas_distance(width)),
                                ("height", canvas_distance(50.)),
                            ],
                        );
                        let fill = node(
                            "fill",
                            "solid",
                            &format!("Fill {index}"),
                            vec![("geometry", link(shape.id)), ("color", color(paint))],
                        );
                        let mut bindings = vec![
                            ("geometry", link(fill.id)),
                            ("camera", link(camera.id)),
                            ("distance", distance(depth)),
                            (
                                "position",
                                literal(Literal::Vector {
                                    value: vec![50., 25.],
                                    unit: Unit::Canvas,
                                    space: Space::Screen,
                                }),
                            ),
                        ];
                        if physical {
                            bindings.push((
                                "meters_per_unit",
                                literal(Literal::Scalar { value: 0.0123 }),
                            ));
                        }
                        let placement = node(
                            "place",
                            if physical {
                                "camera_plane_physical"
                            } else {
                                "camera_plane_viewport"
                            },
                            &format!("Plane {index}"),
                            bindings,
                        );
                        children.push(link(placement.id));
                        nodes.extend([shape, fill, placement]);
                    }
                    let shape = node(
                        "mesh",
                        "box",
                        "World Box",
                        vec![("size", position([0.6, 0.6, 0.2]))],
                    );
                    let material = node(
                        "material",
                        "unlit",
                        "Blue",
                        vec![
                            ("geometry", link(shape.id)),
                            ("color", color([0., 0., 1., 1.])),
                        ],
                    );
                    let world = node(
                        "place",
                        "world",
                        "Box Placement",
                        vec![
                            ("geometry", link(material.id)),
                            ("position", position([0., 0., 3.])),
                        ],
                    );
                    children.push(link(world.id));
                    let scene = node(
                        "scene",
                        "spatial",
                        "Scene",
                        vec![
                            ("camera", link(view_id)),
                            ("children", Binding::List { values: children }),
                        ],
                    );
                    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
                    nodes.extend([shape, material, world, camera, view, scene]);
                    c.definitions[0].nodes = nodes;
                    let mut project = Project::new(8);
                    project
                        .commit(EditBatch {
                            base: project.snapshot().revision(),
                            mutations: vec![Mutation::PutDocument(c.document(document).unwrap())],
                        })
                        .unwrap();
                    let prepared = rendering::spatial(
                        &project.snapshot(),
                        document,
                        c.definitions[0].outputs[0].id,
                        Time::ZERO,
                        [160, 120],
                        Default::default(),
                        &|| Ok(()),
                    )
                    .unwrap();
                    let graph = rendering::image(
                        &project.snapshot(),
                        document,
                        c.definitions[0].outputs[0].id,
                        Time::ZERO,
                        [160, 120],
                        Default::default(),
                        &|| Ok(()),
                    )
                    .unwrap();
                    assert!(matches!(
                        graph.nodes.as_slice(),
                        [fold_render::ImageOp::Spatial { .. }]
                    ));
                    let mut frame = adapter
                        .render(&prepared.scene, prepared.binding.clone(), &cancel)
                        .unwrap();
                    let pixels = frame.readback(&prepared.binding, &cancel).unwrap();
                    let depth = frame.read_depth(&prepared.binding, &cancel).unwrap();
                    let request = fold_app::media_workflow::SceneRequest {
                        source: fold_project::DocumentRef {
                            document,
                            output: c.definitions[0].outputs[0].id.to_string(),
                            extensions: Default::default(),
                        },
                        time: Time::ZERO,
                        dimensions: [160, 120],
                        region: None,
                        preview: None,
                    };
                    let mut registered = fold_app::media_workflow::evaluate_scene_gpu(
                        &project.snapshot(),
                        &request,
                        &mut renderer,
                        &mut decoder,
                        &cancel,
                    )
                    .unwrap();
                    let registered_pixels = registered.readback(&cancel).unwrap();
                    assert_eq!(registered_pixels.pixels(), pixels.pixels());

                    for y in 0..120 {
                        for x in 0..160 {
                            let p = [x as f64 + 0.5, y as f64 + 0.5];
                            let inside_plane = |width: f64, distance: f64| {
                                let project = |point: [f64; 2]| {
                                    let attached_height =
                                        if orthographic { 4. } else { 2. * distance };
                                    let scale = if physical {
                                        [0.0123; 2]
                                    } else {
                                        [attached_height * (4. / 3.) / 200., attached_height / 100.]
                                    };
                                    let view_height = if orthographic {
                                        4.
                                    } else {
                                        2. * (distance + view_offset)
                                    };
                                    [
                                        80. + ((point[0] - 100.) * scale[0] - view_x) * 120.
                                            / view_height,
                                        60. + (point[1] - 50.) * scale[1] * 120. / view_height,
                                    ]
                                };
                                let a = project([50., 25.]);
                                let b = project([50. + width, 75.]);
                                p[0] > a[0] && p[0] < b[0] && p[1] > a[1] && p[1] < b[1]
                            };
                            let box_scale = if orthographic {
                                30.
                            } else {
                                60. / (0.9 + view_offset)
                            };
                            let box_half = 0.3 * box_scale;
                            let box_center = 80. - view_x * box_scale;
                            let in_box = (p[0] - box_center).abs() < box_half
                                && (p[1] - 60.).abs() < box_half;
                            let (expected, z) = if in_box {
                                ([0., 0., 1., 1.], Some((0.9 + view_offset) as f32))
                            } else if inside_plane(50., 2.) {
                                ([0., 1., 0., 1.], Some((2. + view_offset) as f32))
                            } else if inside_plane(100., rear_distance) {
                                ([1., 0., 0., 1.], Some((rear_distance + view_offset) as f32))
                            } else {
                                ([0.; 4], None)
                            };
                            assert_eq!(
                                pixels.pixels()[y * 160 + x],
                                expected,
                                "ortho={orthographic} physical={physical} rear={rear_distance} view_offset={view_offset} ({x},{y})"
                            );
                            let actual = depth.view_distance(x as u32, y as u32);
                            match z {
                                Some(z) => assert!((actual.unwrap() - z).abs() < 2e-5),
                                None => assert_eq!(actual, None),
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
fn camera_plane_stroke_over_fill_matches_premultiplied_paint() {
    use fold_motion::types::{Alpha, Encoding};
    let (host, info) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    eprintln!("Camera Plane paint: {info:?}");
    let mut adapter = Adapter::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let distance = |value| {
        literal(Literal::Distance {
            value,
            unit: Unit::Canvas,
        })
    };
    let paint = |value| {
        literal(Literal::Color {
            value,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        })
    };
    let mut failures = vec![];
    for (fill_alpha, stroke_alpha) in [(1., 1.), (1., 0.5), (0.5, 1.), (0.5, 0.5)] {
        let document = DocumentId::new();
        let mut c =
            Construction::from_document(&fold_motion::package::create(document).unwrap()).unwrap();
        c.canvas.width = 160;
        c.canvas.height = 120;
        let camera = node(
            "camera",
            "perspective",
            "Camera",
            vec![("position", position([0., 0., 4.]))],
        );
        let shape = node(
            "shape",
            "rectangle",
            "Rectangle",
            vec![("width", distance(80.)), ("height", distance(60.))],
        );
        let fill = node(
            "fill",
            "solid",
            "Fill",
            vec![
                ("geometry", link(shape.id)),
                ("color", paint([1., 0., 0., fill_alpha])),
            ],
        );
        let stroke = node(
            "stroke",
            "solid_centered_butt_miter",
            "Stroke",
            vec![
                ("geometry", link(fill.id)),
                ("color", paint([0., 1., 0., stroke_alpha])),
                ("width", distance(10.)),
            ],
        );
        let plane = node(
            "place",
            "camera_plane_viewport",
            "Plane",
            vec![
                ("geometry", link(stroke.id)),
                ("camera", link(camera.id)),
                (
                    "position",
                    literal(Literal::Vector {
                        value: vec![40., 30.],
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    }),
                ),
            ],
        );
        let scene = node(
            "scene",
            "spatial",
            "Scene",
            vec![
                ("camera", link(camera.id)),
                (
                    "children",
                    Binding::List {
                        values: vec![link(plane.id)],
                    },
                ),
            ],
        );
        c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
        c.definitions[0].nodes = vec![camera, shape, fill, stroke, plane, scene];
        let mut project = Project::new(8);
        commit(&mut project, &c, document);
        let prepared = rendering::spatial(
            &project.snapshot(),
            document,
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            [160, 120],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        let mut frame = adapter
            .render(&prepared.scene, prepared.binding.clone(), &cancel)
            .unwrap();
        let pixels = frame.readback(&prepared.binding, &cancel).unwrap();
        let mut mismatches = 0usize;
        let mut first = None;
        for y in 0..120 {
            for x in 0..160 {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let inside = |a: f64, b: f64, c: f64, d: f64| px > a && px < c && py > b && py < d;
                let fill = if inside(40., 30., 120., 90.) {
                    fill_alpha
                } else {
                    0.
                };
                let stroke = if inside(35., 25., 125., 95.) && !inside(45., 35., 115., 85.) {
                    stroke_alpha
                } else {
                    0.
                };
                let expected = [
                    fill * (1. - stroke),
                    stroke,
                    0.,
                    stroke + fill * (1. - stroke),
                ]
                .map(|v| v as f32);
                let actual = pixels.pixels()[y * 160 + x];
                if actual != expected {
                    mismatches += 1;
                    first.get_or_insert((x, y, actual, expected));
                }
            }
        }
        eprintln!(
            "fill={fill_alpha} stroke={stroke_alpha}: {mismatches} incorrect pixels; first={first:?}"
        );
        if mismatches != 0 {
            failures.push((fill_alpha, stroke_alpha, mismatches, first));
        }
    }
    assert!(
        failures.is_empty(),
        "Camera Plane paint failures: {failures:?}"
    );
}

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn camera_plane_text_and_outline_match_screen_interiors_and_retain_frames() {
    use fold_motion::types::{Alpha, Encoding};
    let document = DocumentId::new();
    let mut c =
        Construction::from_document(&fold_motion::package::create(document).unwrap()).unwrap();
    c.canvas.width = 640;
    c.canvas.height = 220;
    let camera = node(
        "camera",
        "perspective",
        "Camera",
        vec![("position", position([0., 0., 4.]))],
    );
    let text = node(
        "text",
        "packaged",
        "Text",
        vec![(
            "text",
            literal(Literal::Text {
                value: "O B 8 O".into(),
            }),
        )],
    );
    let outline = node(
        "outline_text",
        "curves",
        "Outline",
        vec![("text", link(text.id))],
    );
    let paint = |value| {
        literal(Literal::Color {
            value,
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        })
    };
    let fill = node(
        "fill",
        "solid",
        "Fill",
        vec![
            ("geometry", link(text.id)),
            ("color", paint([1., 0., 0., 0.5])),
        ],
    );
    let stroke = node(
        "stroke",
        "solid_centered_butt_miter",
        "Stroke",
        vec![
            ("geometry", link(fill.id)),
            ("color", paint([0., 1., 0., 0.5])),
            (
                "width",
                literal(Literal::Distance {
                    value: 4.,
                    unit: Unit::Canvas,
                }),
            ),
        ],
    );
    let plane = node(
        "place",
        "camera_plane_viewport",
        "Plane",
        vec![("geometry", link(stroke.id)), ("camera", link(camera.id))],
    );
    let scene = node(
        "scene",
        "spatial",
        "Scene",
        vec![
            ("camera", link(camera.id)),
            (
                "children",
                Binding::List {
                    values: vec![link(plane.id)],
                },
            ),
        ],
    );
    let (plane_id, fill_id, text_id, outline_id) = (plane.id, fill.id, text.id, outline.id);
    c.definitions[0].outputs[0].value = Reference::Node { node: scene.id }.into();
    c.definitions[0].nodes = vec![camera, text, outline, fill, stroke, plane, scene];
    let mut screen = c.clone();
    let placement = screen.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.id == plane_id)
        .unwrap();
    placement.mode = "screen".into();
    placement.parameters.remove("camera");
    placement.parameters.remove("distance");
    let mut project = Project::new(8);
    commit(&mut project, &screen, document);
    let output = c.definitions[0].outputs[0].id;
    let oracle = fold_render::render(
        rendering::image(
            &project.snapshot(),
            document,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap(),
    )
    .unwrap();
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!(
        "Native Camera Plane typography: {info:?}; 3x3 uniform Screen neighborhoods; tolerance 0.002"
    );
    let mut adapter = Adapter::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut retained: Option<(
        fold_render::gpu::spatial::Frame,
        fold_render::spatial::Binding,
        Vec<[f32; 4]>,
    )> = None;
    let mut baseline = None;
    for geometry in [text_id, outline_id, text_id] {
        c.definitions[0]
            .nodes
            .iter_mut()
            .find(|n| n.id == fill_id)
            .unwrap()
            .parameters
            .insert("geometry".into(), Parameter::new(link(geometry)).unwrap());
        commit(&mut project, &c, document);
        let prepared = rendering::spatial(
            &project.snapshot(),
            document,
            output,
            Time::ZERO,
            [640, 220],
            Default::default(),
            &|| Ok(()),
        )
        .unwrap();
        let mut frame = adapter
            .render(&prepared.scene, prepared.binding.clone(), &cancel)
            .unwrap();
        let pixels = frame.readback(&prepared.binding, &cancel).unwrap();
        let mut checked = [0usize; 2];
        let mut excluded = 0;
        let mut max_error = 0f32;
        for y in 1..219 {
            for x in 1..639 {
                let expected = oracle.pixels()[y * 640 + x];
                let uniform = (y - 1..=y + 1).all(|yy| {
                    (x - 1..=x + 1).all(|xx| {
                        oracle.pixels()[yy * 640 + xx]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| (*a - b).abs() < 1e-6)
                    })
                });
                if !uniform {
                    excluded += 1;
                    continue;
                }
                checked[usize::from(expected[3] > 0.)] += 1;
                for (actual, expected) in pixels.pixels()[y * 640 + x].iter().zip(expected) {
                    let error = (*actual - expected).abs();
                    max_error = max_error.max(error);
                    assert!(
                        error <= 0.002,
                        "({x},{y}) channel {actual} != {expected}; pixel {:?}; Screen {:?}",
                        pixels.pixels()[y * 640 + x],
                        oracle.pixels()[y * 640 + x]
                    );
                }
            }
        }
        assert!(checked[0] > 10000 && checked[1] > 1000);
        eprintln!(
            "Typography checked clear/paint={checked:?}; excluded nonuniform={excluded}; maximum error={max_error}"
        );
        if let Some(original) = &baseline {
            assert_eq!(pixels.pixels(), original);
        } else {
            baseline = Some(pixels.pixels().to_vec());
        }
        if let Some((old, binding, original)) = &mut retained {
            assert_eq!(old.readback(binding, &cancel).unwrap().pixels(), original);
            assert!(old.readback(&prepared.binding, &cancel).is_err());
        } else {
            retained = Some((frame, prepared.binding, pixels.pixels().to_vec()));
        }
    }
}
