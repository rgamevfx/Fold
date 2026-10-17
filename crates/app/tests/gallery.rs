use fold_app::gallery::{self, storage::Store};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_motion_canvas::{
    model::{Canvas, Content},
    tools,
};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    gallery::{Kind, Place, Reference, Request},
    packages::CommandRequest,
};
use fold_project::{EditBatch, Mutation, Project};
fn insert(project: &mut Project, document: fold_project::Document) {
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
}
fn request(component: Reference, document: DocumentId) -> Place {
    Place {
        component,
        document,
        objects: vec![],
        at: Time::ZERO,
        position: None,
        track: None,
        update: None,
    }
}
fn find(store: &Store, name: &str) -> Reference {
    store
        .list()
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
        .reference
}
#[test]
fn save_import_place_twice_update_and_reopen_keep_independent_native_artwork() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store {
        root: directory.path().join("library"),
    };
    let imported = Store {
        root: directory.path().join("imported"),
    };
    let mut project = Project::new(10);
    let source = DocumentId::new();
    let target = DocumentId::new();
    let id = ObjectId::new();
    let mut text = tools::new_object("text").unwrap();
    text.extensions.insert(
        "future.vendor".into(),
        serde_json::json!({"nested":{"id":id,"unknown":true}}),
    );
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, text)].into(),
        ..Default::default()
    };
    insert(&mut project, canvas.document(source).unwrap());
    insert(&mut project, Canvas::default().document(target).unwrap());
    let save = |name: &str, update| Request::Save {
        document: source,
        objects: vec![id],
        name: name.into(),
        category: "Titles".into(),
        kind: Kind::Graphic,
        update,
    };
    let result = gallery::operate(
        &store,
        &project.snapshot(),
        save("My title", None),
        &Cancel::default(),
    )
    .unwrap();
    let first: Reference = serde_json::from_value(result["reference"].clone()).unwrap();
    let path = directory.path().join("title.foldcomponent");
    gallery::operate(
        &store,
        &project.snapshot(),
        Request::Export {
            component: first,
            path: path.to_string_lossy().into(),
        },
        &Cancel::default(),
    )
    .unwrap();
    gallery::operate(
        &imported,
        &project.snapshot(),
        Request::Import {
            path: path.to_string_lossy().into(),
        },
        &Cancel::default(),
    )
    .unwrap();
    for _ in 0..2 {
        let batch = gallery::place(
            &imported,
            &project.snapshot(),
            &request(first, target),
            &Cancel::default(),
        )
        .unwrap();
        project.commit(batch).unwrap();
    }
    let before = project.snapshot();
    let placed = Canvas::from_document(&before.state().documents[&target]).unwrap();
    assert_eq!(placed.roots.len(), 2);
    assert_eq!(placed.objects.len(), 4);
    let texts: Vec<_> = placed
        .objects
        .values()
        .filter(|o| matches!(o.content, Content::Text { .. }))
        .collect();
    assert_ne!(texts[0].transform.x.id, texts[1].transform.x.id);
    assert_eq!(
        texts[0].extensions["future.vendor"],
        canvas.objects[&id].extensions["future.vendor"]
    );
    let mut updated = canvas.clone();
    if let Content::Text { text, .. } = &mut updated.objects.get_mut(&id).unwrap().content {
        *text = "Second version".into();
    }
    insert(&mut project, updated.document(source).unwrap());
    let result = gallery::operate(
        &store,
        &project.snapshot(),
        save("My title", Some(first)),
        &Cancel::default(),
    )
    .unwrap();
    let second: Reference = serde_json::from_value(result["reference"].clone()).unwrap();
    assert_eq!(second.version, 2);
    assert_eq!(
        project.snapshot().state().documents[&target].payload,
        before.state().documents[&target].payload
    );
    let mut update = request(second, target);
    update.update = Some(placed.roots[0]);
    let batch = gallery::place(&store, &project.snapshot(), &update, &Cancel::default()).unwrap();
    project.commit(batch).unwrap();
    let after = Canvas::from_document(&project.snapshot().state().documents[&target]).unwrap();
    assert_eq!(after.roots, placed.roots);
    let other = placed.roots[1];
    assert_eq!(after.objects[&other], placed.objects[&other]);
    let file = directory.path().join("project.fold");
    fold_project::save(&project.snapshot(), &file).unwrap();
    let reopened = fold_project::load(&file, 10).unwrap();
    assert_eq!(
        reopened.snapshot().state().documents[&target].payload,
        project.snapshot().state().documents[&target].payload
    );
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents[&target].payload,
        before.state().documents[&target].payload
    );
    assert!(
        gallery::operate(
            &store,
            &project.snapshot(),
            save("Stale", Some(first)),
            &Cancel::default()
        )
        .is_err()
    );
}
#[test]
fn gallery_command_uses_real_session_worker_and_one_undo_for_definition_and_clip() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store {
        root: directory.path().into(),
    };
    for component in store.list().unwrap() {
        store.get(component.reference).unwrap().validate().unwrap();
    }
    let graphic = find(&store, "Title");
    let document = DocumentId::new();
    let mut project = Project::new(10);
    let sequence = fold_timeline::Sequence {
        dimensions: [1920, 1080],
        rate: [30, 1],
        tracks: vec![],
        clips: vec![],
        transitions: vec![],
        extensions: Default::default(),
    };
    insert(&mut project, sequence.document(document).unwrap());
    let mut session = fold_app::session::Session::new(project);
    let before = session.snapshot().unwrap();
    let command = CommandRequest {
        id: fold_platform::gallery::PLACE.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&request(graphic, document)).unwrap(),
    };
    session.command(DesktopCommand::Extension(command.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while session.extension_pending(&command) {
        session.poll();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        session.extension_error(&command).is_none(),
        "{:?}",
        session.extension_error(&command)
    );
    let after = session.snapshot().unwrap();
    assert_eq!(after.state().documents.len(), 2);
    assert_eq!(
        fold_timeline::Sequence::from_document(&after.state().documents[&document])
            .unwrap()
            .clips
            .len(),
        1
    );
    session.command(DesktopCommand::Undo);
    let undone = session.snapshot().unwrap();
    assert_eq!(undone.state().documents.len(), 1);
    assert_eq!(
        undone.state().documents[&document].payload,
        before.state().documents[&document].payload
    );
    session.command(DesktopCommand::Redo);
    assert_eq!(session.snapshot().unwrap().state().documents.len(), 2);
}
#[test]
fn composition_instances_use_existing_effect_controls() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store {
        root: directory.path().into(),
    };
    let mut project = Project::new(8);
    let motion = DocumentId::new();
    let id = ObjectId::new();
    let object = tools::rectangle("Box", [50., 50.], [300., 100.]);
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, object.clone())].into(),
        ..Default::default()
    };
    insert(&mut project, canvas.document(motion).unwrap());
    let composition = DocumentId::new();
    insert(
        &mut project,
        fold_composition::Composition::default()
            .document(composition)
            .unwrap(),
    );
    let batch = gallery::place(
        &store,
        &project.snapshot(),
        &request(find(&store, "Title"), composition),
        &Cancel::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
    let layer = fold_composition::Composition::from_document(
        &project.snapshot().state().documents[&composition],
    )
    .unwrap()
    .layers[0]
        .id;
    let mut args = request(find(&store, "Glow"), composition);
    args.objects = vec![layer];
    let batch = gallery::place(&store, &project.snapshot(), &args, &Cancel::default()).unwrap();
    project.commit(batch).unwrap();
    let c = fold_composition::Composition::from_document(
        &project.snapshot().state().documents[&composition],
    )
    .unwrap();
    assert_eq!(c.layers[0].effects.len(), 1);
    assert!(
        !fold_app::packages::builtins()
            .effect_interface(
                &project.snapshot(),
                c.layers[0].effects[0].definition.document
            )
            .unwrap()
            .controls
            .is_empty()
    );
}

