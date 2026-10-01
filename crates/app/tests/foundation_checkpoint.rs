use fold_foundation::{DocumentId, Time};
use fold_motion::{Solid, SolidProvider};
use fold_platform::VideoRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project, Snapshot, load, save};

fn pixels(registry: &VideoRegistry, snapshot: &Snapshot, source: &DocumentRef) -> Vec<u8> {
    let plan = registry
        .compile(snapshot, source, Time::ZERO, 2, 1)
        .unwrap();
    let mut bytes = Vec::new();
    fold_render::render(plan)
        .unwrap()
        .write_ppm(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn edits_undo_persistence_and_provider_failures() {
    let mut registry = VideoRegistry::default();
    registry.register(SolidProvider).unwrap();
    assert!(registry.register(SolidProvider).is_err());
    let id = DocumentId::new();
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let mut project = Project::new(16);
    let put = |project: &mut Project, doc| {
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(doc)],
            })
            .unwrap()
    };
    let original = put(
        &mut project,
        Solid {
            rgb: [0.0, 0.5, 1.0],
        }
        .document(id)
        .unwrap(),
    );
    let expected = b"P6\n2 1\n255\n\x00\xbc\xff\x00\xbc\xff";
    assert_eq!(pixels(&registry, &original, &source), expected);
    let edited = put(
        &mut project,
        Solid {
            rgb: [1.0, 0.0, 0.0],
        }
        .document(id)
        .unwrap(),
    );
    assert_ne!(pixels(&registry, &edited, &source), expected);
    assert_eq!(pixels(&registry, &original, &source), expected);
    project.undo().unwrap();
    assert_eq!(pixels(&registry, &project.snapshot(), &source), expected);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.fold");
    save(&project.snapshot(), &path).unwrap();
    let reopened = load(&path, 16).unwrap();
    assert_eq!(pixels(&registry, &reopened.snapshot(), &source), expected);
    let bad_port = DocumentRef {
        output: "audio".into(),
        ..source.clone()
    };
    assert!(
        registry
            .compile(&original, &bad_port, Time::ZERO, 1, 1)
            .is_err()
    );
    assert!(
        VideoRegistry::default()
            .compile(&original, &source, Time::ZERO, 1, 1)
            .unwrap_err()
            .contains("missing video provider")
    );
    for kind in 0..3 {
        let mut doc = Solid { rgb: [0.0; 3] }.document(id).unwrap();
        match kind {
            0 => doc.schema_version = 99,
            1 => doc.payload = b"invalid".to_vec(),
            _ => doc.payload = br#"{"rgb":[2,0,0]}"#.to_vec(),
        }
        let snapshot = put(&mut project, doc);
        assert!(
            registry
                .compile(&snapshot, &source, Time::ZERO, 1, 1)
                .is_err()
        );
    }
}

#[test]
fn headless_cli_checkpoint_and_rerender() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("demo.fold");
    let image = dir.path().join("demo.ppm");
    let again = dir.path().join("again.ppm");
    let invoke = |command: &str, output: &std::path::Path| {
        std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .arg(command)
            .arg(&project)
            .arg(output)
            .output()
            .unwrap()
    };
    let result = invoke("checkpoint", &image);
    assert!(result.status.success(), "{:?}", result);
    assert!(invoke("render", &again).status.success());
    let bytes = std::fs::read(&image).unwrap();
    assert_eq!(bytes, std::fs::read(&again).unwrap());
    let header = b"P6\n64 64\n255\n";
    assert!(bytes.starts_with(header));
    assert_eq!(bytes.len(), header.len() + 64 * 64 * 3);
    assert!(
        bytes[header.len()..]
            .chunks_exact(3)
            .all(|p| p == [0, 188, 255])
    );
    assert!(!invoke("render", &image).status.success());
    assert_eq!(bytes, std::fs::read(&image).unwrap());
    assert!(!invoke("checkpoint", &again).status.success());
}
