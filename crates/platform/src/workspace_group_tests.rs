use super::*;

#[test]
fn network_targets_are_per_group_and_independent_of_inspector_lock() {
    let (mut workspace, [a, b], _, _) = fixture();
    workspace.follow_network(a);
    workspace.follow_network(b);
    workspace.toggle_inspector_lock();
    workspace.record_selection(b);
    assert_eq!(workspace.inspector_editor(), Some(a));
    assert_eq!(workspace.network_target(LinkGroup::A), Some(a));
    assert_eq!(workspace.network_target(LinkGroup::B), Some(b));
    workspace.network_group = LinkGroup::B;
    assert_eq!(workspace.inspector_group, LinkGroup::A);
    workspace.set_editor_group(b, LinkGroup::Unlinked);
    workspace.follow_network(b);
    workspace.record_selection(b);
    assert!(!workspace.network_targets.contains_key(&LinkGroup::Unlinked));
    assert!(
        !workspace
            .group_selections
            .contains_key(&LinkGroup::Unlinked)
    );
    assert_eq!(workspace.network_target(LinkGroup::B), None);
    workspace.close_editor(a);
    assert!(workspace.inspector_lock.is_none());
    assert_eq!(workspace.network_target(LinkGroup::A), None);
}
fn location(document: DocumentId) -> ViewLocation {
    ViewLocation {
        document,
        time: Time::new(7, 48).unwrap(),
        label: "Same name".into(),
    }
}
fn fixture() -> (
    Workspace,
    [PanelInstanceId; 2],
    [PanelInstanceId; 2],
    [DocumentId; 2],
) {
    let mut w = Workspace::default();
    w.project = "/project/groups.fold".into();
    let docs = [DocumentId::new(), DocumentId::new()];
    let a = w.add_editor("editor", "kind");
    let b = w.add_editor("editor", "kind");
    w.editors.get_mut(&a).unwrap().bind(location(docs[0]));
    w.refresh_outputs(test_output);
    w.editors.get_mut(&b).unwrap().bind(location(docs[1]));
    w.refresh_outputs(test_output);
    w.record_selection(a);
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
    (w, [a, b], [x, y], docs)
}
#[test]
fn sources_inspectors_clocks_and_selection_stay_in_their_groups() {
    let (mut w, [a, b], [x, y], docs) = fixture();
    w.focused_editor = Some(b);
    w.inspector_viewer = Some(y);
    assert_eq!(w.resolve(x).unwrap().document, docs[0]);
    assert_eq!(w.resolve(y).unwrap().document, docs[1]);
    assert_eq!(w.inspector_editor(), Some(a));
    assert_eq!(w.inspector_context(), Some((a, Time::new(7, 48).unwrap())));
    w.viewers.get_mut(&y).unwrap().time = Time::new(5, 24).unwrap();
    assert_eq!(w.inspector_context(), Some((a, Time::new(7, 48).unwrap())));
    w.inspector_group = LinkGroup::B;
    assert_eq!(w.inspector_context(), Some((b, Time::new(5, 24).unwrap())));
    let nested = DocumentId::new();
    w.editors.get_mut(&b).unwrap().navigate(location(nested));
    w.refresh_outputs(test_output);
    assert_eq!(w.resolve(y).unwrap().document, nested);
    assert_eq!(w.resolve(x).unwrap().document, docs[0]);
    w.set_viewer_group(y, LinkGroup::A);
    assert_eq!(w.resolve(y), w.resolve(x));
    assert_ne!(w.viewers[&y].time, w.viewers[&x].time);
}
#[test]
fn independent_sources_selection_lock_missing_types_and_closure() {
    let (mut w, [a, b], [x, y], docs) = fixture();
    let motion = w.add_editor("motion", "motion");
    w.editors.get_mut(&motion).unwrap().bind(location(docs[1]));
    w.refresh_outputs(test_output);
    let object = fold_foundation::ObjectId::new();
    w.editors.get_mut(&motion).unwrap().selection.objects = vec![object];
    w.follow_source(y, motion);
    w.record_selection(motion);
    assert_eq!(w.inspector_editor(), Some(motion));
    assert_eq!(w.editor_for_viewer(x), Some(a));
    assert_eq!(w.editor_for_viewer(y), Some(motion));
    assert_eq!(w.viewers[&x].time, Time::new(7, 48).unwrap());
    assert_eq!(w.viewers[&y].time, Time::new(1, 8).unwrap());
    w.toggle_inspector_lock();
    w.editors
        .get_mut(&motion)
        .unwrap()
        .bind(location(DocumentId::new()));
    w.refresh_outputs(test_output);
    w.record_selection(a);
    assert_eq!(w.inspector_editor(), Some(motion));
    let locked = &w.inspector_lock.as_ref().unwrap().1;
    assert_eq!(locked.document(), Some(docs[1]));
    assert_eq!(locked.selection.objects, vec![object]);
    // No matching viewer for the locked network: retain its exact local time.
    assert_eq!(
        w.inspector_context(),
        Some((motion, Time::new(1, 8).unwrap()))
    );
    w.toggle_inspector_lock();
    assert_eq!(w.inspector_editor(), Some(a));
    w.set_viewer_group(y, LinkGroup::B);
    assert_eq!(
        w.resolve(y),
        None,
        "B has no Motion; never choose its compositor"
    );
    assert_eq!(w.viewers[&y].source.as_deref(), Some("motion"));
    w.set_viewer_group(y, LinkGroup::A);
    assert_eq!(w.editor_for_viewer(y), Some(motion));
    w.set_viewer_group(x, LinkGroup::B);
    assert_eq!(w.editor_for_viewer(x), Some(b));
    let output = w.resolve(y).unwrap();
    w.close_editor(motion);
    assert_eq!(w.resolve(y), Some(output));
    assert!(matches!(w.viewers[&y].binding, ViewerBinding::Pinned(_)));
    assert_eq!(w.editor_for_viewer(x), Some(b));
    let mut restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    assert_eq!(restored.resolve(x), w.resolve(x));
    assert_eq!(restored.resolve(y), w.resolve(y));
}
#[test]
fn occupied_groups_are_rejected_and_moving_source_unlinks_only_its_followers() {
    let (mut w, [a, b], [x, y], docs) = fixture();
    w.set_editor_group(a, LinkGroup::B);
    assert_eq!(w.editors[&a].group, LinkGroup::A);
    assert_eq!(w.editor_for_viewer(x), Some(a));
    w.set_editor_group(a, LinkGroup::C);
    assert!(matches!(w.viewers[&x].binding, ViewerBinding::Pinned(_)));
    assert_eq!(w.resolve(x).unwrap().document, docs[0]);
    assert_eq!(w.editor_for_viewer(y), Some(b));
    assert_eq!(w.viewers[&y].time, Time::new(1, 8).unwrap());
    let d = w.add_editor("editor", "kind");
    assert_eq!(w.editors[&d].group, LinkGroup::A);
    let e = w.add_editor("editor", "kind");
    assert_eq!(w.editors[&e].group, LinkGroup::D);
    let f = w.add_editor("editor", "kind");
    assert_eq!(w.editors[&f].group, LinkGroup::Unlinked);
    let mut restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    assert_eq!(restored.resolve(x), w.resolve(x));
    assert_eq!(restored.resolve(y), w.resolve(y));
}
#[test]
fn all_network_choice_unlinks_one_viewer_and_lock_and_source_restore() {
    let (mut w, [a, _], [x, y], _) = fixture();
    let other = w.resolve(y);
    w.toggle_inspector_lock();
    let output = DocumentRef {
        document: DocumentId::new(),
        output: "matte".into(),
        extensions: Default::default(),
    };
    w.pin_output(x, output.clone());
    w.editors
        .get_mut(&a)
        .unwrap()
        .bind(location(DocumentId::new()));
    w.refresh_outputs(test_output);
    assert_eq!(w.resolve(x), Some(output));
    assert_eq!(w.resolve(y), other);
    let mut restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    assert_eq!(
        restored.inspector_lock.as_ref().unwrap().1.document(),
        w.inspector_lock.as_ref().unwrap().1.document()
    );
    assert_eq!(restored.resolve(x), w.resolve(x));
    assert_eq!(restored.viewers[&x].time, w.viewers[&x].time);
}
#[test]
fn portable_presets_remove_all_project_bindings_but_keep_layout_and_links() {
    use crate::desktop::ViewLocation;
    use fold_foundation::{DocumentId, Time};
    let mut workspace = Workspace::default();
    workspace.project = "/project.fold".into();
    workspace.layout = "layout".into();
    let editor = workspace.add_editor("editor", "kind");
    workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: DocumentId::new(),
            time: Time::new(5, 24).unwrap(),
            label: "Private document".into(),
        });
    workspace.refresh_outputs(test_output);
    workspace.record_selection(editor);
    workspace.toggle_inspector_lock();
    let output = workspace.output(&workspace.editors[&editor]).unwrap();
    let viewer = workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(output.clone()),
        last_output: Some(output),
        editor: Some(editor),
        playing: true,
        ..Default::default()
    });
    let preset = workspace.preset();
    assert!(preset.project.is_empty());
    assert_eq!(preset.layout, "layout");
    assert!(preset.editors[&editor].navigation.is_empty());
    assert!(preset.editors[&editor].selection.document.is_none());
    assert!(preset.inspector_lock.is_none());
    assert_eq!(preset.viewers[&viewer].binding, ViewerBinding::Unbound);
    assert!(preset.viewers[&viewer].last_output.is_none());
    assert!(preset.viewers[&viewer].editor.is_none());
    assert!(!preset.viewers[&viewer].playing);
    assert_eq!(
        preset.editors[&editor].group,
        workspace.editors[&editor].group
    );
    let bytes = serde_json::to_vec(&preset).unwrap();
    assert!(
        !String::from_utf8(bytes.clone())
            .unwrap()
            .contains("Private document")
    );
    Workspace::decode(&bytes, "").unwrap();
    assert!(!workspace.editors[&editor].navigation.is_empty());
}

