use crate::automation;
use fold_project::Project;
use serde_json::json;
const SOURCE: &str = include_str!("../../../../demos/badge-live.js");
fn apply(project: &mut Project, source: &str) -> serde_json::Value {
    let snapshot = project.snapshot();
    let bindings = automation::handles::load(&snapshot, "badge-demo").unwrap();
    let batch=automation::code::compile(&json!({"namespace":"badge-demo","bindings":bindings,"source":source,"request_id":"test","label":"SDK tools"}),snapshot.revision(),0,&Default::default()).unwrap();
    let (edit, result) = automation::stage(&snapshot, &batch, &Default::default()).unwrap();
    project.commit_named(edit, "SDK tools".into()).unwrap();
    result
}
#[test]
fn badge_sdk_preserves_artist_edits_and_handles_across_save_reopen() {
    let mut project = Project::new(16);
    apply(&mut project, SOURCE);
    let handles = automation::handles::load(&project.snapshot(), "badge-demo").unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 3);
    let doc = serde_json::from_value(handles["motion"]["id"].clone()).unwrap();
    let text = serde_json::from_value(handles["motion/badge/text"]["id"].clone()).unwrap();
    let motion =
        fold_motion::Motion::from_document(&project.snapshot().state().documents[&doc]).unwrap();
    let count = motion.graph.nodes.len();
    let original = motion.clone();
    apply(
        &mut project,
        "const d=plan.document('motion');d.badge('badge').text.set('text','Artist text'); d.badge('badge').role('transform',{translation:{Value:datum([620,290])}}).properties({name:'Renamed badge'});",
    );
    apply(&mut project, SOURCE);
    let root = project.snapshot();
    let m = fold_motion::Motion::from_document(&root.state().documents[&doc]).unwrap();
    assert_eq!(m.graph.nodes.len(), count);
    assert_eq!(
        m.graph.node(text).unwrap().inputs["text"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Text("Artist text".into()))
    );
    let badge = serde_json::from_value(handles["motion/badge"]["id"].clone()).unwrap();
    let object = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == badge)
        .unwrap();
    assert_eq!(object.name, "Renamed badge");
    assert_eq!(
        m.graph.node(object.transform.unwrap()).unwrap().inputs["translation"],
        fold_motion::graph::Input::Value(fold_motion::fields::Datum::Vector([620., 290.]))
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("badge.fold");
    fold_project::save(&root, &path).unwrap();
    let mut reopened = fold_project::load(&path, 16).unwrap();
    assert_eq!(
        automation::handles::load(&reopened.snapshot(), "badge-demo").unwrap(),
        handles
    );
    apply(
        &mut reopened,
        "const d=plan.document('motion'); const c=d.duplicate('copies',d.badge('badge')); d.oscillator('bounce',c,'offset_y').set('minimum',-20).set('maximum',20);",
    );
    let changed =
        fold_motion::Motion::from_document(&reopened.snapshot().state().documents[&doc]).unwrap();
    assert_eq!(
        changed.graph.node(text).unwrap().inputs["text"],
        m.graph.node(text).unwrap().inputs["text"]
    );
    reopened.undo().unwrap();
    assert_eq!(
        reopened.snapshot().state().documents,
        root.state().documents
    );
    let t = fold_foundation::Time::new(3, 4).unwrap();
    let mut e = fold_motion::evaluation::Evaluator::new(&original, t).unwrap();
    assert!(e.scene().is_ok());
}

#[test]
fn saved_controls_keep_identity_and_deleted_handles_fail_without_recreation() {
    let mut project = Project::new(16);
    let source = "const d=plan.document('motion');d.control('speed','Speed',1);d.node('spare','fold.motion.math');";
    apply(&mut project, source);
    let bindings = automation::handles::load(&project.snapshot(), "badge-demo").unwrap();
    apply(&mut project, source);
    assert_eq!(
        automation::handles::load(&project.snapshot(), "badge-demo").unwrap(),
        bindings
    );
    let root = project.snapshot();
    let document = serde_json::from_value(bindings["motion"]["id"].clone()).unwrap();
    let (deleted, _) = crate::packages::builtins()
        .author_cancellable(
            &root.state().documents[&document],
            &[json!({"op":"remove","node":bindings["motion/spare"]["id"]})],
            &Default::default(),
        )
        .unwrap();
    project
        .commit(fold_project::EditBatch {
            base: root.revision(),
            mutations: vec![fold_project::Mutation::PutDocument(deleted)],
        })
        .unwrap();
    let before = project.snapshot();
    let error = automation::handles::load(&before, "badge-demo").unwrap_err();
    assert!(error.contains("deleted object"), "{error}");
    assert_eq!(project.snapshot().state(), before.state());
    project.undo().unwrap();
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::RemoveDocument(document)],
        })
        .unwrap();
    assert!(
        automation::handles::load(&project.snapshot(), "badge-demo")
            .unwrap_err()
            .contains("deleted document")
    );
    let keys = bindings
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let repaired = automation::handles::forget(&project.snapshot(), "badge-demo", &keys).unwrap();
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::SetSettings(repaired)],
        })
        .unwrap();
    assert_eq!(
        automation::handles::load(&project.snapshot(), "badge-demo").unwrap(),
        json!({})
    );
}
