use super::*;
#[test]
fn sidecars_round_trip_without_project_io_and_flush_latest_state_on_close() {
    let root = std::env::temp_dir().join(format!(
        "fold-workspace-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut workspace = Workspace::default();
    workspace.project = "/project/original.fold".into();
    let editor = workspace.add_editor("unavailable.editor", "unknown.document");
    workspace.focused_editor = Some(editor);
    {
        let mut store = WorkspaceStore::at(Some(root.clone()));
        for _ in 0..10 {
            store.save(workspace.clone());
        }
        workspace.add_viewer(fold_platform::workspace::ViewerInstance {
            pixel_exact: true,
            ..Default::default()
        });
        store.save(workspace.clone());
        // Drop drains bounded/coalesced saves, including the last half-second.
    }
    let path = sidecar(&root, &workspace.project);
    let loaded = Workspace::decode(&std::fs::read(path).unwrap(), &workspace.project).unwrap();
    assert_eq!(loaded.viewers.len(), 1);
    assert!(loaded.viewers.values().next().unwrap().pixel_exact);
    assert_eq!(loaded.editors[&editor].contribution, "unavailable.editor");
    let mut store = WorkspaceStore::at(Some(root.clone()));
    store.load(workspace.project.clone());
    let start = std::time::Instant::now();
    loop {
        if let Some(completion) = store.take() {
            assert!(completion.load_ticket.is_some());
            assert_eq!(completion.project, workspace.project);
            assert_eq!(
                completion.value.unwrap().unwrap().focused_editor,
                Some(editor)
            );
            break;
        }
        assert!(start.elapsed().as_secs() < 3);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn same_path_reload_rejects_an_earlier_lifetime() {
    let (sender, _requests) = mpsc::sync_channel(1);
    let (publish, result) = mpsc::channel();
    let mut store = WorkspaceStore {
        sender: Some(sender),
        result,
        pending: VecDeque::new(),
        load_ticket: 0,
        thread: None,
    };
    let project = "/same.fold".to_string();
    store.load(project.clone());
    let old = store.load_ticket;
    store.invalidate_load();
    publish
        .send(Completion {
            load_ticket: Some(old),
            project: project.clone(),
            value: Ok(Some(Workspace::default())),
        })
        .unwrap();
    assert!(store.take().is_none());
    publish
        .send(Completion {
            load_ticket: Some(store.load_ticket),
            project: project.clone(),
            value: Ok(None),
        })
        .unwrap();
    let completion = store.take().unwrap();
    assert!(completion.load_ticket.is_some());
    assert_eq!(completion.project, project);
    assert!(completion.value.unwrap().is_none());
    // Save errors must not complete or replace a pending load.
    publish
        .send(Completion {
            load_ticket: None,
            project,
            value: Err("save failed".into()),
        })
        .unwrap();
    assert!(store.take().unwrap().load_ticket.is_none());
}
