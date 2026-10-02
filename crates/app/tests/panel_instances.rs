use fold_app::{media_workflow, session::Session};
use fold_foundation::{DocumentId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection, ViewLocation},
    panel_context::PanelContext,
    workspace::{ViewerBinding, ViewerInstance, Workspace},
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

fn fixture() -> (Session, [DocumentId; 2]) {
    let ids = [DocumentId::new(), DocumentId::new()];
    let mut project = Project::new(8);
    let mut motion = fold_motion::Motion::empty();
    motion.info.width = 32;
    motion.info.height = 32;
    fold_motion::authoring::add_content(&mut motion, "fold.motion.rectangle").unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: ids
                .iter()
                .map(|&id| Mutation::PutDocument(motion.document(id).unwrap()))
                .collect(),
        })
        .unwrap();
    project
        .commit(media_workflow::select_output(&project.snapshot(), ids[0]).unwrap())
        .unwrap();
    (Session::new(project), ids)
}
fn location(document: DocumentId, time: Time) -> ViewLocation {
    ViewLocation {
        document,
        time,
        label: "Same name".into(),
    }
}

#[test]
fn two_editors_and_viewers_preserve_exact_targets_delivery_and_undo() {
    let (mut session, ids) = fixture();
    let before = session.snapshot().unwrap();
    let mut workspace = Workspace::default();
    let a = workspace.add_editor("fold.motion.canvas", fold_motion::MOTION);
    let b = workspace.add_editor("fold.motion.canvas", fold_motion::MOTION);
    workspace
        .editors
        .get_mut(&a)
        .unwrap()
        .bind(location(ids[0], Time::new(7, 48).unwrap()));
    workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .bind(location(ids[1], Time::new(1, 24).unwrap()));
    workspace.publish(a);
    let x = workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(a),
        ..Default::default()
    });
    let y = workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(DocumentRef {
            document: ids[1],
            output: "video".into(),
            extensions: Default::default(),
        }),
        ..Default::default()
    });
    {
        let mut context = PanelContext::new(&mut session, workspace.editors.get_mut(&a).unwrap());
        context.command(DesktopCommand::Navigate(location(
            ids[1],
            Time::new(1, 48).unwrap(),
        )));
        assert_eq!(context.state().viewer_document, Some(ids[1]));
        assert_eq!(
            context.state().navigation.last().unwrap().time,
            Time::new(1, 48).unwrap()
        );
        context.command(DesktopCommand::Select(Selection {
            document: Some(ids[1]),
            objects: vec![],
        }));
        context.command(DesktopCommand::NavigateBack);
    }
    assert_eq!(
        workspace.editors[&a].navigation.last().unwrap().time,
        Time::new(7, 48).unwrap()
    );
    assert_eq!(workspace.editors[&b].document(), Some(ids[1]));
    workspace.focused_editor = Some(b);
    assert_eq!(workspace.resolve(x).unwrap().document, ids[0]);
    assert_eq!(workspace.resolve(y).unwrap().document, ids[1]);
    for (viewer, time) in [
        (x, Time::new(7, 48).unwrap()),
        (y, Time::new(1, 24).unwrap()),
    ] {
        let output = workspace.resolve(viewer).unwrap();
        let state = session.preview_state(&output, time);
        let key = state.preview_key(state.frame, 1).unwrap();
        assert_eq!(key.target, Some((output.document, time)));
        let frame = media_workflow::evaluate(
            &before,
            &key,
            &mut fold_media::Decoder::default(),
            &fold_media::Cancel::default(),
        )
        .unwrap();
        assert_eq!(frame.to_display().unwrap().dimensions(), [32, 32]);
    }
    // Two views of one document retain distinct exact (including subframe) time.
    workspace.viewers.get_mut(&y).unwrap().binding =
        ViewerBinding::Pinned(workspace.editors[&a].output().unwrap());
    let output = workspace.resolve(y).unwrap();
    let one = session.preview_state(&output, Time::new(7, 48).unwrap());
    let two = session.preview_state(&output, Time::new(1, 8).unwrap());
    let one = one.preview_key(one.frame, 1).unwrap();
    let two = two.preview_key(two.frame, 1).unwrap();
    assert_eq!(one.frame, two.frame);
    assert_eq!(one.content, two.content);
    assert_ne!(one, two);
    assert_eq!(session.snapshot().unwrap().revision(), before.revision());
    assert_eq!(session.output_info().unwrap().0, ids[0]);
    // The last authored undo entry is still delivery selection, not navigation.
    session.command(DesktopCommand::Undo);
    assert_eq!(
        session.snapshot().unwrap().state().settings,
        Project::new(0).snapshot().state().settings
    );
}

