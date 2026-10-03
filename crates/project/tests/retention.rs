use fold_foundation::DocumentId;
use fold_project::{Document, EditBatch, Item, ItemId, Mutation, Parent, Project, Revision};

#[test]
fn history_payload_measurement_deduplicates_shared_documents_and_counts_snapshot_handles() {
    let mut project = Project::new(4);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(Document {
                id,
                package_id: "test".into(),
                type_id: "test".into(),
                schema_version: 1,
                revision: Revision(0),
                dependencies: vec![],
                assets: vec![],
                payload: vec![0; 4096],
                extensions: Default::default(),
            })],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let before = project.retention();
    assert_eq!(before.external_handles, 1);
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutItem(Item {
                id: ItemId::Document(id),
                name: "Renamed".into(),
                parent: Parent::Root,
                order: 0,
                extensions: Default::default(),
            })],
        })
        .unwrap();
    let after = project.retention();
    assert!(after.roots > before.roots);
    assert_eq!(after.payload_bytes, before.payload_bytes);
    drop(snapshot);
    assert_eq!(project.retention().external_handles, 0);
}