#[test]
fn named_provider_outputs_refresh_navigation_pinning_and_restore_without_stale_defaults() {
    let mut w = Workspace::default();
    let editor = w.add_editor("motion", "construction");
    let root = DocumentId::new();
    let child = DocumentId::new();
    let reference = |document, port: &str| DocumentRef {
        document,
        output: port.into(),
        extensions: Default::default(),
    };
    let root_output = reference(root, "scene-a");
    let child_output = reference(child, "scene-b");
    w.editors.get_mut(&editor).unwrap().bind(location(root));
    w.record_selection(editor);
    let linked = w.add_viewer(Default::default());
    assert!(w.resolve(linked).is_none());
    assert!(w.editor_for_viewer(linked).is_none());
    let resolve = |document| {
        Some(if document == root {
            root_output.clone()
        } else {
            child_output.clone()
        })
    };
    w.refresh_outputs(resolve);
    assert_eq!(w.resolve(linked), Some(root_output.clone()));
    assert_eq!(w.editor_for_viewer(linked), Some(editor));
    let pinned = w.add_viewer(Default::default());
    w.pin_output(pinned, root_output.clone());
    w.editors
        .get_mut(&editor)
        .unwrap()
        .navigate(location(child));
    assert!(
        w.resolve(linked).is_none(),
        "A new location must not retain the previous output"
    );
    w.refresh_outputs(resolve);
    assert_eq!(w.resolve(linked), Some(child_output));
    assert_eq!(w.resolve(pinned), Some(root_output.clone()));
    assert_eq!(w.editor_for_viewer(pinned), None);
    w.editors.get_mut(&editor).unwrap().back();
    assert_eq!(w.resolve(linked), Some(root_output.clone()));
    let changed = reference(root, "scene-c");
    w.refresh_outputs(|document| (document == root).then(|| changed.clone()));
    assert_eq!(w.resolve(linked), Some(changed.clone()));
    assert_eq!(w.resolve(pinned), Some(root_output));
    assert_eq!(w.editor_for_viewer(pinned), None);
    let json = serde_json::to_value(&w).unwrap();
    assert!(json.get("outputs").is_none());
    let mut restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), "").unwrap();
    assert!(restored.resolve(linked).is_none());
    restored.refresh_outputs(|document| (document == root).then(|| changed.clone()));
    assert_eq!(restored.resolve(linked), Some(changed));
    restored.refresh_outputs(|_| None);
    assert!(restored.resolve(linked).is_none());
    assert!(restored.editor_for_viewer(linked).is_none());
    assert!(
        restored.resolve(pinned).is_some(),
        "Pinned intent survives provider unavailability"
    );
}

