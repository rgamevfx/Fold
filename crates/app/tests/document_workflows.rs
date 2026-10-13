use fold_app::{browser, packages};
use fold_platform::browser::BrowserCommand;
use fold_platform::desktop::DesktopClient;
use fold_project::{Parent, Project};

#[test]
fn project_new_exposes_all_creative_documents() {
    let registry = packages::builtins();
    let kinds = registry.document_kinds();
    for (kind, title) in [
        (fold_timeline::SEQUENCE, "Timeline"),
        (fold_composition::COMPOSITION, "Composition"),
        (fold_motion_canvas::TYPE, "Motion"),
        (fold_three_d::TYPE, "3D"),
        (fold_compositor::COMPOSITE, "Compositing graph"),
    ] {
        assert!(
            kinds
                .iter()
                .any(|entry| entry.type_id == kind && entry.title == title),
            "missing {title} in Project > New"
        );
        let mut project = Project::new(8);
        let batch = browser::edit(
            &project.snapshot(),
            BrowserCommand::Create {
                kind: kind.into(),
                parent: Parent::Root,
            },
        )
        .unwrap();
        project.commit(batch).unwrap();
        assert_eq!(project.snapshot().state().documents.len(), 1);
        let session = fold_app::session::Session::new(Project::new(4));
        let snapshot = project.snapshot();
        let document = snapshot.state().documents.values().next().unwrap();
        assert_eq!(
            session.has_network(document),
            matches!(kind, fold_motion_canvas::TYPE | fold_compositor::COMPOSITE)
        );
        project.undo().unwrap();
        assert!(project.snapshot().state().documents.is_empty());
    }
}

#[test]
fn composition_adds_separate_motion_and_three_d_outputs_with_undo() {
    use fold_foundation::DocumentId;
    use fold_media::Cancel;
    use fold_platform::image_source::Source;
    use fold_project::{EditBatch, Mutation};
    let registry = packages::builtins();
    assert!(
        !registry
            .document_kinds()
            .iter()
            .any(|kind| kind.type_id == "fold.motion.construction")
    );
    let composition_id = DocumentId::new();
    let original = registry
        .create(fold_composition::COMPOSITION, composition_id)
        .unwrap();
    let info = fold_composition::Composition::from_document(&original)
        .unwrap()
        .info;
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(original.clone())],
        })
        .unwrap();
    let original = project.snapshot().state().documents[&composition_id]
        .as_ref()
        .clone();
    for (kind, type_id) in [
        ("motion", fold_motion_canvas::TYPE),
        ("3d", fold_three_d::TYPE),
    ] {
        let args = fold_app::composition_layers::AddArgs {
            document: composition_id,
            kind: kind.into(),
            layer: None,
        };
        let batch = fold_app::composition_layers::add(
            &project.snapshot(),
            &serde_json::to_vec(&args).unwrap(),
            &Cancel::default(),
        )
        .unwrap();
        project.commit(batch).unwrap();
        let snapshot = project.snapshot();
        let composition = fold_composition::Composition::from_document(
            &snapshot.state().documents[&composition_id],
        )
        .unwrap();
        let Source::Output(reference) = composition.layers[0].source.as_ref().unwrap() else {
            panic!("missing document output")
        };
        let document = &snapshot.state().documents[&reference.document];
        assert_eq!(document.type_id, type_id);
        assert_eq!(registry.output_ref(&snapshot, reference).unwrap(), info);
        assert!(registry.supports(document));
        project.undo().unwrap();
        assert_eq!(
            project.snapshot().state().documents[&composition_id].as_ref(),
            &original
        );
        assert_eq!(project.snapshot().state().documents.len(), 1);
    }
    let old = fold_app::composition_layers::AddArgs {
        document: composition_id,
        kind: "scene".into(),
        layer: None,
    };
    assert!(
        fold_app::composition_layers::add(
            &project.snapshot(),
            &serde_json::to_vec(&old).unwrap(),
            &Cancel::default()
        )
        .is_err()
    );
}