#[test]
fn group_sources_do_not_change_delivery_project_history_or_other_viewer_time() {
    use fold_platform::workspace::LinkGroup;
    let (mut session, ids) = fixture();
    let before = session.snapshot().unwrap();
    let mut w = Workspace::default();
    let a = w.add_editor("fold.motion.canvas", fold_motion::MOTION);
    let b = w.add_editor("fold.motion.canvas", fold_motion::MOTION);
    w.editors
        .get_mut(&a)
        .unwrap()
        .bind(location(ids[0], Time::ZERO));
    w.editors
        .get_mut(&b)
        .unwrap()
        .bind(location(ids[1], Time::ZERO));
    w.publish(a);
    w.set_editor_group(b, LinkGroup::B);
    let x = w.add_viewer(ViewerInstance {
        time: Time::new(7, 48).unwrap(),
        ..Default::default()
    });
    let y = w.add_viewer(ViewerInstance {
        group: LinkGroup::B,
        time: Time::new(1, 8).unwrap(),
        ..Default::default()
    });
    w.focused_editor = Some(b);
    w.inspector_group = LinkGroup::A;
    assert_eq!(w.inspector_editor(), Some(a));
    for viewer in [x, y] {
        let output = w.resolve(viewer).unwrap();
        let state = session.preview_state(&output, w.viewers[&viewer].time);
        let key = state.preview_key(state.frame, 1).unwrap();
        assert_eq!(key.target, Some((output.document, w.viewers[&viewer].time)));
        assert_eq!(
            media_workflow::evaluate(
                &before,
                &key,
                &mut fold_media::Decoder::default(),
                &fold_media::Cancel::default()
            )
            .unwrap()
            .to_display()
            .unwrap()
            .dimensions(),
            [32, 32]
        );
    }
    w.reconcile();
    let pinned = w.resolve(x).unwrap();
    w.viewers.get_mut(&x).unwrap().binding = ViewerBinding::Pinned(pinned);
    w.editors
        .get_mut(&a)
        .unwrap()
        .bind(location(ids[1], Time::new(1, 48).unwrap()));
    w.publish(a);
    assert_eq!(w.resolve(x).unwrap().document, ids[0]);
    assert_eq!(w.viewers[&y].time, Time::new(1, 8).unwrap());
    w.set_viewer_group(x, LinkGroup::B);
    assert_eq!(w.resolve(x), w.resolve(y));
    assert_ne!(w.viewers[&x].time, w.viewers[&y].time);
    assert_eq!(session.snapshot().unwrap().revision(), before.revision());
    assert_eq!(session.output_info().unwrap().0, ids[0]);
    session.command(DesktopCommand::Undo);
    assert_eq!(
        session.snapshot().unwrap().state().settings,
        Project::new(0).snapshot().state().settings
    );
}

