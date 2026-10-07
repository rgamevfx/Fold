use fold_app::automation::{self, Batch};
use fold_foundation::DocumentId;
use fold_project::Project;
use serde_json::json;

#[test]
fn public_creation_owns_native_layer_and_preserves_undo_and_saved_source() {
    let mut project = Project::new(8);
    let source = "let mut a=Asset::new(\"Orbit\"); let radius=a.scalar(\"radius\",\"Radius\",25.,\"px\"); let c=a.circle(\"dot\",radius); a.output(&c); a";
    let batch:Batch=serde_json::from_value(json!({"request_id":"rust-create","label":"Create Rust asset","revision":0,"project_epoch":0,"ids":["composition","graph","definition","instance","layer"],"operations":[
        {"op":"create","id":{"$ref":"composition"},"kind":"fold.composition.document","name":"Rust study"},
        {"op":"command","id":"fold.app.document-settings","arguments":{"document":{"$ref":"composition"},"settings":{"format":{"width":960,"height":540,"rate":[30,1],"frames":180},"output_duration":null,"timeline":false}}},
        {"op":"command","id":"fold.app.create-rust-asset","arguments":{"composition":{"$ref":"composition"},"graph":{"$ref":"graph"},"definition":{"$ref":"definition"},"instance":{"$ref":"instance"},"layer":{"$ref":"layer"},"source":source}}
    ]})).unwrap();
    let (proposal, result) =
        automation::stage(&project.snapshot(), &batch, &Default::default()).unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.commit_named(proposal, batch.label).unwrap();
    let composition: DocumentId =
        serde_json::from_value(result["ids"]["composition"].clone()).unwrap();
    let graph: DocumentId = serde_json::from_value(result["ids"]["graph"].clone()).unwrap();
    let definition = serde_json::from_value(result["ids"]["definition"].clone()).unwrap();
    let snapshot = project.snapshot();
    let comp =
        fold_composition::Composition::from_document(&snapshot.state().documents[&composition])
            .unwrap();
    assert_eq!(comp.layers.len(), 1);
    assert_eq!(comp.info.width, 960);
    assert_eq!(comp.layers[0].info, comp.info);
    assert!(comp.layers[0].graph_object().is_some());
    assert_eq!(
        snapshot.state().documents[&graph].extensions[fold_composition::LAYER_GRAPH],
        json!(true)
    );
    let m = fold_motion::Motion::from_document(&snapshot.state().documents[&graph]).unwrap();
    assert_eq!(
        fold_motion::program::record(&m, definition)
            .unwrap()
            .unwrap()
            .source,
        source
    );
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("asset.fold");
    fold_project::save(&snapshot, &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(
        reopened.snapshot().state().documents[&graph].payload,
        snapshot.state().documents[&graph].payload
    );
    project.undo().unwrap();
    assert!(project.snapshot().state().documents.is_empty());
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 2);
    let guide = automation::catalog(&json!({"guide":"rust-assets"})).unwrap();
    assert!(
        guide["text"]
            .as_str()
            .unwrap()
            .contains("fold.app.create-rust-asset")
    );
    assert!(
        automation::catalog(&json!({"guide":"rust-sdk"})).unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("pub struct Asset")
    );
}

#[test]
fn discovered_composition_edits_sync_source_clocks_and_keys_in_one_transaction() {
    let mut project = Project::new(8);
    let batch: Batch = serde_json::from_value(json!({
        "request_id":"composition-create","label":"Asset","revision":0,"project_epoch":0,
        "ids":["composition","graph","definition","instance","layer"],"operations":[
            {"op":"create","id":{"$ref":"composition"},"kind":"fold.composition.document","name":"Test"},
            {"op":"edit","document":{"$ref":"composition"},"edits":[{"op":"format","width":500,"height":888,"rate":[25,1],"frames":150}]},
            {"op":"command","id":"fold.app.create-rust-asset","arguments":{
                "composition":{"$ref":"composition"},"graph":{"$ref":"graph"},"definition":{"$ref":"definition"},"instance":{"$ref":"instance"},"layer":{"$ref":"layer"},
                "source":"let mut a=Asset::new(\"Circle\"); let c=a.circle(\"dot\",10.); a.output(&c); a"}}
        ]})).unwrap();
    let (proposal, result) =
        automation::stage(&project.snapshot(), &batch, &Default::default()).unwrap();
    project.commit(proposal).unwrap();
    let ids = &result["ids"];
    let before = project.snapshot();
    let edits = json!([
        {"op":"layer","layer":ids["layer"],"position":[100.,200.],"opacity":0.75},
        {"op":"range","layer":ids["layer"],"start":{"numerator":2,"denominator":1},"end":{"numerator":8,"denominator":1}},
        {"op":"keyframe","layer":ids["layer"],"channel":"position_y","time":{"numerator":2,"denominator":1},"value":200.},
        {"op":"keyframe","layer":ids["layer"],"channel":"position_y","time":{"numerator":3,"denominator":1},"value":300.}
    ]);
    let mut batch: Batch = serde_json::from_value(json!({"request_id":"composition-edit","label":"Move asset","revision":before.revision(),"project_epoch":0,"operations":[
        {"op":"edit","document":ids["composition"],"edits":edits}
    ]})).unwrap();
    let (proposal, _) = automation::stage(&before, &batch, &Default::default()).unwrap();
    project.commit_named(proposal, "Move asset".into()).unwrap();
    let comp: DocumentId = serde_json::from_value(ids["composition"].clone()).unwrap();
    let graph: DocumentId = serde_json::from_value(ids["graph"].clone()).unwrap();
    let snapshot = project.snapshot();
    let composition =
        fold_composition::Composition::from_document(&snapshot.state().documents[&comp]).unwrap();
    let layer = &composition.layers[0];
    assert_eq!(layer.info.width, 500);
    assert_eq!(
        layer
            .local_time(fold_foundation::Time::new(2, 1).unwrap())
            .unwrap(),
        fold_foundation::Time::ZERO
    );
    let motion = fold_motion::Motion::from_document(&snapshot.state().documents[&graph]).unwrap();
    let object = &motion.scene.as_ref().unwrap().objects[0];
    assert_eq!(object.opacity, 0.75);
    assert_eq!(object.offset[4..], [100., 200.]);
    assert_eq!(object.animation["transform.5"].keys.len(), 2);
    let matrix =
        fold_motion::evaluation::Evaluator::new(&motion, fold_foundation::Time::new(1, 2).unwrap())
            .unwrap()
            .world_matrix(object.id)
            .unwrap();
    assert_eq!(matrix[4..], [100., 250.]);
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    project.redo().unwrap();
    batch.revision = project.snapshot().revision();
    batch
        .operations
        .push(json!({"op":"edit","document":ids["composition"],"edits":[
            {"op":"lock","layer":ids["layer"],"locked":true},
            {"op":"layer","layer":ids["layer"],"opacity":0.1}
        ]}));
    let before_failure = project.snapshot();
    assert!(automation::stage(&before_failure, &batch, &Default::default()).is_err());
    assert_eq!(
        project.snapshot().state().documents,
        before_failure.state().documents
    );
    let catalog = automation::catalog(&json!({"type":"fold.composition.document"})).unwrap();
    assert_eq!(catalog["type"], "fold.composition.document");
    assert!(catalog["catalog"]["operation_schemas"].is_array());
}