#[test]
fn native_motion_and_three_d_proposals_use_guarded_authoring_and_publication() {
    use fold_foundation::{DocumentId, ObjectId};
    use fold_media::Cancel;
    use fold_platform::proposals::{DocumentInspection, InspectionRequest, Scope, ScopedRequest};
    use fold_project::{EditBatch, Mutation};
    use serde_json::json;
    let registry = packages::builtins();
    let mut canvas = fold_motion_canvas::model::Canvas::default();
    let object = ObjectId::new();
    canvas.roots.push(object);
    canvas.objects.insert(
        object,
        fold_motion_canvas::tools::new_object("rectangle").unwrap(),
    );
    let expected = canvas.objects[&object].transform.x.clone();
    let mut value = expected.clone();
    value.value = 42.;
    let motion_edit = serde_json::to_value(fold_motion_canvas::editing::Edit::Parameter {
        object,
        expected,
        value,
    })
    .unwrap();
    let scene = fold_three_d::model::Scene::new(fold_media::VideoInfo {
        width: 320,
        height: 180,
        rate: [24, 1],
        frames: 48,
    });
    let object = scene
        .objects
        .iter()
        .find(|(_, object)| {
            matches!(
                object.content,
                fold_three_d::model::Content::Instance { .. }
            )
        })
        .unwrap()
        .0;
    let expected = scene.objects[object].transform.position.clone();
    let mut value = expected.clone();
    value.value = serde_json::to_value(fold_graph::binding::Binding::Literal {
        value: fold_graph::types::Literal::Vector {
            value: vec![1., 0., 0.],
            unit: fold_graph::types::Unit::Meters,
            space: fold_graph::types::Space::World,
        },
    })
    .unwrap();
    let spatial_edit = serde_json::to_value(fold_three_d::editing::Edit::Parameter {
        object: *object,
        expected,
        value,
    })
    .unwrap();
    for (mut document, edit) in [
        (canvas.document(DocumentId::new()).unwrap(), motion_edit),
        (scene.document(DocumentId::new()).unwrap(), spatial_edit),
    ] {
        document
            .extensions
            .insert("vendor.keep".into(), json!({"opaque":true}));
        let id = document.id;
        let mut project = Project::new(8);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document)],
            })
            .unwrap();
        let before = project.snapshot();
        let document = &before.state().documents[&id];
        let scope = Scope { document: id };
        let provider = registry.scoped_proposals(document).unwrap();
        let DocumentInspection::Native(mut expected) = provider
            .inspect(document, scope, &Cancel::default())
            .unwrap()
        else {
            panic!("native records")
        };
        expected["outputs"] = json!(registry.document_outputs(document).len());
        expected["image_inputs"] = json!([]);
        let input = ScopedRequest {
            scope,
            version: 1,
            body: json!({"op":"author","document":id,"expected":expected,"edits":[edit]}),
        };
        let prepared = provider
            .prepare(&before, &input, &Cancel::default())
            .unwrap();
        assert_eq!(prepared.scope().objects.len(), 1);
        let inspection = prepared
            .inspect(&InspectionRequest::all(scope), &Cancel::default())
            .unwrap();
        assert!(!inspection.changes.is_empty());
        let changes = prepared.publication(&before, &Cancel::default()).unwrap();
        assert_eq!(changes.documents[0].extensions, document.extensions);
        let mut protected = changes.documents[0].clone();
        let mut payload: serde_json::Value = serde_json::from_slice(&protected.payload).unwrap();
        let changed = prepared.scope().objects[0].to_string();
        payload["objects"][&changed]["protected"] = json!(true);
        protected.payload = serde_json::to_vec(&payload).unwrap();
        assert!(
            provider
                .prepare_change(&before, scope, &protected, &Cancel::default())
                .is_err()
        );
        project
            .commit(EditBatch {
                base: changes.base,
                mutations: changes
                    .documents
                    .into_iter()
                    .map(Mutation::PutDocument)
                    .collect(),
            })
            .unwrap();
        assert!(
            prepared
                .publication(&project.snapshot(), &Cancel::default())
                .is_err()
        );
        assert!(
            prepared
                .reconcile(&project.snapshot(), &Cancel::default())
                .is_ok()
        );
        project.undo().unwrap();
        assert_eq!(
            project.snapshot().state().documents[&id],
            before.state().documents[&id]
        );
        let mut artist = document.as_ref().clone();
        let mut payload: serde_json::Value = serde_json::from_slice(&artist.payload).unwrap();
        payload["objects"][&changed]["name"] = json!("Artist edit");
        artist.payload = serde_json::to_vec(&payload).unwrap();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(artist)],
            })
            .unwrap();
        assert!(
            prepared
                .reconcile(&project.snapshot(), &Cancel::default())
                .is_err()
        );
        let mut stale = input;
        stale.body["expected"] = json!({"native":{}});
        assert!(
            provider
                .prepare(&before, &stale, &Cancel::default())
                .is_err()
        );
    }
}
