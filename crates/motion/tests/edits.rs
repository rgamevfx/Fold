use fold_foundation::{DocumentId, ObjectId};
use fold_motion::{Motion, authoring, edits::Changes, package};
use fold_platform::packages::{CommandRequest, PackageRegistry};
use fold_project::{EditBatch, Mutation, Project};
use serde_json::json;

fn request(project: &Project, id: DocumentId, before: &Motion, after: &Motion) -> CommandRequest {
    CommandRequest {
        id: package::CHANGE.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(&Changes::between(id, before, after).unwrap()).unwrap(),
    }
}
#[test]
fn sparse_edits_preserve_resources_and_unknown_data_and_reject_stale_or_invalid_changes() {
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let mut project = Project::new(8);
    let id = DocumentId::new();
    let mut before = Motion::empty();
    before
        .extensions
        .insert("unknown".into(), json!({"keep":[1,2],"remove":1}));
    let font_id = ObjectId::new();
    let mut font = fold_motion::document::Font::builtin();
    font.source = fold_motion::document::FontSource::Embedded(font.bytes().to_vec());
    before.fonts.insert(font_id, font);
    let mut document = before.document(id).unwrap();
    document
        .extensions
        .insert("opaque".into(), json!({"keep":true}));
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let mut after = before.clone();
    after.scene = Some(Default::default());
    authoring::drift::create(&mut after).unwrap();
    after
        .extensions
        .get_mut("unknown")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("remove");
    after.extensions.insert("added".into(), json!(true));
    let edit = request(&project, id, &before, &after);
    assert!(edit.arguments.len() < 32 * 1024);
    let batch = registry
        .stage(&project.snapshot(), &edit, &Default::default())
        .unwrap();
    project.commit(batch).unwrap();
    let saved = project.snapshot();
    assert_eq!(
        Motion::from_document(&saved.state().documents[&id]).unwrap(),
        after
    );
    assert_eq!(
        saved.state().documents[&id].extensions["opaque"],
        json!({"keep":true})
    );
    assert_eq!(
        registry
            .stage(&saved, &edit, &Default::default())
            .unwrap_err(),
        "stale command revision"
    );

    // Changes to a resource, extension removal and optional-field removal remain expressible.
    let mut next = after.clone();
    next.fonts.get_mut(&font_id).unwrap().name = "Renamed".into();
    next.scene = None;
    next.extensions.remove("added");
    let edit = request(&project, id, &after, &next);
    assert!(edit.arguments.len() < 1024);
    project
        .commit(registry.stage(&saved, &edit, &Default::default()).unwrap())
        .unwrap();
    assert_eq!(
        Motion::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        next
    );
    let original = project.snapshot();
    for change in [
        json!({"Set":{"path":["graph","nodes",999,"position",0],"value":12}}),
        json!({"Set":{"path":["info","width"],"value":0}}),
        json!({"Remove":{"path":["graph"]}}),
    ] {
        let bad = CommandRequest {
            id: package::CHANGE.into(),
            base: original.revision(),
            arguments: serde_json::to_vec(&json!({"document":id,"changes":[change]})).unwrap(),
        };
        assert!(
            registry
                .stage(&original, &bad, &Default::default())
                .is_err()
        );
        assert_eq!(
            project.snapshot().state().documents,
            original.state().documents
        );
    }
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        registry
            .stage(&original, &request(&project, id, &next, &before), &cancel)
            .is_err()
    );
}
