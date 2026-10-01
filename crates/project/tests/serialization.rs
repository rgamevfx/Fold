use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_project::{EditBatch, Metadata, Mutation, Project, ProjectError, Revision, load, save};
use serde_json::json;

#[test]
fn stable_ids_and_time_survive_serialization_and_time_is_validated() {
    let ids = (DocumentId::new(), AssetId::new(), ObjectId::new());
    let encoded = serde_json::to_vec(&ids).unwrap();
    assert_eq!(
        serde_json::from_slice::<(DocumentId, AssetId, ObjectId)>(&encoded).unwrap(),
        ids
    );
    assert_ne!(ids.0, DocumentId::new());
    let time = Time::new(1001, 24000).unwrap();
    assert_eq!(
        serde_json::from_value::<Time>(serde_json::to_value(time).unwrap()).unwrap(),
        time
    );
    assert_eq!(
        serde_json::from_value::<Time>(json!({"numerator": 2, "denominator": 4})).unwrap(),
        Time::new(1, 2).unwrap()
    );
    assert!(serde_json::from_value::<Time>(json!({"numerator": 1, "denominator": 0})).is_err());
    assert!(serde_json::from_value::<Time>(json!({"numerator": 1, "denominator": -1})).is_err());
}

#[test]
fn exhausted_revision_rejects_edits_without_wrapping() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("exhausted.fold");
    save(&Project::new(5).snapshot(), &path).unwrap();
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["project"]["revision"] = json!(u64::MAX - 1);
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    let mut project = load(&path, 5).unwrap();
    project
        .commit(EditBatch {
            base: Revision(u64::MAX - 1),
            mutations: vec![Mutation::SetSettings(Metadata::from([(
                "test".into(),
                json!(true),
            )]))],
        })
        .unwrap();
    let before = project.snapshot();
    assert_eq!(project.undo().unwrap_err(), ProjectError::RevisionOverflow);
    assert_eq!(
        project
            .commit(EditBatch {
                base: Revision(u64::MAX),
                mutations: vec![Mutation::SetSettings(Metadata::new())],
            })
            .unwrap_err(),
        ProjectError::RevisionOverflow
    );
    assert_eq!(project.snapshot().state(), before.state());
}
