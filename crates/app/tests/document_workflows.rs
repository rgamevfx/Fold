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
        (fold_motion::CONSTRUCTION, "Motion graph"),
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
            matches!(kind, fold_motion::CONSTRUCTION | fold_compositor::COMPOSITE)
        );
        project.undo().unwrap();
        assert!(project.snapshot().state().documents.is_empty());
    }
}
