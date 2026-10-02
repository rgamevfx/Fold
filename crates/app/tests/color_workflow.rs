use fold_app::{color, media_workflow as workflow};
use fold_foundation::{DocumentId, Time};
use fold_project::{EditBatch, Mutation, Project};

#[test]
fn application_defaults_are_aces_and_missing_contract_stays_legacy() {
    let project = color::new_project(8);
    assert_eq!(
        fold_platform::color::project(&project.snapshot())
            .unwrap()
            .unwrap()
            .working_space,
        "ACEScg"
    );
    assert!(
        fold_platform::color::project(&Project::new(8).snapshot())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fold_color::DisplayTransform::default().display,
        "sRGB - Display"
    );
}

fn scene(snapshot: &fold_project::Snapshot, document: DocumentId) -> fold_render::Frame {
    workflow::evaluate_scene(
        snapshot,
        &workflow::SceneRequest {
            source: fold_project::DocumentRef {
                document,
                output: "video".into(),
                extensions: Default::default(),
            },
            time: Time::ZERO,
            dimensions: [16, 16],
        },
        &mut fold_media::Decoder::default(),
        &fold_media::Cancel::default(),
    )
    .unwrap()
}

#[test]
#[ignore = "requires packaged OCIO runtime; set FOLD_COLOR_ROOT"]
fn persisted_color_undo_legacy_and_matched_cli_preview_output() {
    let mut project = color::new_project(16);
    let id = DocumentId::new();
    let mut document = fold_motion::Solid {
        rgb: [-0.05, 0.18, 2.],
    }
    .document(id)
    .unwrap();
    document
        .extensions
        .insert("future".into(), serde_json::json!({"bytes":[8,4,1]}));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let original_key = workflow::content_for(&snapshot, id).unwrap();
    let frame = scene(&snapshot, id);
    assert_eq!(frame.working_space(), fold_render::WorkingSpace::AcesCg);
    assert_eq!(frame.pixels()[0], [-0.05, 0.18, 2., 1.]);
    let descriptor = frame.descriptor();
    assert_eq!(
        descriptor.timing.unwrap().duration,
        Time::new(1, 24).unwrap()
    );
    assert_eq!(descriptor.planes[0].row_stride, 16 * 16);
    let preview = color::preview(&snapshot, &frame).unwrap();
    let mut output = fold_platform::color::OutputTransform {
        display: "sRGB - Display".into(),
        ..Default::default()
    };
    output
        .extensions
        .insert("future-output".into(), serde_json::json!(17));
    project
        .commit(color::set_output(&snapshot, id, output.clone()).unwrap())
        .unwrap();
    assert_eq!(
        workflow::content_for(&project.snapshot(), id).unwrap(),
        original_key,
        "delivery settings never invalidate a viewer's scene/presentation"
    );
    let delivery = color::delivery(&project.snapshot(), id, &frame).unwrap();
    assert_eq!(preview.rgba(), delivery.rgba());
    project.undo().unwrap();
    assert_eq!(
        fold_platform::color::output(&project.snapshot(), id)
            .unwrap()
            .display,
        "Rec.1886 Rec.709 - Display"
    );
    project.redo().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("color.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 16).unwrap();
    assert_eq!(
        fold_platform::color::output(&reopened.snapshot(), id).unwrap(),
        output
    );
    assert_eq!(
        reopened.snapshot().state().documents[&id].extensions["future"],
        serde_json::json!({"bytes":[8,4,1]})
    );
    let ppm = directory.path().join("color.ppm");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
        .args(["render"])
        .arg(&path)
        .arg(&ppm)
        .args(["16", "16", "0", "1"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(ppm).unwrap();
    let rgb: Vec<_> = delivery
        .rgba()
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    assert_eq!(&bytes[b"P6\n16 16\n255\n".len()..], rgb);
    // The actual preview worker uses the same transform, and its config-driven
    // controls arrive asynchronously rather than loading OCIO in the UI.
    use fold_platform::desktop::DesktopClient;
    let mut session = fold_app::session::Session::new(reopened);
    let reference = fold_project::DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let key = session
        .preview_state(&reference, Time::ZERO)
        .preview_key(0, 1)
        .unwrap();
    session.request_preview(key);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        session.poll();
        if let Some(result) = session.take_preview() {
            assert_eq!(result.frame.unwrap().rgba()[..4], preview.rgba()[..4]);
            assert_eq!(
                session.state().color_choices.inputs.first().unwrap(),
                fold_platform::color::VIDEO_INPUT
            );
            assert_eq!(
                session
                    .state()
                    .color_choices
                    .outputs
                    .first()
                    .unwrap()
                    .display,
                "Rec.1886 Rec.709 - Display"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "preview worker timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Opening/saving a missing-contract project never changes its interpretation.
    let mut legacy = Project::new(4);
    legacy
        .commit(EditBatch {
            base: legacy.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::Solid { rgb: [0., 0.5, 1.] }
                    .document(id)
                    .unwrap(),
            )],
        })
        .unwrap();
    let legacy_path = directory.path().join("legacy.fold");
    fold_project::save(&legacy.snapshot(), &legacy_path).unwrap();
    let legacy = fold_project::load(&legacy_path, 4).unwrap();
    assert_eq!(
        color::preview(&legacy.snapshot(), &scene(&legacy.snapshot(), id))
            .unwrap()
            .rgba()[..4],
        [0, 188, 255, 255]
    );
    assert!(color::external(&legacy.snapshot(), &path).is_err());
}

#[test]
#[ignore = "requires packaged OCIO runtime and FFmpeg; set FOLD_COLOR_ROOT"]
fn native_signal_input_overrides_read_nodes_export_and_external_configs() {
    use fold_compositor::{Composite, Node, Parameters as P, Source};
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("source.mp4");
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:size=16x16:rate=24",
            "-frames:v",
            "2",
            "-c:v",
            "libx264",
            "-threads",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_primaries",
            "bt709",
        ])
        .arg(&video)
        .status()
        .unwrap();
    assert!(result.success());
    let cancel = fold_media::Cancel::default();
    let source = fold_media::inspect(&video, &cancel).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let signal = decoder
        .decode_signal(&source, Time::ZERO, [16, 16], &cancel)
        .unwrap();
    let legacy = decoder
        .decode(&source, Time::ZERO, [16, 16], &cancel)
        .unwrap();
    assert!(signal.is_signal());
    assert!(!legacy.is_signal());
    assert_eq!(signal.storage_bytes(), 16 * 16 * 16);
    assert!(
        (signal.encoded_pixels().next().unwrap()[0] - legacy.encoded_pixels().next().unwrap()[0])
            .abs()
            > 0.01,
        "native signal must not be sRGB-converted before assignment"
    );
    let mut project = color::new_project(16);
    let asset = fold_foundation::AssetId::new();
    let read = Node::new(
        P::Read {
            source: Source::Asset {
                asset,
                info: source.info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: source.info.time(2).unwrap(),
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![read.id]);
    let mut composite = Composite {
        info: source.info.clone(),
        output: output.id,
        nodes: vec![read, output],
        extensions: Default::default(),
    };
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: video.to_str().unwrap().into(),
                    fingerprint: source.fingerprint,
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(composite.document(id).unwrap()),
            ],
        })
        .unwrap();
    let first = scene(&project.snapshot(), id);
    let key = workflow::content_for(&project.snapshot(), id).unwrap();
    composite.nodes[0].extensions.insert(
        fold_platform::color::INPUT_KEY.into(),
        serde_json::json!("ACEScg"),
    );
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(composite.document(id).unwrap())],
        })
        .unwrap();
    let assigned = scene(&project.snapshot(), id);
    assert_ne!(first.pixels(), assigned.pixels());
    assert_ne!(key, workflow::content_for(&project.snapshot(), id).unwrap());
    project.undo().unwrap();
    assert_eq!(scene(&project.snapshot(), id).pixels(), first.pixels());
    let export = directory.path().join("output.mp4");
    workflow::export(&project.snapshot(), &export, 0, 2, &cancel).unwrap();
    let exported = fold_media::inspect(&export, &cancel).unwrap();
    let decoded = decoder
        .decode_signal(&exported, Time::ZERO, [16, 16], &cancel)
        .unwrap();
    let expected = color::delivery(&project.snapshot(), id, &first).unwrap();
    for c in 0..3 {
        assert!(
            (decoded.encoded_pixels().next().unwrap()[c] - expected.rgba()[c] as f32 / 255.).abs()
                < 0.025,
            "no double transfer during encoding"
        );
    }
    let config_dir = directory.path().join("external");
    std::fs::create_dir(&config_dir).unwrap();
    let config_path = config_dir.join("config.ocio");
    std::fs::write(
        &config_path,
        include_bytes!("../../../resources/color/config.ocio"),
    )
    .unwrap();
    project
        .commit(color::external(&project.snapshot(), &config_path).unwrap())
        .unwrap();
    assert_eq!(scene(&project.snapshot(), id).pixels(), first.pixels());
    let pinned = project.snapshot();
    std::fs::write(&config_path, "broken config").unwrap();
    assert_eq!(
        scene(&pinned, id).pixels(),
        first.pixels(),
        "running evaluation retains its immutable config resources"
    );
    assert!(
        std::thread::spawn(move || color::with_config(&pinned, |_| Ok(())))
            .join()
            .unwrap()
            .is_err(),
        "a new worker cannot silently substitute changed resources"
    );
}
