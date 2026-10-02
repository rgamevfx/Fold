use super::*;
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
    w.editors.get_mut(&b).unwrap().bind(location(docs[1]));
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
    assert_eq!(w.resolve(y).unwrap().document, nested);
    assert_eq!(w.resolve(x).unwrap().document, docs[0]);
    w.set_viewer_group(y, LinkGroup::A);
    assert_eq!(w.resolve(y), w.resolve(x));
    assert_ne!(w.viewers[&y].time, w.viewers[&x].time);
}
#[test]
fn competing_editors_require_explicit_publication_and_closure_pins_only_their_followers() {
    let (mut w, [a, b], [x, y], docs) = fixture();
    let other = w.add_editor("editor", "kind");
    w.editors.get_mut(&other).unwrap().bind(location(docs[1]));
    w.focused_editor = Some(other);
    assert_eq!(w.editor_for_viewer(x), Some(a));
    w.publish(other);
    assert_eq!(w.editor_for_viewer(x), Some(other));
    assert_eq!(w.editor_for_viewer(y), Some(b));
    w.close_editor(other);
    assert!(matches!(w.viewers[&x].binding, ViewerBinding::Pinned(_)));
    assert_eq!(w.resolve(x).unwrap().document, docs[1]);
    assert_eq!(w.viewers[&y].binding, ViewerBinding::Linked);
    assert_eq!(
        w.group_sources.get(&LinkGroup::A),
        None,
        "never choose a replacement by focus"
    );
}
#[test]
fn moving_a_source_pins_old_group_and_explicitly_supplies_new_group() {
    let (mut w, [a, b], [x, y], docs) = fixture();
    w.set_editor_group(a, LinkGroup::B);
    assert!(matches!(w.viewers[&x].binding, ViewerBinding::Pinned(_)));
    assert_eq!(w.resolve(x).unwrap().document, docs[0]);
    assert_eq!(w.editor_for_viewer(y), Some(a));
    assert_eq!(w.editors[&b].group, LinkGroup::B);
    assert_eq!(w.viewers[&y].time, Time::new(1, 8).unwrap());
    let restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
    assert_eq!(restored.resolve(x), w.resolve(x));
    assert_eq!(restored.resolve(y), w.resolve(y));
    assert_eq!(restored.group_sources, w.group_sources);
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
    let restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), &w.project).unwrap();
    for (viewer, _, document) in &follows {
        assert_eq!(restored.resolve(*viewer).unwrap().document, *document);
    }
    assert_eq!(restored.group_sources.len(), 4);
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
    json.as_object_mut().unwrap().remove("group_sources");
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
    let restored = Workspace::decode(&serde_json::to_vec(&json).unwrap(), &w.project).unwrap();
    assert_eq!(restored.version, 3);
    for viewer in [x, y] {
        assert_eq!(restored.resolve(viewer), w.resolve(viewer));
        assert_eq!(restored.viewers[&viewer].time, w.viewers[&viewer].time);
    }
    assert_eq!(restored.editors[&b].group, LinkGroup::B);
    let mut invalid = serde_json::to_value(&restored).unwrap();
    invalid["viewers"][x.0.to_string()]["group"] = serde_json::json!("E");
    assert!(Workspace::decode(&serde_json::to_vec(&invalid).unwrap(), &w.project).is_err());
}
