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
        if let Some((project, result)) = store.take() {
            assert_eq!(project, workspace.project);
            assert_eq!(result.unwrap().unwrap().focused_editor, Some(editor));
            break;
        }
        assert!(start.elapsed().as_secs() < 3);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
