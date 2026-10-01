use fold_foundation::{AssetId, DocumentId};
use fold_project::*;
use serde_json::json;
use std::sync::Arc;

fn document() -> Document {
    Document {
        id: DocumentId::new(),
        type_id: "unknown.document".into(),
        package_id: "unknown.package".into(),
        schema_version: 73,
        revision: Revision(0),
        dependencies: Vec::new(),
        assets: Vec::new(),
        payload: vec![0, 255, 128, 10],
        extensions: Metadata::new(),
    }
}
fn reference(id: DocumentId) -> DocumentRef {
    DocumentRef {
        document: id,
        output: "image".into(),
        extensions: Metadata::new(),
    }
}
fn commit(
    project: &mut Project,
    mutations: Vec<Mutation>,
) -> Result<CommittedSnapshot, ProjectError> {
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations,
    })
}

#[test]
fn cross_document_atomicity_snapshots_and_history() {
    let mut project = Project::new(10);
    let empty = project.snapshot();
    let first = document();
    let mut second = document();
    second.dependencies.push(reference(first.id));
    let created = commit(
        &mut project,
        vec![
            Mutation::PutDocument(second.clone()),
            Mutation::PutDocument(first.clone()),
        ],
    )
    .unwrap();
    assert!(empty.state().documents.is_empty());
    assert_eq!(created.state().documents.len(), 2);
    assert_eq!(
        created.state().documents[&first.id].revision,
        created.revision()
    );
    assert_eq!(
        commit(&mut project, vec![Mutation::RemoveDocument(first.id)]).unwrap_err(),
        ProjectError::MissingDocument(first.id)
    );
    assert_eq!(project.snapshot().revision(), created.revision());
    let mut changed = first.clone();
    changed.payload = vec![42];
    let edited = commit(&mut project, vec![Mutation::PutDocument(changed)]).unwrap();
    assert!(Arc::ptr_eq(
        &created.state().documents[&second.id],
        &edited.state().documents[&second.id]
    ));
    assert_eq!(created.state().documents[&first.id].payload, first.payload);
    let undone = project.undo().unwrap();
    assert_eq!(undone.state().documents, created.state().documents);
    assert!(undone.revision() > edited.revision());
    assert_eq!(
        project.redo().unwrap().state().documents,
        edited.state().documents
    );
    project.undo().unwrap();
    assert!(project.undo().unwrap().state().documents.is_empty());
    assert_eq!(project.undo().unwrap_err(), ProjectError::NoUndo);
    assert_eq!(project.redo().unwrap().state().documents.len(), 2);
    commit(
        &mut project,
        vec![
            Mutation::RemoveDocument(first.id),
            Mutation::RemoveDocument(second.id),
        ],
    )
    .unwrap();
    assert_eq!(project.redo().unwrap_err(), ProjectError::NoRedo);
}

#[test]
fn stale_cycles_and_missing_assets_publish_nothing() {
    let mut project = Project::new(10);
    let mut first = document();
    let mut second = document();
    first.dependencies.push(reference(second.id));
    second.dependencies.push(reference(first.id));
    assert_eq!(
        commit(
            &mut project,
            vec![
                Mutation::PutDocument(first.clone()),
                Mutation::PutDocument(second.clone())
            ]
        )
        .unwrap_err(),
        ProjectError::Cycle
    );
    assert_eq!(project.snapshot().revision(), Revision(0));
    assert_eq!(project.undo().unwrap_err(), ProjectError::NoUndo);
    first.dependencies.clear();
    first.assets.push(AssetId::new());
    assert_eq!(
        commit(&mut project, vec![Mutation::PutDocument(first.clone())]).unwrap_err(),
        ProjectError::MissingAsset(first.assets[0])
    );
    first.assets.clear();
    first.dependencies.push(reference(first.id));
    assert_eq!(
        commit(&mut project, vec![Mutation::PutDocument(first.clone())]).unwrap_err(),
        ProjectError::Cycle
    );
    first.dependencies.clear();
    commit(&mut project, vec![Mutation::PutDocument(first.clone())]).unwrap();
    project.undo().unwrap();
    assert!(matches!(
        project.commit(EditBatch {
            base: Revision(0),
            mutations: vec![Mutation::PutDocument(second)]
        }),
        Err(ProjectError::StaleRevision { .. })
    ));
    // Failed transactions do not invalidate redo.
    assert_eq!(project.redo().unwrap().state().documents.len(), 1);
    assert_eq!(
        commit(&mut project, vec![]).unwrap_err(),
        ProjectError::EmptyEdit
    );
}

