#![cfg(target_os = "linux")]
mod support;
use fold_app::session::Session;
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::desktop::DesktopClient;
use fold_project::{EditBatch, Mutation, Project};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn mcp_subprocess_uses_shared_schema_and_native_proposal_apply() {
    workflow(Workflow::Author);
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_native_motion_frames_match_candidate_and_committed_artwork() {
    workflow(Workflow::Render);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_capture_prepares_editable_shapes_without_changing_pixels_or_committed_state() {
    workflow(Workflow::Capture);
}
#[test]
#[cfg(feature = "spatial")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_motion_and_3d_outputs_survive_save_reopen_compositing_and_timeline() {
    workflow(Workflow::Mixed);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_native_image_edits_match_candidate_and_committed_artwork() {
    workflow(Workflow::Image);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_anchor_animation_candidates_keep_native_identity_pixels_and_undo() {
    workflow(Workflow::Anchor);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_animated_subdivision_candidates_keep_native_identity_pixels_and_undo() {
    workflow(Workflow::Subdivision);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_bezier_subdivision_candidates_keep_native_identity_pixels_and_undo() {
    workflow(Workflow::BezierSubdivision);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mcp_animated_anchor_removal_keeps_path_pixels_save_and_undo() {
    workflow(Workflow::Removal);
}
enum Workflow {
    BezierSubdivision,
    Removal,
    Subdivision,
    Anchor,
    Image,
    Author,
    Render,
    Capture,
    Mixed,
}
const GRADIENT_SCRIPT: &str = r#"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
    let x = input.seconds() * 246.;
    let image = input.image("Photo")?;
    Ok(fframes::svgr!(<g>
        <defs><linearGradient id="ramp">
            <stop offset="0" stop-color="red"/>
            <stop offset="0.4" stop-color="green" stop-opacity="0.6"/>
            <stop offset="1" stop-color="blue"/>
        </linearGradient>
        <pattern id="tile" patternUnits="userSpaceOnUse" x="3" width="16" height="16">
            <image href={image.clone()} width="16" height="16" preserveAspectRatio="none"/>
        </pattern></defs>
        <rect x={x} width="160" height="100" fill="url(#ramp)"/>
        <image href={image} x="320" y="20" width="160" height="100"/>
        <rect x="500" width="80" height="80" fill="url(#tile)"/>
    </g>))
}"#;

#[test]
#[cfg(feature = "spatial")]
#[ignore = "requires native Vulkan GPU sharing and the fframes compiler/bridge"]
fn mixed_motion_and_3d_float_exr_matches_linear_preview() {
    use fold_platform::delivery::{Channels, Compression, Precision, Settings};
    let mut project = Project::new(16);
    let motion = DocumentId::new();
    let mut canvas = support::motion();
    canvas.width = 640;
    canvas.height = 360;
    let object = canvas.objects.values_mut().next().unwrap();
    object.transform.y.value = 100.;
    object.transform.shear.value = 0.25;
    object.paint.fill = Some([0.35, 0.2, 0.95, 0.5].into());
    object.content = fold_motion_canvas::model::Content::Script {
        source: r#"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
            let x = input.seconds() * 246.;
            let shader = fframes::Shader::sksl(
                "half4 main(float2 p) { return half4(-0.25, 2.0, 0.125, 0.5); }"
            );
            let layer = shader.draw(&fframes::Frame::new(0, 0, 30), Default::default());
            let href = layer.href();
            Ok(fframes::svgr!(<g>
                <rect x={x} width="160" height="100"/>
                <image href={href} x="380" y="0" width="48" height="48"/>
            </g>))
        }"#
        .into(),
        controls: Default::default(),
        extensions: Default::default(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(motion).unwrap())],
        })
        .unwrap();
    let (_, _, timeline) = mixed_documents(&mut project, motion);
    let snapshot = project.snapshot();
    let source = fold_project::DocumentRef {
        document: timeline,
        output: "video".into(),
        extensions: Default::default(),
    };
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        destination: directory
            .path()
            .join("mixed.####.exr")
            .to_str()
            .unwrap()
            .into(),
        channels: Channels::Rgba,
        precision: Precision::Float,
        compression: Compression::Zip1,
        color_space: fold_color::LINEAR_SRGB.into(),
        first: 15,
        last: 16,
        step: 1,
        premultiplied: true,
        create_folders: true,
    };
    let (host, _) =
        pollster::block_on(fold_render::gpu::Host::headless(256 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut previews = vec![];
    for frame in settings.first..=settings.last {
        let mut preview = fold_app::output::evaluate_scene_gpu(
            &snapshot,
            &fold_app::output::SceneRequest {
                source: source.clone(),
                time: fold_foundation::Time::new(i64::from(frame), 30).unwrap(),
                dimensions: [640, 360],
                region: None,
                preview: None,
            },
            &mut renderer,
            &mut decoder,
            &cancel,
        )
        .unwrap();
        assert_eq!(preview.statistics.cpu_adapter_nodes, 0);
        assert_eq!(preview.statistics.readback_bytes, 0);
        previews.push(preview.readback(&cancel).unwrap().pixels().to_vec());
    }
    assert_ne!(
        previews[0], previews[1],
        "exact frame time must move the script"
    );
    fold_app::output::export_frames(&snapshot, &source, &settings, &cancel, Some(host.clone()))
        .unwrap();
    let straight_settings = Settings {
        destination: directory
            .path()
            .join("straight.####.exr")
            .to_str()
            .unwrap()
            .into(),
        premultiplied: false,
        ..settings.clone()
    };
    fold_app::output::export_frames(&snapshot, &source, &straight_settings, &cancel, Some(host))
        .unwrap();
    let decode = |path: std::path::PathBuf| {
        let exr = fold_media::exr::Source {
            info: fold_media::exr::inspect(&path, &cancel).unwrap(),
            fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
            path,
        };
        fold_media::exr::decode(
            &exr,
            [Some("R"), Some("G"), Some("B"), Some("A")],
            [640, 360],
            &cancel,
        )
        .unwrap()
    };
    let mut evidence = vec![];
    for (frame, preview) in (settings.first..=settings.last).zip(previews) {
        let decoded = decode(settings.path(frame).unwrap());
        assert_eq!(decoded.pixels.len(), preview.len());
        let max_error = decoded
            .pixels
            .iter()
            .flatten()
            .zip(preview.iter().flatten())
            .map(|(actual, expected)| (actual - expected).abs())
            .fold(0_f32, f32::max);
        assert_eq!(
            max_error, 0.,
            "Float EXR must preserve linear preview pixels"
        );
        assert!(preview.iter().flatten().all(|value| value.is_finite()));
        let negative = preview.iter().filter(|pixel| pixel[0] < -0.2).count();
        let hdr = preview.iter().filter(|pixel| pixel[1] > 1.9).count();
        assert!(
            negative > 100 && hdr > 100,
            "signed HDR must survive the mixed graph"
        );
        assert!(
            decoded
                .pixels
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        let straight = decode(straight_settings.path(frame).unwrap());
        assert_eq!(straight.pixels.len(), preview.len());
        assert!(
            straight
                .pixels
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        let mut straight_error = 0_f32;
        for (actual, expected) in straight.pixels.iter().zip(&preview) {
            assert_eq!(
                actual[3], expected[3],
                "straight export must retain coverage"
            );
            if expected[3] == 0. {
                assert_eq!(
                    actual[..3],
                    [0.; 3],
                    "transparent straight pixels must have zero RGB"
                );
            } else {
                for channel in 0..3 {
                    straight_error = straight_error
                        .max((actual[channel] - expected[channel] / expected[3]).abs());
                }
            }
        }
        assert_eq!(
            straight_error, 0.,
            "straight Float EXR must preserve unassociated RGB"
        );
        let transparent = preview.iter().filter(|pixel| pixel[3] == 0.).count();
        let translucent = preview
            .iter()
            .filter(|pixel| (pixel[3] - 0.5).abs() < 0.01)
            .count();
        let alpha_min = preview.iter().map(|pixel| pixel[3]).fold(1_f32, f32::min);
        let alpha_max = preview.iter().map(|pixel| pixel[3]).fold(0_f32, f32::max);
        assert!(
            transparent > 0 && translucent > 0,
            "alpha {alpha_min}..{alpha_max}: transparent={transparent}, half-opacity={translucent}"
        );
        evidence.push(
            json!({"frame":frame,"time":[frame,30],"pixels":preview.len(),
            "transparent_pixels":transparent,"half_opacity_pixels":translucent,"alpha_range":[alpha_min,alpha_max],"max_error":max_error,"straight_rgb_max_error":straight_error,"straight_alpha_unchanged":true,"straight_transparent_rgb_zero":true,
            "negative_red_pixels":negative,"hdr_green_pixels":hdr,
            "preview_cpu_adapter_nodes":0,"preview_readback_bytes_before_review":0}),
        );
    }
    if let Ok(path) = std::env::var("FOLD_MOTION_EXPORT_EVIDENCE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "workflow":"Motion + 3D -> Compositor Over -> Timeline -> Float RGBA EXR",
                "dimensions":[640,360],"color_space":fold_color::LINEAR_SRGB,
                "alpha_modes":["premultiplied","straight"],"animated_frames_differ":true,"frames":evidence
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

fn mixed_documents(
    project: &mut Project,
    motion: DocumentId,
) -> (DocumentId, DocumentId, DocumentId) {
    use fold_compositor::{Composite, Node, OperationConfig as P, Source};
    use fold_foundation::Time;
    use fold_project::DocumentRef;
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let spatial = DocumentId::new();
    let composite = DocumentId::new();
    let timeline = DocumentId::new();
    let info = fold_media::VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    };
    let reference = |document| DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let read = |document| {
        Node::new(
            P::Read {
                options: Default::default(),
                source: Source::Document {
                    source: reference(document),
                    info: info.clone(),
                },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: Time::new(5, 1).unwrap(),
            },
            vec![],
        )
    };
    let foreground = read(motion);
    let background = read(spatial);
    let merge = Node::new(
        P::Composite {
            mode: fold_render::operations::MergeMode::Over,
            channels: Default::default(),
        },
        vec![foreground.id, background.id],
    );
    let output = Node::new(P::Output, vec![merge.id]);
    let graph = Composite {
        info: info.clone(),
        output: output.id,
        nodes: vec![foreground, background, merge, output],
        extensions: Default::default(),
    };
    let track = Track::new(TrackKind::Video, "Mixed contexts");
    let sequence = Sequence {
        dimensions: [640, 360],
        rate: [30, 1],
        clips: vec![Clip {
            id: ObjectId::new(),
            track: track.id,
            link: None,
            asset: Default::default(),
            info: SourceMedia::Document {
                source: reference(composite),
                info: info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(5, 1).unwrap(),
            level: 1.,
            effects: vec![],
            extensions: Default::default(),
        }],
        tracks: vec![track],
        transitions: vec![],
        extensions: Default::default(),
    };
    let mut scene = fold_three_d::model::Scene::new(info);
    let (sx, cx) = (10_f64.to_radians()).sin_cos();
    let (sy, cy) = (15_f64.to_radians()).sin_cos();
    scene
        .objects
        .get_mut(&scene.roots[0])
        .unwrap()
        .transform
        .rotation
        .value = serde_json::to_value(fold_graph::binding::Binding::Literal {
        value: fold_graph::types::Literal::Rotation {
            value: [sx * cy, cx * sy, sx * sy, cx * cy],
            space: fold_graph::types::Space::World,
        },
    })
    .unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(scene.document(spatial).unwrap()),
                Mutation::PutDocument(graph.document(composite).unwrap()),
                Mutation::PutDocument(sequence.document(timeline).unwrap()),
            ],
        })
        .unwrap();
    (spatial, composite, timeline)
}

#[test]
#[ignore = "requires the native fframes compiler/bridge"]
fn artist_conversion_command_runs_in_background_and_restores_script_with_undo() {
    use fold_platform::{desktop::DesktopCommand, packages::CommandRequest};
    let document = DocumentId::new();
    let object = ObjectId::new();
    let mut script = fold_motion_canvas::tools::new_object("script").unwrap();
    if let fold_motion_canvas::model::Content::Script { source, .. } = &mut script.content {
        *source = r#"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
            let x = input.seconds() * 100.;
            Ok(fframes::svgr!(<circle cx={x} cy="50" r="20"/>))
        }"#
        .into();
    }
    let canvas = fold_motion_canvas::model::Canvas {
        roots: vec![object],
        objects: [(object, script)].into(),
        ..Default::default()
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    let mut session = Session::new(project);
    assert!(session.supports_extension(fold_motion_context::package::CONVERT));
    let before = session.snapshot().unwrap();
    let request = CommandRequest {
        id: fold_motion_context::package::CONVERT.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&fold_motion_context::package::ConvertArgs {
            document,
            object,
            time: fold_foundation::Time::new(1, 2).unwrap(),
        })
        .unwrap(),
    };
    session.command(DesktopCommand::Extension(request.clone()));
    assert_eq!(session.snapshot().unwrap().state(), before.state());
    let deadline = Instant::now() + Duration::from_secs(120);
    while session.extension_pending(&request) {
        assert!(
            Instant::now() < deadline,
            "Artist conversion did not finish"
        );
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let converted = fold_motion_canvas::model::Canvas::from_document(
        &session.snapshot().unwrap().state().documents[&document],
    )
    .unwrap();
    assert!(matches!(
        converted.objects[&object].content,
        fold_motion_canvas::model::Content::Group { .. }
    ));
    session.command(DesktopCommand::Undo);
    assert_eq!(
        fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&document]
        )
        .unwrap(),
        canvas
    );
    session.command(DesktopCommand::Redo);
    assert_eq!(
        fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&document]
        )
        .unwrap(),
        converted
    );
    let before_stale = session.snapshot().unwrap();
    session.command(DesktopCommand::Extension(request.clone()));
    while session.extension_pending(&request) {
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.snapshot().unwrap().state(), before_stale.state());
    assert!(session.extension_error(&request).is_some());
    let mut different = request.clone();
    different.arguments.push(b' ');
    assert!(session.extension_error(&different).is_none());
    session.command(DesktopCommand::Undo);
    let before_cancel = session.snapshot().unwrap();
    let mut fresh = request;
    fresh.base = before_cancel.revision();
    session.command(DesktopCommand::Extension(fresh.clone()));
    session.command(DesktopCommand::CancelExtension(fresh.clone()));
    while session.extension_pending(&fresh) {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(session.snapshot().unwrap().state(), before_cancel.state());
}
fn assert_point_close(actual: [f64; 2], expected: [f64; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
    }
}
fn workflow(mode: Workflow) {
    let frames = !matches!(mode, Workflow::Author);
    let capture = matches!(mode, Workflow::Capture);
    let mixed = matches!(mode, Workflow::Mixed);
    let image = matches!(mode, Workflow::Image);
    let anchor = matches!(
        mode,
        Workflow::Anchor | Workflow::Subdivision | Workflow::BezierSubdivision | Workflow::Removal
    );
    let removal = matches!(mode, Workflow::Removal);
    let subdivision = matches!(mode, Workflow::Subdivision | Workflow::BezierSubdivision);
    let bezier = matches!(mode, Workflow::BezierSubdivision);
    let directory = tempfile::tempdir().unwrap();
    let id = DocumentId::new();
    let mut project = Project::new(16);
    let mut canvas = support::motion();
    canvas.width = 640;
    canvas.height = 360;
    if frames {
        canvas
            .objects
            .values_mut()
            .next()
            .unwrap()
            .transform
            .shear
            .value = 0.25;
    }
    if mixed {
        canvas.objects.values_mut().next().unwrap().content =
            fold_motion_canvas::model::Content::Script {
                source:
                    r#"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
                let x = input.seconds() * 246.;
                Ok(fframes::svgr!(<rect x={x} width="160" height="100"/>))
            }"#
                    .into(),
                controls: Default::default(),
                extensions: Default::default(),
            };
    }
    let capture_asset = if image || capture {
        let path = directory.path().join("image.ppm");
        let bytes = b"P6\n2 1\n255\n\xff\x00\x00\x00\x00\xff";
        std::fs::write(&path, bytes).unwrap();
        let asset = fold_foundation::AssetId::new();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: path.canonicalize().unwrap().to_str().unwrap().into(),
                    fingerprint: format!("sha256:{}", fold_media::content_hash(bytes)),
                    extensions: Default::default(),
                })],
            })
            .unwrap();
        if image {
            let object = canvas.objects.values_mut().next().unwrap();
            let transform = object.transform.clone();
            *object = fold_motion_canvas::tools::image(asset, [160, 100]).unwrap();
            object.transform = transform;
        }
        Some(asset)
    } else {
        None
    };
    let anchor_track = if anchor {
        use fold_graph::{
            binding::{Interpolation, Key, Track},
            types::{Literal, Space, Unit},
        };
        use fold_motion_canvas::model::{Content, Contour, Point};
        let object = canvas.objects.values_mut().next().unwrap();
        let point = |position, incoming, outgoing| Point {
            id: ObjectId::new(),
            position,
            position_track: None,
            incoming_track: None,
            outgoing_track: None,
            incoming,
            outgoing,
            extensions: Default::default(),
        };
        object.content = Content::Path {
            contours: vec![Contour {
                id: ObjectId::new(),
                points: vec![
                    point([40., 40.], [0.; 2], [40., -20.]),
                    point([340., 180.], [-40., 20.], [0.; 2]),
                ],
                closed: false,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        };
        object.paint.fill = None;
        object.paint.stroke = Some([0.2, 0.9, 0.5, 0.7].into());
        object.paint.stroke_width.value = 7.;
        let vector = |value| Literal::Vector {
            value,
            unit: Unit::Canvas,
            space: Space::Screen,
        };
        let mut track =
            Track::constant(vector(vec![40., 40.]), fold_foundation::Time::ZERO).unwrap();
        track.keys.push(Key {
            id: ObjectId::new(),
            time: fold_foundation::Time::new(1, 1).unwrap(),
            value: vector(vec![200., 160.]),
            interpolation: Interpolation::Linear,
            handles: vec![],
            extensions: Default::default(),
        });
        if bezier {
            for key in &mut track.keys {
                key.interpolation = Interpolation::Bezier;
                key.handles = vec![
                    fold_graph::binding::Handles {
                        tangents: fold_animation::Tangents::Broken,
                        ..Default::default()
                    };
                    2
                ];
            }
            // Flat temporal tangents produce easing, not the linear fixture.
            let sampled = track
                .sample(fold_foundation::Time::new(1, 4).unwrap())
                .unwrap();
            let Literal::Vector { value, .. } = sampled else {
                unreachable!()
            };
            assert!((value[0] - 65.).abs() < 1e-8);
            assert!((value[1] - 58.75).abs() < 1e-8);
        }
        Some(serde_json::to_value(track).unwrap())
    } else {
        None
    };
    let original_canvas = canvas.clone();
    let object_id = *canvas.objects.keys().next().unwrap();
    let image_object = image.then(|| serde_json::to_value(&canvas.objects[&object_id]).unwrap());
    let initial_canvas = if image {
        let mut empty = canvas.clone();
        empty.roots.clear();
        empty.objects.clear();
        empty
    } else {
        canvas.clone()
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(initial_canvas.document(id).unwrap())],
        })
        .unwrap();
    let linked = mixed.then(|| mixed_documents(&mut project, id));
    if mixed {
        let path = directory.path().join("mixed.fold");
        fold_project::save(&project.snapshot(), &path).unwrap();
        let loaded = fold_project::load(&path, 16).unwrap();
        assert_eq!(loaded.snapshot().state(), project.snapshot().state());
        project = loaded;
    }
    let frame_id = linked.map_or(id, |(_, _, timeline)| timeline);
    let saved_gradient = directory.path().join("gradient.fold");
    let gradient_destination = saved_gradient.clone();
    let mut session = Session::new(project);
    #[cfg(feature = "gpu")]
    if frames {
        session.set_render_host(
            pollster::block_on(fold_render::gpu::Host::headless(256 * 1024 * 1024))
                .unwrap()
                .0,
        );
    }
    #[cfg(not(feature = "gpu"))]
    assert!(!frames);
    let path = session.enable_agent_transport(directory.path()).unwrap();
    let worker = std::thread::spawn(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .arg("mcp")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut rpc_id = 0;
        let mut exchange = |method: &str, params: Value| {
            rpc_id += 1;
            writeln!(
                input,
                "{}",
                json!({"jsonrpc":"2.0","id":rpc_id,"method":method,"params":params})
            )
            .unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["id"], rpc_id);
            assert!(response.get("error").is_none(), "{response}");
            if method == "initialize" {
                writeln!(
                    input,
                    "{}",
                    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
                )
                .unwrap();
                input.flush().unwrap();
            }
            response["result"].clone()
        };
        assert_eq!(
            exchange(
                "initialize",
                json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}})
            )["protocolVersion"],
            "2025-11-25"
        );
        let schema = exchange("tools/list", json!({}))["tools"][0]["inputSchema"].clone();
        let mut tool = |command: Value| {
            let mut response = exchange(
                "tools/call",
                json!({"name":"fold_request","arguments":{"version":2,"command":command}}),
            );
            assert_eq!(
                response["isError"], false,
                "{}: {response}",
                command["method"]
            );
            let handle = response["structuredContent"]["result"]["job"].clone();
            while response["structuredContent"]["result"]["state"] == "running" {
                response = exchange(
                    "tools/call",
                    json!({"name":"fold_request","arguments":{"version":2,"command":{"method":"job_status","handle":handle,"wait_ms":1000}}}),
                );
                assert_eq!(response["isError"], false, "{response}");
            }
            assert_ne!(
                response["structuredContent"]["result"]["state"], "failed",
                "{}: {response}",
                command["method"]
            );
            response["structuredContent"].clone()
        };
        let mut discovery = tool(json!({"method":"discover"}));
        assert_eq!(schema, discovery["result"]["request_schema"]);
        if let Some(object) = image_object {
            let inspected = tool(
                json!({"method":"inspect_document","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"scope":{"document":id}}),
            );
            let native: Value =
                serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
            let guard = || json!({"epoch":discovery["epoch"],"request_id":ObjectId::new(),"base":discovery["result"]["revision"],"scope":{"document":id}});
            let observe = |target| json!({"method":"observe_frame","target":target,"output":"video","time":{"numerator":0,"denominator":1},"dimensions":[640,360]});
            let committed = json!({"kind":"committed","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"document":id});
            let empty = tool(observe(committed.clone()));
            let proposed = tool(
                json!({"method":"propose","guard":guard(),"edit":{"kind":"native","request":{"op":"author","document":id,"expected":native,"edits":[{"op":"insert","id":object_id,"object":object,"parent":null,"index":0}]}}}),
            );
            let handle = proposed["result"]["handle"].clone();
            let inspection = tool(json!({"method":"inspect_candidate","handle":handle}));
            let candidate = tool(observe(
                json!({"kind":"candidate","handle":handle,"document":id}),
            ));
            assert_ne!(candidate["result"]["image"], empty["result"]["image"]);
            assert_eq!(
                tool(observe(committed))["result"]["image"],
                empty["result"]["image"]
            );
            tool(
                json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]}),
            );
            discovery = tool(json!({"method":"discover"}));
            let published = tool(observe(
                json!({"kind":"committed","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"document":id}),
            ));
            assert_eq!(candidate["result"]["image"], published["result"]["image"]);
        }
        if let Some(track) = anchor_track {
            let inspected = tool(
                json!({"method":"inspect_document","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"scope":{"document":id}}),
            );
            let native: Value =
                serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
            let expected = native["native"]["objects"][object_id.to_string()]["content"]["contours"][0]["points"][0].clone();
            let mut value = expected.clone();
            value["position_track"] = track.clone();
            let mut handle_track = track;
            handle_track["id"] = json!(ObjectId::new());
            for key in handle_track["keys"].as_array_mut().unwrap() {
                key["id"] = json!(ObjectId::new());
            }
            handle_track["keys"][0]["value"]["value"] = json!([40., -20.]);
            handle_track["keys"][1]["value"]["value"] = json!([80., 40.]);
            value["outgoing_track"] = handle_track;
            let guard = || json!({"epoch":discovery["epoch"],"request_id":ObjectId::new(),"base":discovery["result"]["revision"],"scope":{"document":id}});
            let observe = |target| json!({"method":"observe_frame","target":target,"output":"video","time":{"numerator":1,"denominator":2},"dimensions":[640,360]});
            let committed = json!({"kind":"committed","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"document":id});
            let before = tool(observe(committed.clone()));
            let proposal = tool(
                json!({"method":"propose","guard":guard(),"edit":{"kind":"native","request":{"op":"author","document":id,"expected":native,"edits":[{"op":"point","object":object_id,"expected":expected,"value":value}]}}}),
            );
            let handle = proposal["result"]["handle"].clone();
            let inspection = tool(json!({"method":"inspect_candidate","handle":handle}));
            let candidate = tool(observe(
                json!({"kind":"candidate","handle":handle,"document":id}),
            ));
            assert_ne!(candidate["result"]["image"], before["result"]["image"]);
            // Review uses the same immutable candidate at both exact times.
            let mut review = observe(json!({"kind":"candidate","handle":handle,"document":id}));
            review["review"] = json!({"times":[{"numerator":0,"denominator":1}],"layout":{"kind":"contact_sheet","columns":2}});
            let sheet = tool(review.clone());
            assert_eq!(sheet["result"]["image"]["dimensions"], json!([1280, 360]));
            let decode = |v: &Value| {
                let s = v["result"]["image"]["data"].as_str().unwrap();
                (0..s.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                    .collect::<Vec<_>>()
            };
            let first = decode(&candidate);
            let sheet_pixels = decode(&sheet);
            for y in 0..360 {
                assert_eq!(
                    &sheet_pixels[y * 1280 * 4..y * 1280 * 4 + 640 * 4],
                    &first[y * 640 * 4..(y + 1) * 640 * 4]
                );
            }
            review["review"]["layout"] = json!({"kind":"onion"});
            let onion = tool(review);
            let onion_pixels = decode(&onion);
            for y in 0..360 {
                for x in 0..640 * 4 {
                    let a = u16::from(sheet_pixels[y * 1280 * 4 + x]);
                    let b = u16::from(sheet_pixels[y * 1280 * 4 + 640 * 4 + x]);
                    assert_eq!(onion_pixels[y * 640 * 4 + x], (a + b).div_ceil(2) as u8);
                }
            }
            assert_eq!(
                sheet["result"]["review"]["frames"],
                onion["result"]["review"]["frames"]
            );
            if let Ok(path) = std::env::var("FOLD_MOTION_REVIEW_EVIDENCE") {
                std::fs::write(
                    path,
                    serde_json::to_vec(&json!({"sheet":sheet,"onion":onion})).unwrap(),
                )
                .unwrap();
            }
            assert_eq!(
                tool(observe(committed))["result"]["image"],
                before["result"]["image"]
            );
            tool(
                json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]}),
            );
            discovery = tool(json!({"method":"discover"}));
            let published = tool(observe(
                json!({"kind":"committed","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"document":id}),
            ));
            assert_eq!(candidate["result"]["image"], published["result"]["image"]);
            let parameters = tool(
                json!({"method":"inspect_parameters","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"document":id,"offset":0,"limit":256}),
            );
            let target = parameters["result"]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| {
                    p["target"]["parameter"]["id"] == expected["id"]
                        && p["target"]["component"].is_null()
                })
                .unwrap()["target"]
                .clone();
            let sample = tool(
                json!({"method":"sample_parameter","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"target":target,"time":{"numerator":1,"denominator":2}}),
            );
            assert_point_close(
                serde_json::from_value(sample["result"]["value"]["value"].clone()).unwrap(),
                [120., 100.],
            );
            let property = format!("{}/outgoing", expected["id"].as_str().unwrap());
            let handle_target = parameters["result"]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| {
                    p["target"]["parameter"]["name"] == property
                        && p["target"]["component"].is_null()
                })
                .unwrap()["target"]
                .clone();
            let handle_sample = tool(
                json!({"method":"sample_parameter","epoch":discovery["epoch"],
                "revision":discovery["result"]["revision"],"target":handle_target,
                "time":{"numerator":1,"denominator":2}}),
            );
            assert_point_close(
                serde_json::from_value(handle_sample["result"]["value"]["value"].clone()).unwrap(),
                [60., 10.],
            );
        }
        if capture {
            let inspected = tool(
                json!({"method":"inspect_document","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"scope":{"document":id}}),
            );
            let native: Value =
                serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
            let expected = native["native"]["objects"][object_id.to_string()].clone();
            let mut object = expected.clone();
            let control = fold_motion_canvas::model::Control {
                name: "Photo".into(),
                value: fold_motion_canvas::model::ControlValue::Image(capture_asset.unwrap()),
                protected: false,
                extensions: Default::default(),
            };
            object["content"] = json!({"kind":"script","source":GRADIENT_SCRIPT,"controls":{ObjectId::new().to_string():control}});
            let guard = || json!({"epoch":discovery["epoch"],"request_id":ObjectId::new(),"base":discovery["result"]["revision"],"scope":{"document":id}});
            let proposed = tool(
                json!({"method":"propose","guard":guard(),"edit":{"kind":"native","request":{"op":"author","document":id,"expected":native,"edits":[{"op":"replace","id":object_id,"expected":expected,"object":object}]}}}),
            );
            let handle = proposed["result"]["handle"].clone();
            let inspected = tool(json!({"method":"inspect_candidate","handle":handle}));
            tool(
                json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspected["result"]["receipt"]}),
            );
            discovery = tool(json!({"method":"discover"}));
        }
        let committed = |revision| json!({"kind":"committed","epoch":discovery["epoch"],"revision":revision,"document":frame_id});
        let frame = |target| json!({"method":"observe_frame","target":target,"output":"video","time":{"numerator":1,"denominator":2},"dimensions":[640,360]});
        let before = frames.then(|| {
            assert_eq!(discovery["result"]["frame_observation"]["available"], true);
            tool(frame(committed(discovery["result"]["revision"].clone())))
        });
        let guard = || json!({"epoch":discovery["epoch"],"request_id":ObjectId::new(),"base":discovery["result"]["revision"],"scope":{"document":id}});
        let inspected = tool(
            json!({"method":"inspect_document","epoch":discovery["epoch"],"revision":discovery["result"]["revision"],"scope":{"document":id}}),
        );
        let native: Value =
            serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
        let request = if capture {
            json!({"op":"capture","document":id,"expected":native,"object":object_id,"time":{"numerator":1,"denominator":2}})
        } else if removal {
            let expected = native["native"]["objects"][object_id.to_string()].clone();
            let point = expected["content"]["contours"][0]["points"][0]["id"].clone();
            json!({"op":"author","document":id,"expected":native,"edits":[{
                "op":"remove_point","object":object_id,"expected":expected,"point":point}]})
        } else if subdivision {
            let expected = native["native"]["objects"][object_id.to_string()].clone();
            let contour = &expected["content"]["contours"][0];
            json!({"op":"author","document":id,"expected":native,"edits":[{
                "op":"split_segment","object":object_id,"expected":expected,
                "contour":contour["id"],"start":contour["points"][0]["id"],"fraction":0.35,
                "time":{"numerator":1,"denominator":2}}]})
        } else if anchor {
            let expected = native["native"]["objects"][object_id.to_string()].clone();
            let point = expected["content"]["contours"][0]["points"][0]["id"].clone();
            let mut after_anchor = expected.clone();
            after_anchor["content"]["contours"][0]["points"][0]["position_track"]["keys"][1]["value"]
                ["value"] = json!([280., 200.]);
            json!({"op":"author","document":id,"expected":native,"edits":[
                {"op":"move_point","object":object_id,"expected":expected,"point":point,
                    "part":"anchor","position":[330.,200.],"time":{"numerator":1,"denominator":1},"linked":false,"auto_key":false},
                {"op":"move_point","object":object_id,"expected":after_anchor,"point":point,
                    "part":"outgoing","position":[470.,280.],"time":{"numerator":1,"denominator":1},"linked":false,"auto_key":false}
            ]})
        } else {
            support::motion_request(id, native, 123.)
        };
        let proposal = tool(
            json!({"method":"propose","guard":guard(),"edit":{"kind":"native","request":request}}),
        );
        let handle = proposal["result"]["handle"].clone();
        let inspection = tool(json!({"method":"inspect_candidate","handle":handle}));
        let candidate = frames.then(|| {
            let candidate = tool(frame(
                json!({"kind":"candidate","handle":handle,"document":id}),
            ));
            let unchanged = tool(frame(committed(discovery["result"]["revision"].clone())));
            assert_eq!(
                unchanged["result"]["image"],
                before.as_ref().unwrap()["result"]["image"]
            );
            candidate
        });
        let parent_candidate = mixed.then(|| {
            tool(frame(
                json!({"kind":"candidate","handle":handle,"document":frame_id}),
            ))
        });
        let apply = json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]});
        let applied = tool(apply.clone());
        assert_eq!(tool(apply), applied);
        if let Some(candidate) = candidate {
            let current = tool(json!({"method":"discover"}));
            let after = tool(frame(committed(current["result"]["revision"].clone())));
            if let Some(parent) = &parent_candidate {
                assert_eq!(parent["result"]["image"], after["result"]["image"]);
                assert_eq!(parent["result"]["transfers"]["cpu_adapter_nodes"], 0);
                assert_eq!(
                    parent["result"]["transfers"]["evaluation_readback_bytes"],
                    0
                );
            }
            let linked_frames = linked.map(|(spatial, composite, _)| {
                let observe = |document| frame(json!({"kind":"committed","epoch":discovery["epoch"],"revision":current["result"]["revision"],"document":document}));
                let motion = tool(observe(id));
                let spatial = tool(observe(spatial));
                let composite = tool(observe(composite));
                assert_eq!(candidate["result"]["image"], motion["result"]["image"]);
                assert_eq!(after["result"]["image"], composite["result"]["image"]);
                assert_ne!(after["result"]["image"], motion["result"]["image"]);
                assert_ne!(after["result"]["image"], spatial["result"]["image"]);
                for response in [&motion, &spatial, &composite] {
                    assert_eq!(response["result"]["transfers"]["cpu_adapter_nodes"], 0);
                    assert_eq!(response["result"]["transfers"]["evaluation_readback_bytes"], 0);
                }
                json!({"motion":motion,"spatial":spatial,"composite":composite})
            });
            if !mixed {
                assert_eq!(candidate["result"]["image"], after["result"]["image"]);
            }
            let before = before.unwrap();
            if capture {
                assert_eq!(before["result"]["image"], after["result"]["image"]);
            } else if !subdivision {
                assert_ne!(before["result"]["image"], after["result"]["image"]);
            }
            for response in [&before, &candidate, &after] {
                assert_eq!(response["result"]["image"]["dimensions"], json!([640, 360]));
                assert_eq!(response["result"]["transfers"]["cpu_adapter_nodes"], 0);
                assert_eq!(
                    response["result"]["transfers"]["evaluation_readback_bytes"],
                    0
                );
            }
            let pixel = |response: &Value, x: usize, y: usize| {
                let data = response["result"]["image"]["data"].as_str().unwrap();
                let start = (y * 640 + x) * 8;
                data[start..start + 8].to_owned()
            };
            if !capture && !mixed && !image && !anchor {
                assert_ne!(pixel(&before, 40, 40), pixel(&after, 40, 40));
                // At y=80 the shear moves the left edge to x=20. An unsheared
                // rectangle would incorrectly cover this pixel.
                assert_ne!(pixel(&before, 5, 80), pixel(&before, 40, 40));
                assert_eq!(pixel(&before, 5, 80), pixel(&after, 40, 40));
                assert_eq!(pixel(&before, 40, 40), pixel(&after, 200, 40));
            }
            let gradient_edit = capture.then(|| {
                let current = tool(json!({"method":"discover"}));
                let inspected = tool(json!({"method":"inspect_document","epoch":current["epoch"],"revision":current["result"]["revision"],"scope":{"document":id}}));
                let native: Value = serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
                let (leaf, expected) = native["native"]["objects"].as_object().unwrap().iter().find(|(_,object)| object["paint"]["fill"]["kind"] == "linear").unwrap();
                let mut object = expected.clone();
                let stop_ids = object["paint"]["fill"]["stops"].as_array().unwrap().iter().map(|stop| stop["id"].clone()).collect::<Vec<_>>();
                object["paint"]["fill"]["stops"][1]["color"] = json!([1.,1.,0.,1.]);
                let guard = || json!({"epoch":current["epoch"],"request_id":ObjectId::new(),"base":current["result"]["revision"],"scope":{"document":id}});
                let proposed = tool(json!({"method":"propose","guard":guard(),"edit":{"kind":"native","request":{"op":"author","document":id,"expected":native,"edits":[{"op":"replace","id":leaf,"expected":expected,"object":object}]}}}));
                let handle = proposed["result"]["handle"].clone();
                let inspection = tool(json!({"method":"inspect_candidate","handle":handle}));
                let edited = tool(frame(json!({"kind":"candidate","handle":handle,"document":id})));
                assert_ne!(edited["result"]["image"], after["result"]["image"]);
                let unchanged = tool(frame(json!({"kind":"committed","epoch":current["epoch"],"revision":current["result"]["revision"],"document":id})));
                assert_eq!(unchanged["result"]["image"], after["result"]["image"]);
                tool(json!({"method":"apply","guard":guard(),"handle":handle,"receipt":inspection["result"]["receipt"]}));
                let updated = tool(json!({"method":"discover"}));
                let committed = tool(frame(json!({"kind":"committed","epoch":current["epoch"],"revision":updated["result"]["revision"],"document":id})));
                assert_eq!(committed["result"]["image"], edited["result"]["image"]);
                let inspected = tool(json!({"method":"inspect_document","epoch":current["epoch"],"revision":updated["result"]["revision"],"scope":{"document":id}}));
                let published: Value = serde_json::from_str(inspected["result"]["source"].as_str().unwrap()).unwrap();
                let published_stops = published["native"]["objects"][leaf]["paint"]["fill"]["stops"].as_array().unwrap();
                assert_eq!(published_stops.iter().map(|stop| stop["id"].clone()).collect::<Vec<_>>(), stop_ids);
                assert_eq!(published_stops[1]["color"], json!([1.,1.,0.,1.]));
                tool(json!({"method":"save_project_copy","guard":{"epoch":current["epoch"],"request_id":ObjectId::new(),"base":updated["result"]["revision"]},"path":gradient_destination}));
                for response in [&edited,&committed] {
                    assert_eq!(response["result"]["transfers"]["cpu_adapter_nodes"],0);
                    assert_eq!(response["result"]["transfers"]["evaluation_readback_bytes"],0);
                }
                json!({"candidate":edited,"committed":committed,"stop_ids":stop_ids})
            });
            if let Ok(path) = std::env::var("FOLD_MOTION_MCP_EVIDENCE") {
                std::fs::write(
                    path,
                    serde_json::to_vec_pretty(
                        &json!({"before":before,"candidate":candidate,"parent_candidate":parent_candidate,"after":after,"linked_frames":linked_frames,"gradient_edit":gradient_edit}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
        }
        drop(input);
        assert!(child.wait().unwrap().success());
    });
    let deadline = Instant::now() + Duration::from_secs(if frames { 720 } else { 20 });
    while !worker.is_finished() {
        assert!(Instant::now() < deadline, "MCP subprocess did not finish");
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap();
    let current = session.snapshot().unwrap();
    assert_eq!(
        fold_motion_canvas::model::Canvas::from_document(&current.state().documents[&id])
            .unwrap()
            .objects[&object_id]
            .transform
            .x
            .value,
        if capture || anchor { 0. } else { 123. }
    );
    if anchor && !removal {
        let path = directory.path().join("anchor.fold");
        fold_project::save(&current, &path).unwrap();
        assert_eq!(
            fold_project::load(&path, 16).unwrap().snapshot().state(),
            current.state()
        );
        let after =
            fold_motion_canvas::model::Canvas::from_document(&current.state().documents[&id])
                .unwrap();
        let fold_motion_canvas::model::Content::Path { contours, .. } =
            &after.objects[&object_id].content
        else {
            unreachable!()
        };
        let point = &contours[0].points[0];
        if subdivision {
            assert_eq!(contours[0].points.len(), 3);
            let inserted = &contours[0].points[1];
            assert_ne!(inserted.id, point.id);
            assert!(inserted.position_track.is_some());
            assert!(inserted.incoming_track.is_some());
            assert!(inserted.outgoing_track.is_some());
        }
        assert_point_close(
            point
                .position_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            if subdivision {
                [120., 100.]
            } else {
                [160., 120.]
            },
        );
        assert_point_close(
            point
                .outgoing_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            if subdivision { [21., 3.5] } else { [80., 30.] },
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        let restored = fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&id],
        )
        .unwrap();
        let fold_motion_canvas::model::Content::Path { contours, .. } =
            &restored.objects[&object_id].content
        else {
            unreachable!()
        };
        assert_eq!(contours[0].points[0].id, point.id);
        assert_point_close(
            contours[0].points[0]
                .position_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            [120., 100.],
        );
        assert_eq!(
            contours[0].points[0].position_track.as_ref().unwrap().id,
            point.position_track.as_ref().unwrap().id
        );
        assert_point_close(
            contours[0].points[0]
                .outgoing_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            [60., 10.],
        );
        assert_eq!(
            contours[0].points[0].outgoing_track.as_ref().unwrap().id,
            point.outgoing_track.as_ref().unwrap().id
        );
        assert_eq!(
            contours[0].points[0].outgoing_track.as_ref().unwrap().keys[1].id,
            point.outgoing_track.as_ref().unwrap().keys[1].id
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(
                &session.snapshot().unwrap().state().documents[&id]
            )
            .unwrap(),
            original_canvas
        );
    }
    if removal {
        let path = directory.path().join("removed-anchor.fold");
        fold_project::save(&current, &path).unwrap();
        assert_eq!(
            fold_project::load(&path, 16).unwrap().snapshot().state(),
            current.state()
        );
        let after =
            fold_motion_canvas::model::Canvas::from_document(&current.state().documents[&id])
                .unwrap();
        let fold_motion_canvas::model::Content::Path {
            contours: original, ..
        } = &original_canvas.objects[&object_id].content
        else {
            unreachable!()
        };
        let fold_motion_canvas::model::Content::Path { contours, .. } =
            &after.objects[&object_id].content
        else {
            unreachable!()
        };
        assert_eq!(contours[0].id, original[0].id);
        assert_eq!(contours[0].points, vec![original[0].points[1].clone()]);
        assert_eq!(
            after.objects[&object_id].paint,
            original_canvas.objects[&object_id].paint
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        let restored = fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&id],
        )
        .unwrap();
        let fold_motion_canvas::model::Content::Path { contours, .. } =
            &restored.objects[&object_id].content
        else {
            unreachable!()
        };
        assert_eq!(contours[0].points[0].id, original[0].points[0].id);
        assert_point_close(
            contours[0].points[0]
                .position_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            [120., 100.],
        );
        assert_point_close(
            contours[0].points[0]
                .outgoing_at(fold_foundation::Time::new(1, 2).unwrap())
                .unwrap(),
            [60., 10.],
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(
                &session.snapshot().unwrap().state().documents[&id]
            )
            .unwrap(),
            original_canvas
        );
    }
    if image {
        let path = directory.path().join("image.fold");
        fold_project::save(&current, &path).unwrap();
        assert_eq!(
            fold_project::load(&path, 16).unwrap().snapshot().state(),
            current.state()
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(
                &session.snapshot().unwrap().state().documents[&id]
            )
            .unwrap(),
            original_canvas
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        let removed = session.snapshot().unwrap();
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(&removed.state().documents[&id])
                .unwrap(),
            initial_canvas
        );
        assert_eq!(removed.state().assets, current.state().assets);
        session.command(fold_platform::desktop::DesktopCommand::Redo);
        session.command(fold_platform::desktop::DesktopCommand::Redo);
        let redone = session.snapshot().unwrap();
        assert_eq!(redone.state().assets, current.state().assets);
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(&redone.state().documents[&id])
                .unwrap(),
            fold_motion_canvas::model::Canvas::from_document(&current.state().documents[&id])
                .unwrap()
        );
    }
    if capture {
        let canvas =
            fold_motion_canvas::model::Canvas::from_document(&current.state().documents[&id])
                .unwrap();
        assert!(matches!(
            canvas.objects[&object_id].content,
            fold_motion_canvas::model::Content::Group { .. }
        ));
        assert!(canvas.objects.values().all(|object| !matches!(
            object.content,
            fold_motion_canvas::model::Content::Script { .. }
        )));
        assert!(
            canvas
                .objects
                .values()
                .any(|object| matches!(object.content,
            fold_motion_canvas::model::Content::Image {asset,..} if Some(asset)==capture_asset))
        );
        assert!(canvas.objects.values().any(|object| matches!(object.paint.fill,
            Some(fold_motion_canvas::model::PaintSource::ImagePattern {asset,..}) if Some(asset)==capture_asset)));
        assert_eq!(canvas.assets(), vec![capture_asset.unwrap()]);
        let loaded = fold_project::load(&saved_gradient, 16).unwrap();
        assert_eq!(
            loaded.snapshot().state().documents,
            current.state().documents
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        let restored = fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&id],
        )
        .unwrap();
        assert_ne!(restored, canvas);
        assert!(matches!(
            restored.objects[&object_id].content,
            fold_motion_canvas::model::Content::Group { .. }
        ));
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        let restored = fold_motion_canvas::model::Canvas::from_document(
            &session.snapshot().unwrap().state().documents[&id],
        )
        .unwrap();
        assert!(
            matches!(&restored.objects[&object_id].content, fold_motion_canvas::model::Content::Script { source, .. } if source == GRADIENT_SCRIPT)
        );
        session.command(fold_platform::desktop::DesktopCommand::Undo);
        assert_eq!(
            fold_motion_canvas::model::Canvas::from_document(
                &session.snapshot().unwrap().state().documents[&id]
            )
            .unwrap(),
            original_canvas
        );
    }
}