#[test]
fn curated_presets_expose_independent_root_controls_and_portable_assets() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store {
        root: directory.path().into(),
    };
    let mut project = Project::new(32);
    let target = DocumentId::new();
    insert(&mut project, Canvas::default().document(target).unwrap());
    for (_, name) in fold_motion_canvas::presets::CATALOG {
        let reference = find(&store, name);
        let package = store.get(reference).unwrap();
        package.validate().unwrap();
        for _ in 0..2 {
            let batch = gallery::place(
                &store,
                &project.snapshot(),
                &request(reference, target),
                &Cancel::default(),
            )
            .unwrap();
            project.commit(batch).unwrap();
        }
        let snapshot = project.snapshot();
        let canvas = Canvas::from_document(&snapshot.state().documents[&target]).unwrap();
        let roots = &canvas.roots[canvas.roots.len() - 2..];
        let Content::Script { controls: a, .. } = &canvas.objects[&roots[0]].content else {
            panic!("preset must expose its controls on the root")
        };
        let Content::Script { controls: b, .. } = &canvas.objects[&roots[1]].content else {
            panic!()
        };
        assert!(a.keys().all(|id| !b.contains_key(id)));
        assert!(a.values().any(|c| c.name == "Speed"));
        let mut object = canvas.objects[&roots[0]].clone();
        let Content::Script { controls, .. } = &mut object.content else {
            unreachable!()
        };
        let value = controls
            .values_mut()
            .find_map(|c| {
                if c.name == "Speed" {
                    Some(&mut c.value)
                } else {
                    None
                }
            })
            .unwrap();
        let fold_motion_canvas::model::ControlValue::Number(value) = value else {
            panic!()
        };
        value.value = 2.;
        let changed = fold_motion_canvas::editing::apply(
            &canvas,
            &[fold_motion_canvas::editing::Edit::Replace {
                id: roots[0],
                expected: Box::new(canvas.objects[&roots[0]].clone()),
                object: Box::new(object),
            }],
        )
        .unwrap();
        assert_eq!(changed.objects[&roots[1]], canvas.objects[&roots[1]]);
        assert!(
            fold_motion_canvas::design::bounds(&canvas, Time::ZERO)
                .unwrap()
                .contains_key(&roots[0])
        );
    }
    assert_eq!(project.snapshot().state().assets.len(), 8);
    for asset in project.snapshot().state().assets.values() {
        let (image, _) = fold_media::still::load(
            std::path::Path::new(&asset.location),
            &asset.fingerprint,
            &Cancel::default(),
        )
        .unwrap();
        assert_eq!(image.dimensions(), [1200, 1200]);
    }
    let logo = store.get(find(&store, "Logo Assemble")).unwrap();
    let path = directory.path().join("logo.foldcomponent");
    gallery::operate(
        &store,
        &project.snapshot(),
        Request::Export {
            component: logo.reference,
            path: path.to_string_lossy().into(),
        },
        &Cancel::default(),
    )
    .unwrap();
    let reopened: gallery::storage::Component =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(reopened.resources.len(), 8);
    reopened.validate().unwrap();
}