#[test]
fn history_limit_and_disabled_history() {
    for limit in [0, 2] {
        let mut project = Project::new(limit);
        for _ in 0..3 {
            commit(&mut project, vec![Mutation::PutDocument(document())]).unwrap();
        }
        for _ in 0..limit {
            project.undo().unwrap();
        }
        assert_eq!(project.undo().unwrap_err(), ProjectError::NoUndo);
        assert_eq!(project.snapshot().state().documents.len(), 3 - limit);
        for _ in 0..limit {
            project.redo().unwrap();
        }
        assert_eq!(project.redo().unwrap_err(), ProjectError::NoRedo);
    }
}

#[test]
fn edit_preview_commits_once_or_cancels_and_stale_sessions_fail() {
    let mut project = Project::new(10);
    let mut first = document();
    commit(&mut project, vec![Mutation::PutDocument(first.clone())]).unwrap();
    let committed = project.snapshot();
    let mut session = project.begin_edit();
    let initial_preview = session.snapshot();
    for value in 0..10 {
        first.payload = vec![value];
        project
            .update_edit(&mut session, vec![Mutation::PutDocument(first.clone())])
            .unwrap();
    }
    assert_eq!(session.generation(), 10);
    assert_eq!(initial_preview.state(), committed.state());
    let rendered_preview = session.snapshot();
    assert_eq!(
        rendered_preview.state().documents[&first.id].payload,
        vec![9]
    );
    assert_eq!(session.state().documents[&first.id].payload, vec![9]);
    assert_eq!(project.snapshot().state(), committed.state());
    let generation = session.generation();
    assert!(
        project
            .update_edit(
                &mut session,
                vec![Mutation::RemoveDocument(DocumentId::new())]
            )
            .is_err()
    );
    assert_eq!(session.generation(), generation);
    project.commit_edit(session).unwrap();
    assert_eq!(
        rendered_preview.state().documents[&first.id].payload,
        vec![9]
    );
    assert_eq!(
        project.undo().unwrap().state().documents,
        committed.state().documents
    );
    let mut cancelled = project.begin_edit();
    project
        .update_edit(&mut cancelled, vec![Mutation::PutDocument(first.clone())])
        .unwrap();
    drop(cancelled);
    assert_eq!(
        project.snapshot().state().documents,
        committed.state().documents
    );
    let mut stale = project.begin_edit();
    project
        .update_edit(&mut stale, vec![Mutation::PutDocument(first)])
        .unwrap();
    project.redo().unwrap();
    assert!(matches!(
        project.commit_edit(stale),
        Err(ProjectError::StaleRevision { .. })
    ));
}