#[test]
fn unsupported_workspace_generations_are_rejected() {
    let mut value = serde_json::to_value(Workspace::default()).unwrap();
    for version in [1, 2, 3, 5] {
        value["version"] = serde_json::json!(version);
        assert!(Workspace::decode(&serde_json::to_vec(&value).unwrap(), "").is_err());
    }
}

#[test]
fn explicit_working_view_switches_existing_viewer_without_selection_following() {
    let (mut w, [parent, _], [viewer, other], docs) = fixture();
    let child = w.add_editor("compositor", "compositor");
    w.editors.get_mut(&child).unwrap().bind(location(docs[1]));
    w.refresh_outputs(test_output);
    w.record_selection(child);
    assert_eq!(w.resolve(viewer).unwrap().document, docs[0]);
    let other_output = w.resolve(other);
    let time = Time::new(13, 24).unwrap();
    assert_eq!(w.view_editor(child, time), Some(viewer));
    assert_eq!(w.editor_for_viewer(viewer), Some(child));
    assert_eq!(w.resolve(viewer).unwrap().document, docs[1]);
    assert_eq!(w.viewers[&viewer].time, time);
    w.record_selection(parent);
    assert_eq!(w.editor_for_viewer(viewer), Some(child));
    assert_eq!(w.resolve(other), other_output);
}