#[test]
fn missing_targets_and_restore_never_choose_another_document() {
    let (mut session, ids) = fixture();
    let mut w = Workspace::default();
    w.project = "/fixture/project.fold".into();
    let editor = w.add_editor("fold.motion.canvas", fold_motion::MOTION);
    w.editors
        .get_mut(&editor)
        .unwrap()
        .bind(location(ids[1], Time::ZERO));
    w.publish(editor);
    let viewer = w.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        ..Default::default()
    });
    w.reconcile();
    let snapshot = session.snapshot().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("project.fold");
    fold_project::save(&snapshot, &path).unwrap();
    let mut project = fold_project::load(&path, 8).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::RemoveDocument(ids[1])],
        })
        .unwrap();
    session = Session::new(project);
    let output = w.resolve(viewer).unwrap();
    assert!(session.preview_state(&output, Time::ZERO).content.is_none());
    let state = PanelContext::new(&mut session, w.editors.get_mut(&editor).unwrap());
    assert_eq!(state.state().selection.document, Some(ids[1]));
    assert!(state.explicit_target());
    drop(state);
    let restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
    assert_eq!(restored.resolve(viewer), Some(output));
    assert_eq!(session.output_info().unwrap().0, ids[0]);
    w.close_editor(editor);
    assert!(matches!(
        w.viewers[&viewer].binding,
        ViewerBinding::Pinned(_)
    ));
    assert_eq!(w.resolve(viewer).unwrap().document, ids[1]);
}

#[test]
fn duplicate_labels_and_rename_do_not_change_bound_ids_or_render_content() {
    let (session, ids) = fixture();
    let snapshot = session.snapshot().unwrap();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: snapshot
                .state()
                .documents
                .values()
                .map(|d| Mutation::PutDocument((**d).clone()))
                .collect(),
        })
        .unwrap();
    let content = media_workflow::content_for(&project.snapshot(), ids[0]).unwrap();
    let mut items = project.snapshot().state().organization.items.clone();
    for item in &mut items {
        item.name = "Same".into();
    }
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: items.iter().cloned().map(Mutation::PutItem).collect(),
        })
        .unwrap();
    let labels = fold_platform::workspace::document_labels(&project.snapshot());
    assert_ne!(labels[&ids[0]], labels[&ids[1]]);
    items
        .iter_mut()
        .find(|i| i.id == fold_project::ItemId::Document(ids[0]))
        .unwrap()
        .name = "Renamed".into();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: items.into_iter().map(Mutation::PutItem).collect(),
        })
        .unwrap();
    assert!(
        fold_platform::workspace::document_labels(&project.snapshot())[&ids[0]]
            .starts_with("Renamed")
    );
    assert_eq!(
        media_workflow::content_for(&project.snapshot(), ids[0]).unwrap(),
        content
    );
}

#[test]
fn scoped_seeks_and_marks_do_not_use_global_selection_or_time() {
    let (mut session, ids) = fixture();
    let mut w = Workspace::default();
    let id = w.add_editor("fold.motion.canvas", fold_motion::MOTION);
    w.editors
        .get_mut(&id)
        .unwrap()
        .bind(location(ids[1], Time::new(7, 48).unwrap()));
    let global = session.state().clone();
    let mut context = PanelContext::new(&mut session, w.editors.get_mut(&id).unwrap());
    use fold_platform::desktop::TransportAction as A;
    context.command(DesktopCommand::Transport(A::NextFrame));
    context.command(DesktopCommand::Transport(A::MarkIn));
    assert_eq!(context.state().frame, 4);
    assert_eq!(context.state().playback_range.start, Some(4));
    context.command(DesktopCommand::Play);
    assert!(context.state().playing);
    assert_eq!(context.playback_requested(), Some(true));
    context.command(DesktopCommand::Pause);
    assert_eq!(context.playback_requested(), Some(false));
    assert_eq!(context.state().frame, 4);
    context.command(DesktopCommand::Navigate(location(
        ids[0],
        Time::new(-1, 24).unwrap(),
    )));
    assert_eq!(context.state().viewer_document, Some(ids[1]));
    drop(context);
    assert_eq!(
        session.state().selection.document,
        global.selection.document
    );
    assert_eq!(session.state().frame, global.frame);
    assert_eq!(session.state().navigation, global.navigation);
    assert!(!session.state().playing);
}