#[test]
fn unknown_data_and_all_manifest_records_survive_save_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.fold");
    let mut project = Project::new(5);
    let asset = Asset {
        id: AssetId::new(),
        location: "missing://footage".into(),
        fingerprint: "sha256:unavailable".into(),
        extensions: Metadata::from([("vendor".into(), json!({"arbitrary": [1, null, true]}))]),
    };
    let mut first = document();
    first.assets.push(asset.id);
    first
        .extensions
        .insert("future_field".into(), json!({"bytes": "not our schema"}));
    let mut second = document();
    second.dependencies.push(reference(first.id));
    second.dependencies[0]
        .extensions
        .insert("future_time_mapping".into(), json!([24000, 1001]));
    commit(
        &mut project,
        vec![
            Mutation::PutDocument(first.clone()),
            Mutation::PutDocument(second),
            Mutation::PutAsset(asset.clone()),
            Mutation::SetSettings(Metadata::from([("rate".into(), json!([24000, 1001]))])),
            Mutation::SetEnvironment(Metadata::from([(
                "unknown.package".into(),
                json!({"version": 73, "hash": "opaque"}),
            )])),
        ],
    )
    .unwrap();
    assert_eq!(
        commit(&mut project, vec![Mutation::RemoveAsset(asset.id)]).unwrap_err(),
        ProjectError::MissingAsset(asset.id)
    );
    save(&project.snapshot(), &path).unwrap();
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["future_archive_field"] = json!({"keep": [false, 1]});
    archive["project"]["future_manifest_field"] = json!("retain me");
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    let mut reopened = load(&path, 5).unwrap();
    assert_eq!(reopened.undo().unwrap_err(), ProjectError::NoUndo);
    assert_eq!(
        reopened.snapshot().state().documents,
        project.snapshot().state().documents
    );
    assert_eq!(
        reopened.snapshot().state().assets,
        project.snapshot().state().assets
    );
    save(&reopened.snapshot(), &path).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, archive);
    // Replacing an existing archive and pinning an older snapshot both work.
    let old = reopened.snapshot();
    commit(&mut reopened, vec![Mutation::SetSettings(Metadata::new())]).unwrap();
    save(&old, &path).unwrap();
    assert_eq!(load(&path, 5).unwrap().snapshot().state(), old.state());
    save(&reopened.snapshot(), &path).unwrap();
    assert_eq!(
        load(&path, 5).unwrap().snapshot().state(),
        reopened.snapshot().state()
    );
}

#[test]
fn extension_fields_cannot_shadow_authoritative_fields() {
    let mut project = Project::new(5);
    let mut first = document();
    first.extensions.insert("payload".into(), json!([99]));
    assert_eq!(
        commit(&mut project, vec![Mutation::PutDocument(first.clone())]).unwrap_err(),
        ProjectError::InvalidDocument(first.id)
    );
    first.extensions.clear();
    let mut second = document();
    second.dependencies.push(reference(first.id));
    second.dependencies[0]
        .extensions
        .insert("output".into(), json!("shadow"));
    assert_eq!(
        commit(
            &mut project,
            vec![
                Mutation::PutDocument(first),
                Mutation::PutDocument(second.clone())
            ]
        )
        .unwrap_err(),
        ProjectError::InvalidDocument(second.id)
    );
    let asset = Asset {
        id: AssetId::new(),
        location: "missing".into(),
        fingerprint: "hash".into(),
        extensions: Metadata::from([("id".into(), json!("shadow"))]),
    };
    assert_eq!(
        commit(&mut project, vec![Mutation::PutAsset(asset.clone())]).unwrap_err(),
        ProjectError::InvalidAsset(asset.id)
    );
    assert_eq!(project.snapshot().revision(), Revision(0));
}

#[test]
fn malformed_unsupported_and_invalid_archives_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.fold");
    save(&Project::new(5).snapshot(), &path).unwrap();
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["version"] = json!(999);
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(matches!(
        load(&path, 5),
        Err(LoadError::UnsupportedFormat { .. })
    ));
    archive["version"] = json!(1);
    let first = document();
    archive["project"]["documents"] = json!(std::collections::BTreeMap::from([(
        DocumentId::new(),
        first
    )]));
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(matches!(load(&path, 5), Err(LoadError::InvalidProject(_))));
    std::fs::write(&path, b"{truncated").unwrap();
    assert!(matches!(load(&path, 5), Err(LoadError::Json(_))));
}
