use fold_media::Cancel;
use fold_platform::packages::*;
use fold_project::{EditBatch, Mutation, Project, Snapshot};
fn stage(snapshot: &Snapshot, _: &[u8], _: &Cancel) -> Result<EditBatch, String> {
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::SetSettings(
            [("registered-command".into(), true.into())].into(),
        )],
    })
}
fn package() -> Contributions {
    Contributions {
        manifest: Manifest {
            id: "test.tools",
            version: "1",
            host_api: HOST_API,
            dependencies: &[],
            panels: &[],
            build: "tests",
        },
        documents: vec![],
        commands: vec![CommandRegistration {
            id: "test.tools.mark",
            title: "Mark",
            execution: Execution::Immediate,
            handler: stage,
        }],
        video: vec![],
        audio: vec![],
    }
}
#[test]
fn static_registration_and_host_transactions_do_not_require_feature_switches() {
    let mut registry = PackageRegistry::default();
    registry.register(package()).unwrap();
    let mut project = Project::new(4);
    let snapshot = project.snapshot();
    let request = CommandRequest {
        id: "test.tools.mark".into(),
        arguments: vec![],
        base: snapshot.revision(),
    };
    let batch = registry
        .stage(&snapshot, &request, &Cancel::default())
        .unwrap();
    assert!(project.snapshot().state().settings.is_empty());
    project.commit(batch).unwrap();
    assert_eq!(
        project.snapshot().state().settings["registered-command"],
        true
    );
    assert!(
        registry
            .stage(&project.snapshot(), &request, &Cancel::default())
            .is_err()
    );
    assert!(project.undo().unwrap().state().settings.is_empty());
    assert_eq!(
        project.redo().unwrap().state().settings["registered-command"],
        true
    );
}
#[test]
fn document_registration_does_not_require_video_metadata() {
    struct Notes;
    impl DocumentProvider for Notes {
        fn package_id(&self) -> &'static str {
            "test.tools"
        }
        fn type_id(&self) -> &'static str {
            "test.tools.notes"
        }
        fn schema(&self) -> u32 {
            1
        }
        fn validate(&self, _: &fold_project::Document) -> Result<(), String> {
            Ok(())
        }
    }
    let mut contributions = package();
    contributions.documents.push(Box::new(Notes));
    let mut registry = PackageRegistry::default();
    registry.register(contributions).unwrap();
    let mut project = Project::new(0);
    let id = fold_foundation::DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(fold_project::Document {
                id,
                type_id: "test.tools.notes".into(),
                package_id: "test.tools".into(),
                schema_version: 1,
                revision: Default::default(),
                dependencies: vec![],
                assets: vec![],
                payload: vec![],
                extensions: Default::default(),
            })],
        })
        .unwrap();
    assert!(registry.supports(&project.snapshot().state().documents[&id]));
    assert!(
        registry
            .output(&project.snapshot(), id)
            .unwrap_err()
            .contains("no video capability")
    );
}

#[test]
fn invalid_contribution_sets_fail_atomically() {
    let mut registry = PackageRegistry::default();
    let mut invalid = package();
    invalid.manifest.host_api += 1;
    assert!(registry.register(invalid).is_err());
    let mut invalid = package();
    invalid.manifest.dependencies = &["missing.package"];
    assert!(registry.register(invalid).is_err());
    let mut invalid = package();
    invalid.commands.push(CommandRegistration {
        id: "test.tools.mark",
        title: "Duplicate",
        execution: Execution::Worker,
        handler: stage,
    });
    assert!(registry.register(invalid).is_err());
    let mut invalid = package();
    invalid.commands[0].id = "foreign.command";
    assert!(registry.register(invalid).is_err());
    assert_eq!(registry.manifests().count(), 0);
    assert!(registry.command("test.tools.mark").is_err());
    registry.register(package()).unwrap();
    assert!(registry.register(package()).is_err());
    assert_eq!(registry.manifests().count(), 1);
    let project = Project::new(4);
    let cancel = Cancel::default();
    cancel.cancel();
    let request = CommandRequest {
        id: "test.tools.mark".into(),
        arguments: vec![],
        base: project.snapshot().revision(),
    };
    assert!(
        registry
            .stage(&project.snapshot(), &request, &cancel)
            .is_err()
    );
}
