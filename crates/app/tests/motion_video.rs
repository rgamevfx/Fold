//! Application registration -> shared output compilation -> rendered pixels.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::{Binding, Interpolation, Key, Track},
    model::{Construction, Parameter, Reference},
    types::{Alpha, Encoding, Literal, Unit},
};
use fold_project::{EditBatch, Mutation, Project};
fn link(id: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node: id }.into(),
    }
}
fn fixture() -> (Project, DocumentId, Construction) {
    let id = DocumentId::new();
    let mut c = Construction::from_document(&fold_motion::package::create(id).unwrap()).unwrap();
    c.canvas.width = 320;
    c.canvas.height = 200;
    c.canvas.start = Time::new(1, 2).unwrap();
    c.canvas.end = Time::new(17, 24).unwrap();
    let shape = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.shape")
        .unwrap();
    let mut track = Track::constant(
        Literal::Distance {
            value: 100.,
            unit: Unit::Canvas,
        },
        Time::ZERO,
    )
    .unwrap();
    track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: Literal::Distance {
            value: 200.,
            unit: Unit::Canvas,
        },
        interpolation: Interpolation::Linear,
        handles: Default::default(),
        extensions: Default::default(),
    });
    let keys = fold_motion::registry::resolve("fold.motion.keyframes", 1, "value")
        .unwrap()
        .node(
            "Width animation",
            [("track".into(), Binding::Track { track })].into(),
        )
        .unwrap();
    shape
        .parameters
        .insert("width".into(), Parameter::new(link(keys.id)).unwrap());
    let fill = fold_motion::registry::resolve("fold.motion.fill", 1, "solid")
        .unwrap()
        .node(
            "Red",
            [
                ("geometry".into(), link(shape.id)),
                (
                    "color".into(),
                    Binding::Literal {
                        value: Literal::Color {
                            value: [1., 0., 0., 0.5],
                            encoding: Encoding::LinearRec709,
                            alpha: Alpha::Straight,
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let placement = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap();
    // This pixel oracle deliberately uses an origin placement, independent of
    // the centered artwork supplied by the interactive document starter.
    placement.parameters.insert(
        "position".into(),
        Parameter::new(Binding::Literal {
            value: Literal::Vector {
                value: vec![0., 0.],
                unit: Unit::Canvas,
                space: fold_motion::types::Space::Screen,
            },
        })
        .unwrap(),
    );
    placement
        .parameters
        .insert("geometry".into(), Parameter::new(link(fill.id)).unwrap());
    c.definitions[0].nodes.extend([fill, keys]);
    let mut project = Project::new(8);
    commit(&mut project, id, &c);
    (project, id, c)
}
fn commit(project: &mut Project, id: DocumentId, c: &Construction) {
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
        })
        .unwrap();
}
#[test]
fn registered_screen_output_preserves_identity_time_and_snapshot_pixels() {
    let (mut project, id, mut c) = fixture();
    let registry = fold_app::packages::builtins();
    let held = project.snapshot();
    let outputs = registry.outputs(&held, id);
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].label, "main");
    assert_eq!(
        outputs[0].reference.output,
        c.definitions[0].outputs[0].id.to_string()
    );
    let reference = outputs[0].reference.clone();
    let info = registry.output_ref(&held, &reference).unwrap();
    assert_eq!(info.frames, 5);
    assert_eq!(info.rate, [24, 1]);
    let graph = registry
        .video(&held, &reference, Time::ZERO, 320, 200)
        .unwrap();
    let frame = fold_render::render(graph.clone()).unwrap();
    assert_eq!(frame.pixels()[10 * 320 + 149], [0.5, 0., 0., 0.5]);
    assert_eq!(frame.pixels()[10 * 320 + 150], [0.; 4]); // scene time 1/2 -> width 150
    let later = fold_render::render(
        registry
            .video(&held, &reference, Time::new(1, 10).unwrap(), 320, 200)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(later.pixels()[10 * 320 + 159], [0.5, 0., 0., 0.5]);
    assert_eq!(later.pixels()[10 * 320 + 160], [0.; 4]);
    c.definitions[0].outputs[0].name = "Renamed artwork".into();
    commit(&mut project, id, &c);
    assert_eq!(
        registry.outputs(&project.snapshot(), id)[0].reference,
        reference
    );
    assert_eq!(
        registry.outputs(&project.snapshot(), id)[0].label,
        "Renamed artwork"
    );
    assert!(fold_render::render(graph).unwrap().pixels() == frame.pixels());
    for time in [Time::new(-1, 24).unwrap(), Time::new(5, 24).unwrap()] {
        assert!(registry.video(&held, &reference, time, 320, 200).is_err());
    }
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        registry
            .video_with(&held, &reference, Time::ZERO, [320, 200], &cancel)
            .is_err()
    );
    let mut missing = reference.clone();
    missing.output = "video".into();
    assert!(
        registry
            .video(&held, &missing, Time::ZERO, 320, 200)
            .is_err()
    );
}
#[test]
fn registered_screen_output_converts_straight_rec709_paint_once_for_aces() {
    let (mut project, id, _) = fixture();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::SetSettings(
                [(
                    fold_platform::color::PROJECT_KEY.into(),
                    serde_json::to_value(fold_platform::color::ProjectColor::default()).unwrap(),
                )]
                .into(),
            )],
        })
        .unwrap();
    let registry = fold_app::packages::builtins();
    let snapshot = project.snapshot();
    let reference = registry.outputs(&snapshot, id)[0].reference.clone();
    let graph = registry
        .video(&snapshot, &reference, Time::ZERO, 320, 200)
        .unwrap();
    let fold_render::ImageOp::Vector(drawings) = &graph.nodes[0] else {
        panic!()
    };
    let color = drawings[0].fill.unwrap();
    for (actual, expected) in color
        .into_iter()
        .zip([0.6130974293, 0.0701937228, 0.0206155926, 0.5])
    {
        assert!((actual - expected).abs() < 1e-10);
    }
}