#[test]
fn placed_source_control_uses_session_preview_commit_cancel_and_undo() {
    use fold_motion_canvas::editing::Edit;
    use fold_platform::desktop::Selection;
    let directory = tempfile::tempdir().unwrap();
    let store = Store {
        root: directory.path().into(),
    };
    let mut project = Project::new(8);
    let composition = DocumentId::new();
    insert(
        &mut project,
        fold_composition::Composition::default()
            .document(composition)
            .unwrap(),
    );
    let batch = gallery::place(
        &store,
        &project.snapshot(),
        &request(find(&store, "Soft Blur In"), composition),
        &Cancel::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
    let mut host = fold_app::session::Session::new(project);
    let sources = host.parameter_sources(composition).unwrap();
    let source = &sources[0];
    host.command(DesktopCommand::Select(Selection {
        document: Some(composition),
        objects: vec![source.owner],
    }));
    let before = host.snapshot().unwrap();
    let canvas =
        Canvas::from_document(&before.state().documents[&source.reference.document]).unwrap();
    let id = canvas.roots[0];
    let expected = canvas.objects[&id].clone();
    let mut object = expected.clone();
    let Content::Script { controls, .. } = &mut object.content else {
        panic!()
    };
    let value = controls.values_mut().find(|c| c.name == "Text").unwrap();
    value.value = fold_motion_canvas::model::ControlValue::Text("Edited from the container".into());
    let command = CommandRequest {
        id: fold_motion_context::package::EDIT.into(),
        base: before.revision(),
        arguments: serde_json::to_vec(&fold_motion_context::package::EditArgs {
            document: source.reference.document,
            edits: vec![Edit::Replace {
                id,
                expected: Box::new(expected),
                object: Box::new(object.clone()),
            }],
        })
        .unwrap(),
    };
    host.command(DesktopCommand::PreviewExtension(command.clone()));
    assert!(host.state().transient);
    assert_eq!(host.snapshot().unwrap().revision(), before.revision());
    host.command(DesktopCommand::CancelPreviewEdit);
    assert!(!host.state().transient);
    assert_eq!(
        host.snapshot().unwrap().state().documents,
        before.state().documents
    );
    host.command(DesktopCommand::PreviewExtension(command.clone()));
    host.command(DesktopCommand::Extension(command.clone()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while host.extension_pending(&command) {
        host.poll();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        host.extension_error(&command).is_none(),
        "{:?}",
        host.extension_error(&command)
    );
    let after = host.snapshot().unwrap();
    assert_eq!(
        Canvas::from_document(&after.state().documents[&source.reference.document])
            .unwrap()
            .objects[&id],
        object
    );
    assert_eq!(
        after.state().documents[&composition],
        before.state().documents[&composition]
    );
    host.command(DesktopCommand::Undo);
    assert_eq!(
        host.snapshot().unwrap().state().documents,
        before.state().documents
    );
    host.command(DesktopCommand::Redo);
    assert_eq!(
        host.snapshot().unwrap().state().documents,
        after.state().documents
    );
}
