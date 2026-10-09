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
fn version_three_duplicate_groups_preserve_viewed_document_during_migration() {
    let (w, [a, b], [x, y], _) = fixture();
    let mut json = serde_json::to_value(&w).unwrap();
    json["version"] = serde_json::json!(3);
    json["editors"][b.0.to_string()]["group"] = serde_json::json!("A");
    json["group_sources"] = serde_json::json!({"A": b});
    for viewer in [x, y] {
        json["viewers"][viewer.0.to_string()]["group"] = serde_json::json!("A");
    }
    let mut restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    assert_ne!(restored.editors[&a].group, restored.editors[&b].group);
    assert_eq!(restored.inspector_editor(), Some(b));
    for viewer in [x, y] {
        assert_eq!(restored.resolve(viewer), w.output(&w.editors[&b]));
        assert_eq!(restored.viewers[&viewer].time, w.viewers[&viewer].time);
    }
}
#[test]
fn migration_freezes_excess_sources_instead_of_merging_them_and_preserves_pins() {
    let mut w = Workspace::default();
    w.project = "/project/migration.fold".into();
    let mut follows = Vec::new();
    for _ in 0..5 {
        let editor = w.add_editor("editor", "kind");
        let document = DocumentId::new();
        w.editors.get_mut(&editor).unwrap().bind(location(document));
        w.refresh_outputs(test_output);
        let viewer = w.add_viewer(Default::default());
        follows.push((viewer, editor, document));
    }
    let pinned = DocumentRef {
        document: DocumentId::new(),
        output: "matte".into(),
        extensions: Default::default(),
    };
    let pin = w.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(pinned.clone()),
        ..Default::default()
    });
    let mut json = serde_json::to_value(&w).unwrap();
    json["version"] = serde_json::json!(1);
    for (viewer, editor, _) in &follows {
        json["viewers"][viewer.0.to_string()]["binding"] = serde_json::json!({"Follow": editor});
    }
    let mut restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    for (viewer, _, document) in &follows {
        assert_eq!(restored.resolve(*viewer).unwrap().document, *document);
    }
    assert_eq!(restored.group_selections.len(), 4);
    assert!(matches!(
        restored.viewers[&follows[4].0].binding,
        ViewerBinding::Pinned(_)
    ));
    assert_eq!(restored.resolve(pin), Some(pinned));
}

#[test]
fn legacy_follow_bindings_migrate_to_separate_groups_without_changing_time_or_pin() {
    let (w, [a, b], [x, y], _) = fixture();
    let mut json = serde_json::to_value(&w).unwrap();
    json["version"] = serde_json::json!(1);
    json.as_object_mut().unwrap().remove("group_selections");
    json.as_object_mut().unwrap().remove("inspector_group");
    for (viewer, editor) in [(x, a), (y, b)] {
        let v = &mut json["viewers"][viewer.0.to_string()];
        v.as_object_mut().unwrap().remove("group");
        v["binding"] = serde_json::json!({"Follow": editor});
    }
    for editor in [a, b] {
        json["editors"][editor.0.to_string()]
            .as_object_mut()
            .unwrap()
            .remove("group");
    }
    let mut restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), &w.project).unwrap();
    restored.refresh_outputs(test_output);
    assert_eq!(restored.version, 4);
    for viewer in [x, y] {
        assert_eq!(restored.resolve(viewer), w.resolve(viewer));
        assert_eq!(restored.viewers[&viewer].time, w.viewers[&viewer].time);
    }
    assert_eq!(restored.editors[&b].group, LinkGroup::B);
    let mut invalid = serde_json::to_value(&restored).unwrap();
    invalid["viewers"][x.0.to_string()]["group"] = serde_json::json!("E");
    assert!(Workspace::decode(&serde_json::to_vec(&invalid).unwrap(), &w.project).is_err());
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