#[test]
fn motion_document_placement_retains_named_output_and_renders_through_timeline() {
    let (mut project, id, _) = fixture();
    let registry = fold_app::packages::builtins();
    let source = registry.default_output(&project.snapshot(), id).unwrap();
    let args = fold_app::document_placement::InsertArgs {
        source: id,
        sequence: None,
        at: Time::ZERO,
    };
    let batch = fold_app::document_placement::insert(
        &project.snapshot(),
        &serde_json::to_vec(&args).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let Mutation::PutDocument(document) = &batch.mutations[0] else {
        panic!()
    };
    let sequence_id = document.id;
    let sequence = fold_timeline::Sequence::from_document(document).unwrap();
    let fold_timeline::SourceMedia::Document { source: placed, .. } = &sequence.clips[0].info
    else {
        panic!()
    };
    assert_eq!(placed, &source);
    assert!(document.dependencies.contains(&source));
    project.commit(batch).unwrap();
    let snapshot = project.snapshot();
    let output = registry.default_output(&snapshot, sequence_id).unwrap();
    let nested = fold_render::render(
        registry
            .video(&snapshot, &output, Time::ZERO, 320, 200)
            .unwrap(),
    )
    .unwrap();
    let direct = fold_render::render(
        registry
            .video(&snapshot, &source, Time::ZERO, 320, 200)
            .unwrap(),
    )
    .unwrap();
    // Timeline presents its tracks over an opaque black canvas.
    assert!(nested.pixels() == direct.over_black().pixels());
    project.undo().unwrap();
    assert!(
        !project
            .snapshot()
            .state()
            .documents
            .contains_key(&sequence_id)
    );
    assert!(project.snapshot().state().documents.contains_key(&id));
    project.redo().unwrap();
    assert_eq!(
        registry.default_output(&project.snapshot(), id).unwrap(),
        source
    );
}

#[test]
fn compositor_read_preserves_motion_output_time_mapping_alpha_and_channels() {
    use fold_compositor::{Composite, Node, Parameters, Source};
    let (mut project, id, _) = fixture();
    let registry = fold_app::packages::builtins();
    let source = registry.default_output(&project.snapshot(), id).unwrap();
    let info = registry.output_ref(&project.snapshot(), &source).unwrap();
    let read = Node::new(
        Parameters::Read {
            source: Source::Document {
                source: source.clone(),
                info: info.clone(),
            },
            start: Time::new(1, 24).unwrap(),
            source_start: Time::new(2, 24).unwrap(),
            duration: Time::new(3, 24).unwrap(),
        },
        vec![],
    );
    let output = Node::new(Parameters::Output, vec![read.id]);
    let mut composite = Composite {
        info,
        output: output.id,
        nodes: vec![read, output],
        extensions: Default::default(),
    };
    let composite_id = DocumentId::new();
    let document = composite.document(composite_id).unwrap();
    assert!(document.dependencies.contains(&source));
    assert_eq!(Composite::from_document(&document).unwrap(), composite);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let output = registry.default_output(&snapshot, composite_id).unwrap();
    let mut nested_channels = registry.channels(&snapshot, &output).unwrap();
    let mut source_channels = registry.channels(&snapshot, &source).unwrap();
    nested_channels.sort();
    source_channels.sort();
    assert_eq!(nested_channels, source_channels);
    for time in [Time::new(1, 24).unwrap(), Time::new(1, 12).unwrap()] {
        let nested =
            fold_render::render(registry.video(&snapshot, &output, time, 320, 200).unwrap())
                .unwrap();
        let direct = fold_render::render(
            registry
                .video(
                    &snapshot,
                    &source,
                    time.checked_add(Time::new(1, 24).unwrap()).unwrap(),
                    320,
                    200,
                )
                .unwrap(),
        )
        .unwrap();
        assert!(nested.pixels() == direct.pixels());
        assert_eq!(nested.pixels()[0][3], 0.5);
    }
    for time in [Time::ZERO, Time::new(1, 6).unwrap()] {
        let inactive =
            fold_render::render(registry.video(&snapshot, &output, time, 320, 200).unwrap())
                .unwrap();
        assert!(inactive.pixels().iter().all(|p| *p == [0.; 4]));
    }
    let Parameters::Read {
        source: Source::Document {
            source: invalid, ..
        },
        ..
    } = &mut composite.nodes[0].parameters
    else {
        panic!()
    };
    invalid.output.clear();
    assert!(composite.document(composite_id).is_err());
    let Parameters::Read {
        source: Source::Document {
            source: invalid, ..
        },
        ..
    } = &mut composite.nodes[0].parameters
    else {
        panic!()
    };
    invalid.output = "missing-output".into();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                composite.document(composite_id).unwrap(),
            )],
        })
        .unwrap();
    assert!(
        registry
            .video(
                &project.snapshot(),
                &output,
                Time::new(1, 24).unwrap(),
                320,
                200
            )
            .is_err()
    );
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn registered_motion_text_matches_gpu_across_output_sizes_and_retained_revisions() {
    use fold_app::media_workflow::{SceneRequest, evaluate_scene, evaluate_scene_gpu};
    use fold_render::gpu::{Host, Renderer};
    let (mut project, id, mut c) = fixture();
    c.canvas.width = 640;
    c.canvas.height = 220;
    let text = fold_motion::registry::resolve("fold.motion.text", 1, "packaged")
        .unwrap()
        .node(
            "Text",
            [(
                "text".into(),
                Binding::Literal {
                    value: Literal::Text {
                        value: "O B 8 O".into(),
                    },
                },
            )]
            .into(),
        )
        .unwrap();
    let fill = c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap();
    fill.parameters
        .insert("geometry".into(), Parameter::new(link(text.id)).unwrap());
    let stroke =
        fold_motion::registry::resolve("fold.motion.stroke", 1, "solid_centered_butt_miter")
            .unwrap()
            .node(
                "Stroke",
                [
                    ("geometry".into(), link(fill.id)),
                    (
                        "width".into(),
                        Binding::Literal {
                            value: Literal::Distance {
                                value: 2.,
                                unit: Unit::Canvas,
                            },
                        },
                    ),
                ]
                .into(),
            )
            .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.place")
        .unwrap()
        .parameters
        .insert("geometry".into(), Parameter::new(link(stroke.id)).unwrap());
    c.definitions[0].nodes.extend([text, stroke]);
    commit(&mut project, id, &c);
    let registry = fold_app::packages::builtins();
    let snapshot = project.snapshot();
    let output = registry.default_output(&snapshot, id).unwrap();
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!(
        "Registered Motion Screen adapter: {info:?}; output={}; revision={:?}",
        output.output,
        snapshot.revision()
    );
    let mut renderer = Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let request = |dimensions| SceneRequest {
        region: None,
        preview: None,
        source: output.clone(),
        time: Time::new(1, 24).unwrap(),
        dimensions,
    };
    let mut retained = None;
    for dimensions in [[160, 55], [640, 220], [1280, 440]] {
        let request = request(dimensions);
        let cpu = evaluate_scene(&snapshot, &request, &mut decoder, &cancel).unwrap();
        let gpu =
            evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel).unwrap();
        let timing = gpu.descriptor().unwrap().timing.unwrap();
        assert_eq!(timing.timestamp, request.time);
        assert_eq!(timing.duration, Time::new(1, 24).unwrap());
        assert_eq!(gpu.statistics.cpu_adapter_nodes, 0);
        assert_eq!(gpu.statistics.readback_bytes, 0);
        assert!(gpu.statistics.vector_instances > 0);
        let mut gpu = renderer.rgba16(gpu, &cancel).unwrap();
        let actual = gpu.readback(&cancel).unwrap();
        let error = actual
            .pixels()
            .iter()
            .flatten()
            .zip(cpu.pixels().iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(
            error <= 2e-3,
            "dimensions={dimensions:?}: RGBA16F max linear error {error}"
        );
        assert!(actual.pixels().iter().any(|p| p[3] > 0.1 && p[3] < 0.9));
        assert!(actual.pixels().iter().filter(|p| p[3] == 0.).count() > actual.pixels().len() / 2);
        let expected_display = cpu.over_black().to_display().unwrap();
        let display = renderer
            .output(&gpu, None, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
        let display_error = display
            .rgba()
            .iter()
            .zip(expected_display.rgba())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            display_error <= 1,
            "Display error at {dimensions:?}: {display_error}"
        );
        if let Some(directory) = std::env::var_os("FOLD_MOTION_SCREEN_CAPTURE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let mut ppm = format!("P6\n{} {}\n255\n", dimensions[0], dimensions[1]).into_bytes();
            for pixel in display.rgba().chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(directory.join(format!("screen-{}.ppm", dimensions[0])), ppm).unwrap();
        }
        // Recompile from the snapshot so resource reuse is independent of Arc identity.
        let again =
            evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel).unwrap();
        assert_eq!(again.statistics.vector_geometry_upload_bytes, 0);
        assert_eq!(again.statistics.vector_cpu_transform_fallbacks, 0);
        eprintln!(
            "Registered text {:?}: max linear error={error}; display byte error={display_error}; recompiled geometry upload=0",
            dimensions
        );
        if dimensions == [640, 220] {
            retained = Some((gpu, actual));
        }
    }
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap()
        .parameters
        .insert(
            "color".into(),
            Parameter::new(Binding::Literal {
                value: Literal::Color {
                    value: [0., 0., 1., 0.5],
                    encoding: Encoding::LinearRec709,
                    alpha: Alpha::Straight,
                },
            })
            .unwrap(),
        );
    commit(&mut project, id, &c);
    let expected = evaluate_scene(
        &project.snapshot(),
        &request([640, 220]),
        &mut decoder,
        &cancel,
    )
    .unwrap();
    let changed = evaluate_scene_gpu(
        &project.snapshot(),
        &request([640, 220]),
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    let mut changed = renderer.rgba16(changed, &cancel).unwrap();
    let actual = changed.readback(&cancel).unwrap();
    assert!(
        actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
            .all(|(a, b)| (a - b).abs() <= 2e-3)
    );
    let (mut held, original) = retained.unwrap();
    assert!(held.readback(&cancel).unwrap().pixels() == original.pixels());
    assert!(actual.pixels() != original.pixels());
    let old = evaluate_scene(&snapshot, &request([640, 220]), &mut decoder, &cancel).unwrap();
    assert!(
        old.pixels()
            .iter()
            .flatten()
            .zip(original.pixels().iter().flatten())
            .all(|(a, b)| (a - b).abs() <= 2e-3)
    );
    eprintln!(
        "Registered Screen retained revision {:?} after paint edit to {:?}",
        snapshot.revision(),
        project.snapshot().revision()
    );
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a native Vulkan adapter"]
fn word_animated_motion_matches_gpu_for_reordered_samples_and_retained_frames() {
    use fold_app::media_workflow::{SceneRequest, evaluate_scene, evaluate_scene_gpu};
    use fold_motion::{registry, types::Space};
    use fold_render::gpu::{Host, Renderer};
    let (mut project, id, mut c) = fixture();
    c.canvas.width = 1920;
    c.canvas.height = 1080;
    c.canvas.start = Time::ZERO;
    c.canvas.end = Time::new(4, 1).unwrap();
    let text = registry::resolve("fold.motion.text", 1, "packaged")
        .unwrap()
        .node("Text", Default::default())
        .unwrap();
    let vector = |y| Binding::Literal {
        value: Literal::Vector {
            value: vec![0., y],
            unit: Unit::Canvas,
            space: Space::Screen,
        },
    };
    let tween = registry::resolve("fold.motion.tween", 1, "cubic_out")
        .unwrap()
        .node(
            "Entrance",
            [
                ("from".into(), vector(80.)),
                ("to".into(), vector(0.)),
                (
                    "duration".into(),
                    Binding::Literal {
                        value: Literal::Duration {
                            value: Time::new(3, 5).unwrap(),
                        },
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    let animator = registry::resolve("fold.motion.text_animator", 1, "word_position")
        .unwrap()
        .node(
            "Words",
            [
                ("text".into(), link(text.id)),
                ("position".into(), link(tween.id)),
            ]
            .into(),
        )
        .unwrap();
    c.definitions[0]
        .nodes
        .iter_mut()
        .find(|n| n.operation == "fold.motion.fill")
        .unwrap()
        .parameters
        .get_mut("geometry")
        .unwrap()
        .value = serde_json::to_value(link(animator.id)).unwrap();
    c.definitions[0].nodes.extend([text, tween, animator]);
    commit(&mut project, id, &c);
    let snapshot = project.snapshot();
    let source = fold_app::packages::builtins()
        .default_output(&snapshot, id)
        .unwrap();
    let (host, info) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("Word animation GPU: {info:?}");
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut held = None;
    let mut first_pixels = None;
    let mut differs = false;
    for time in [
        Time::new(1, 4).unwrap(),
        Time::ZERO,
        Time::new(1, 2).unwrap(),
        Time::new(3, 4).unwrap(),
        Time::new(1, 4).unwrap(),
    ] {
        let request = SceneRequest {
            source: source.clone(),
            time,
            dimensions: [640, 360],
            region: None,
            preview: None,
        };
        let cpu = evaluate_scene(&snapshot, &request, &mut decoder, &cancel).unwrap();
        let gpu =
            evaluate_scene_gpu(&snapshot, &request, &mut renderer, &mut decoder, &cancel).unwrap();
        assert_eq!(gpu.statistics.cpu_adapter_nodes, 0);
        assert_eq!(gpu.statistics.readback_bytes, 0);
        assert_eq!(gpu.descriptor().unwrap().timing.unwrap().timestamp, time);
        let mut gpu = renderer.rgba16(gpu, &cancel).unwrap();
        let pixels = gpu.readback(&cancel).unwrap();
        let max_error = cpu
            .pixels()
            .iter()
            .zip(pixels.pixels())
            .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
            .fold(0f32, f32::max);
        assert!(max_error <= 0.002, "CPU/GPU error {max_error}");
        if let Some(first) = &first_pixels {
            differs |= pixels.pixels() != first;
        } else {
            first_pixels = Some(pixels.pixels().to_vec());
        }
        if held.is_none() {
            held = Some(gpu);
        }
        eprintln!("Word animation {time:?}: CPU/GPU max error {max_error}");
    }
    assert!(differs, "animation must change rendered pixels");
    assert!(held.unwrap().readback(&cancel).unwrap().pixels() == first_pixels.unwrap());
    eprintln!(
        "Word animation: reordered samples, RGBA16F, exact timing, native paths and held frames passed"
    );
}
