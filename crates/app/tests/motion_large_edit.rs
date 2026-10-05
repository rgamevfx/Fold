use fold_motion::Motion;
use fold_platform::packages::CommandRequest;

#[test]
#[ignore = "requires a generated project in FOLD_EDIT_PROJECT"]
fn reference_project_accepts_a_network_edit() {
    let project =
        fold_project::load(std::env::var("FOLD_EDIT_PROJECT").expect("project path"), 8).unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_motion::MOTION)
        .unwrap();
    let before = Motion::from_document(document).unwrap();
    let mut motion = before.clone();
    motion.graph.nodes[0].position[0] += 10.;
    let request = CommandRequest {
        id: fold_motion::package::CHANGE.into(),
        base: snapshot.revision(),
        arguments: serde_json::to_vec(
            &fold_motion::edits::Changes::between(document.id, &before, &motion).unwrap(),
        )
        .unwrap(),
    };
    assert!(request.arguments.len() < 1024);
    fold_app::packages::builtins()
        .stage(&snapshot, &request, &Default::default())
        .unwrap();
}
