use fold_foundation::{DocumentId, Time};
use fold_platform::VideoRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project, load, save};
use fold_render::{ImageOp, render};
use fold_timeline::{ImageSequenceProvider, import_sequence};

fn ppm(rgb: [u8; 3]) -> Vec<u8> {
    let mut bytes = b"P6\n1 1\n255\n".to_vec();
    bytes.extend_from_slice(&rgb);
    bytes
}

#[test]
fn import_undo_persist_seek_and_shared_composition() {
    let id = DocumentId::new();
    let frames = vec![ppm([128, 0, 255]), ppm([0, 255, 0])];
    let document = import_sequence(id, [24000, 1001], &frames).unwrap();
    let mut project = Project::new(4);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let pinned = project.snapshot();
    project.undo().unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.redo().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("media.fold");
    save(&project.snapshot(), &path).unwrap();
    let reopened = load(&path, 4).unwrap();
    assert_eq!(
        pinned.state().documents[&id].payload,
        reopened.snapshot().state().documents[&id].payload
    );
    let mut registry = VideoRegistry::default();
    registry.register(ImageSequenceProvider).unwrap();
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let frame_duration = Time::new(1001, 24000).unwrap();
    for index in [1, 0, 1, 0] {
        let graph = registry
            .compile(
                &pinned,
                &source,
                frame_duration.checked_scale(index, 1).unwrap(),
                1,
                1,
            )
            .unwrap();
        let frame = render(graph).unwrap();
        let mut exported = Vec::new();
        frame.write_ppm(&mut exported).unwrap();
        assert_eq!(exported, frames[index as usize]);
        assert_eq!(
            &frame.to_display().unwrap().rgba()[..3],
            &frames[index as usize][11..]
        );
    }
    for time in [
        Time::new(-1, 24000).unwrap(),
        frame_duration.checked_scale(2, 1).unwrap(),
    ] {
        assert!(registry.compile(&pinned, &source, time, 1, 1).is_err());
    }
    let before_boundary = frame_duration
        .checked_sub(Time::new(1, 48000).unwrap())
        .unwrap();
    let mut first = registry
        .compile(&pinned, &source, before_boundary, 1, 1)
        .unwrap();
    let second = registry
        .compile(&pinned, &source, frame_duration, 1, 1)
        .unwrap();
    first.nodes.extend(second.nodes);
    first.nodes.push(ImageOp::Opacity {
        input: 1,
        opacity: 0.5,
    });
    first.nodes.push(ImageOp::Over {
        foreground: 2,
        background: 0,
    });
    first.output = 3;
    let mixed = render(first).unwrap();
    assert_eq!(mixed.pixels()[0][1..], [0.5, 0.5, 1.0]);
    assert!((mixed.pixels()[0][0] - 0.10793026).abs() < 1e-6);
    assert!(
        registry
            .compile(&pinned, &source, Time::ZERO, 2, 1)
            .is_err()
    );
}

#[test]
fn invalid_imports_and_payloads_fail_explicitly() {
    use fold_platform::VideoProvider;
    let id = DocumentId::new();
    assert!(import_sequence(id, [0, 1], &[ppm([0; 3])]).is_err());
    assert!(import_sequence(id, [24, 1], &[]).is_err());
    assert!(import_sequence(id, [24, 1], &[b"invalid".to_vec()]).is_err());
    let original = import_sequence(id, [24, 1], &[ppm([0; 3])]).unwrap();
    let compile = |document: &fold_project::Document| {
        ImageSequenceProvider.compile(fold_platform::VideoCompile {
            snapshot: &fold_project::Project::new(0).snapshot(),
            document,
            reference: &fold_project::DocumentRef {
                document: id,
                output: "video".into(),
                extensions: Default::default(),
            },
            time: Time::ZERO,
            dimensions: [1, 1],
            cancel: &Default::default(),
            resolve: &|_, _| Err("unexpected nested source".into()),
        })
    };
    for length in 0..original.payload.len() {
        let mut document = original.clone();
        document.payload.truncate(length);
        assert!(compile(&document).is_err());
    }
    let mut document = original;
    document.payload.push(0);
    assert!(compile(&document).is_err());
}

#[test]
fn input_output_transfer_roundtrips_every_rgb8_value() {
    let mut bytes = b"P6\n256 1\n255\n".to_vec();
    for value in 0..=255u8 {
        bytes.extend_from_slice(&[value; 3]);
    }
    let source = fold_media::RgbImage::decode_ppm(&bytes).unwrap();
    let graph = fold_render::RenderGraph {
        width: 256,
        height: 1,
        nodes: vec![ImageOp::Media(source)],
        output: 0,
    };
    let frame = render(graph.clone()).unwrap();
    let mut exported = Vec::new();
    frame.write_ppm(&mut exported).unwrap();
    assert_eq!(exported, bytes);
    let mut mismatched = graph;
    mismatched.width = 255;
    assert!(render(mismatched).is_err());
}

#[test]
#[ignore = "requires packaged OCIO runtime; set FOLD_COLOR_ROOT"]
fn cli_import_and_exact_time_export() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.ppm");
    let second = dir.path().join("second.ppm");
    let project = dir.path().join("project.fold");
    let output = dir.path().join("out.ppm");
    std::fs::write(&first, ppm([255, 0, 0])).unwrap();
    std::fs::write(&second, ppm([0, 255, 0])).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
        .arg("import-sequence")
        .arg(&project)
        .args(["24000", "1001"])
        .arg(first)
        .arg(second)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
        .arg("render")
        .arg(project)
        .arg(&output)
        .args(["1", "1", "1001", "24000"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(output).unwrap(), ppm([93, 211, 54]));
}
