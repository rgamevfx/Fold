use fold_foundation::{AssetId, BinId, DocumentId};
use fold_project::*;
use serde_json::json;

fn apply(
    project: &mut Project,
    mutations: Vec<Mutation>,
) -> Result<CommittedSnapshot, ProjectError> {
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations,
    })
}
fn bin(name: &str, parent: Parent) -> Bin {
    Bin {
        id: BinId::new(),
        name: name.into(),
        parent,
        order: 7,
        extensions: Metadata::new(),
    }
}

#[test]
fn organization_is_atomic_undoable_and_does_not_rewrite_targets() {
    let mut p = Project::new(20);
    let asset = Asset {
        id: AssetId::new(),
        location: "/missing/original.wav".into(),
        fingerprint: "original".into(),
        extensions: Metadata::new(),
    };
    apply(&mut p, vec![Mutation::PutAsset(asset.clone())]).unwrap();
    let original = p.snapshot();
    let a = bin("Shots", Parent::Root);
    let b = bin("Shots", Parent::Bin(a.id));
    apply(
        &mut p,
        vec![Mutation::PutBin(a.clone()), Mutation::PutBin(b.clone())],
    )
    .unwrap();
    let mut moved = b.clone();
    moved.parent = Parent::Root;
    apply(&mut p, vec![Mutation::PutBin(moved.clone())]).unwrap();
    assert_eq!(p.snapshot().state().organization.bins[&b.id], moved);
    p.undo().unwrap();
    assert_eq!(p.snapshot().state().organization.bins[&b.id], b);
    let mut item = p.snapshot().state().organization.items[0].clone();
    item.parent = Parent::Bin(b.id);
    item.name = "Renamed".into();
    item.order = 42;
    apply(&mut p, vec![Mutation::PutItem(item.clone())]).unwrap();
    assert_eq!(p.snapshot().state().assets, original.state().assets);
    let before = p.snapshot();
    assert_eq!(
        apply(&mut p, vec![Mutation::RemoveBin(b.id)]).unwrap_err(),
        ProjectError::NonemptyBin
    );
    let mut cycle = a.clone();
    cycle.parent = Parent::Bin(b.id);
    assert_eq!(
        apply(&mut p, vec![Mutation::PutBin(cycle)]).unwrap_err(),
        ProjectError::Cycle
    );
    let mut dangling = item.clone();
    dangling.parent = Parent::Bin(BinId::new());
    assert!(apply(&mut p, vec![Mutation::PutItem(dangling)]).is_err());
    assert_eq!(p.snapshot().state(), before.state());
    item.parent = Parent::Root;
    let mut renamed = a.clone();
    renamed.name = "Finished".into();
    apply(
        &mut p,
        vec![
            Mutation::PutItem(item.clone()),
            Mutation::RemoveBin(b.id),
            Mutation::PutBin(renamed),
        ],
    )
    .unwrap();
    p.undo().unwrap();
    assert_eq!(
        p.snapshot().state().organization,
        before.state().organization
    );
    p.redo().unwrap();
    assert_eq!(p.snapshot().state().organization.items[0], item);
    apply(
        &mut p,
        vec![Mutation::RemoveBin(a.id), Mutation::RemoveAsset(asset.id)],
    )
    .unwrap();
    assert!(p.snapshot().state().organization.items.is_empty());
    p.undo().unwrap();
    assert_eq!(*p.snapshot().state().assets[&asset.id], asset);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("organized.fold");
    save(&p.snapshot(), &path).unwrap();
    assert_eq!(
        load(path, 10).unwrap().snapshot().state(),
        p.snapshot().state()
    );
}

#[test]
fn legacy_migration_is_deterministic_and_preserves_unknown_bytes_and_resources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.fold");
    let mut p = Project::new(5);
    let asset = Asset {
        id: AssetId::new(),
        location: "Lost/Plate.mp4".into(),
        fingerprint: "hash".into(),
        extensions: Metadata::from([("vendor".into(), json!({"raw": [4, 5]}))]),
    };
    let document = Document {
        id: DocumentId::new(),
        type_id: "unknown".into(),
        package_id: "absent".into(),
        schema_version: 99,
        revision: Revision(0),
        dependencies: vec![],
        assets: vec![asset.id],
        payload: vec![0, 255, 10],
        extensions: Metadata::new(),
    };
    apply(
        &mut p,
        vec![Mutation::PutAsset(asset), Mutation::PutDocument(document)],
    )
    .unwrap();
    save(&p.snapshot(), &path).unwrap();
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["version"] = json!(1);
    archive["project"]
        .as_object_mut()
        .unwrap()
        .remove("organization");
    archive["vendor_archive"] = json!({"keep": true});
    archive["project"]["resources"] = json!(["uninterpreted"]);
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    let a = load(&path, 5).unwrap();
    let b = load(&path, 5).unwrap();
    assert_eq!(
        a.snapshot().state().organization,
        b.snapshot().state().organization
    );
    assert_eq!(a.snapshot().state().assets, p.snapshot().state().assets);
    assert_eq!(
        a.snapshot().state().documents,
        p.snapshot().state().documents
    );
    save(&a.snapshot(), &path).unwrap();
    assert_eq!(
        load(&path, 5).unwrap().snapshot().state(),
        a.snapshot().state()
    );
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(archive["version"], 3);
    let item = archive["project"]["organization"]["items"][0].clone();
    archive["project"]["organization"]["items"]
        .as_array_mut()
        .unwrap()
        .push(item);
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert!(load(&path, 5).is_err());
}
