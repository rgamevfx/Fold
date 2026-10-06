use fold_project::{Candidate, EditBatch, Mutation, Project};

#[test]
fn independent_generations_conflict_with_live_edits_and_preserve_undo() {
    let mut project = Project::new(8);
    let initial = project.snapshot();
    let candidate = Candidate::new(initial.clone());
    let edit = |base, value| EditBatch {
        base,
        mutations: vec![Mutation::SetSettings(std::collections::BTreeMap::from([(
            "artist".into(),
            serde_json::json!(value),
        )]))],
    };
    let first = candidate.apply(edit(initial.revision(), 1)).unwrap();
    let second = first.apply(edit(first.snapshot().revision(), 2)).unwrap();
    assert_eq!(candidate.generation(), 0);
    assert_eq!(first.generation(), 1);
    assert_eq!(second.generation(), 2);
    assert!(project.snapshot().state().settings.is_empty());
    project
        .commit_named(second.proposal(), "Two private changes".into())
        .unwrap();
    assert_eq!(project.snapshot().state().settings["artist"], 2);
    assert!(project.commit(first.proposal()).is_err());
    assert_eq!(project.snapshot().state().settings["artist"], 2);
    project.undo().unwrap();
    assert!(project.snapshot().state().settings.is_empty());
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().settings["artist"], 2);
}
