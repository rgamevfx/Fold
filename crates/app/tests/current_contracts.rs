//! C1 contract retirement: generic storage preserves unsupported provider bytes.
use fold_foundation::DocumentId;
use fold_project::{Document, EditBatch, Mutation, Project};

#[test]
fn retired_providers_and_schemas_are_opaque_and_non_executable() {
    let packages = fold_app::packages::builtins();
    let mut project = Project::new(8);
    let mut documents = vec![];
    for (package, kind, versions) in [
        (
            fold_timeline::PACKAGE,
            "fold.timeline.video-layers",
            vec![1],
        ),
        (
            fold_timeline::PACKAGE,
            fold_timeline::SEQUENCE,
            vec![2, 3, 4],
        ),
        (
            fold_composition::PACKAGE,
            fold_composition::COMPOSITION,
            vec![1, 2],
        ),
    ] {
        for schema_version in versions {
            let document = Document {
                id: DocumentId::new(),
                package_id: package.into(),
                type_id: kind.into(),
                schema_version,
                revision: Default::default(),
                dependencies: vec![],
                assets: vec![],
                payload: vec![0, 255, 17],
                extensions: [("unknown".into(), serde_json::json!({"keep":true}))].into(),
            };
            assert!(!packages.supports(&document));
            documents.push(document);
        }
    }
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: documents
                .iter()
                .cloned()
                .map(Mutation::PutDocument)
                .collect(),
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opaque.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(reopened.snapshot().state(), project.snapshot().state());
    for document in documents {
        assert!(packages.output(&reopened.snapshot(), document.id).is_err());
    }
    assert!(fold_app::media_workflow::output(&reopened.snapshot()).is_err());
}

#[test]
fn compositor_catalog_exposes_only_current_operation_generations() {
    for name in ["Grade", "Merge", "Transform", "Blur"] {
        let versions: Vec<_> = fold_compositor::OPERATORS
            .iter()
            .filter(|op| op.id == name)
            .map(|op| op.version)
            .collect();
        assert_eq!(versions, [2]);
    }
}

#[test]
fn retired_operation_records_roundtrip_without_execution() {
    let info = fold_media::VideoInfo {
        width: 16,
        height: 16,
        rate: [24, 1],
        frames: 1,
    };
    for name in ["Grade", "Merge", "Transform", "Blur"] {
        let operator = fold_compositor::OPERATORS
            .iter()
            .find(|op| op.id == name)
            .unwrap();
        let mut node = fold_compositor::Node::disconnected(operator.defaults(&info).unwrap());
        node.operation.version = 1;
        assert!(node.try_configuration().is_err());
        let encoded = serde_json::to_vec(&node).unwrap();
        let decoded: fold_compositor::Node = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
        assert!(decoded.try_configuration().is_err());
    }
}

#[test]
fn shipped_fixtures_use_current_contracts_and_save_reopen() {
    let packages = fold_app::packages::builtins();
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "Make-it-move",
        "m5a-mixed-scene",
        "m5b-background-spheres",
        "m5b-wrangle",
        "m5b-integrated",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../demos/{name}.fold"));
        let project = fold_project::load(path, 8).unwrap();
        for document in project.snapshot().state().documents.values() {
            assert!(
                packages.supports(document),
                "{name}: {} schema {}",
                document.type_id,
                document.schema_version
            );
            packages.validate_document(document).unwrap();
            packages.output(&project.snapshot(), document.id).unwrap();
        }
        let saved = directory.path().join(format!("{name}.fold"));
        fold_project::save(&project.snapshot(), &saved).unwrap();
        assert_eq!(
            fold_project::load(saved, 8).unwrap().snapshot().state(),
            project.snapshot().state()
        );
    }
}
