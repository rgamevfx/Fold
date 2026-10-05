use super::*;
use std::time::Duration;

#[test]
fn preferences_roundtrip_preserves_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fold/ui.json");
    let mut doc = json!({"version": 1, "future": {"keep": true}, "ui": {"graph": {"future": 9}}});
    let mut value = Appearance::default();
    value.application_text = 19.;
    value.graph.titles = 30.;
    value.colors.text = [0.9, 0.8, 0.7];
    save(
        &path,
        &mut doc,
        Preferences {
            ui: value,
            ..Default::default()
        },
    )
    .unwrap();
    let (loaded, doc) = load(&path).unwrap();
    assert_eq!(loaded.ui, value);
    assert_eq!(doc["future"]["keep"], true);
    assert_eq!(doc["ui"]["graph"]["future"], 9);
}

#[test]
fn invalid_or_newer_preferences_are_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui.json");
    for content in [
        "{broken",
        r#"{"version":2}"#,
        r#"{"version":1,"ui":{"application_text":1000}}"#,
        r#"{"version":1,"ui":{"colors":{"text":[1,2,3]}}}"#,
    ] {
        std::fs::write(&path, content).unwrap();
        let mut store = Store::new(Some(path.clone()));
        assert!(matches!(
            store.result.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Loaded(Err(_))
        ));
        store.save(Preferences::default());
        drop(store);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
    std::fs::write(&path, vec![b' '; 16 * 1024 * 1024 + 1]).unwrap();
    assert!(load(&path).is_err());
}

#[test]
fn worker_flushes_latest_edit_on_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui.json");
    let mut store = Store::new(Some(path.clone()));
    assert!(matches!(
        store.result.recv_timeout(Duration::from_secs(2)).unwrap(),
        Event::Loaded(Ok(_))
    ));
    let mut latest = Appearance::default();
    for size in 12..=24 {
        latest.application_text = size as f32;
        store.save(Preferences {
            ui: latest,
            ..Default::default()
        });
    }
    drop(store);
    assert_eq!(load(&path).unwrap().0.ui, latest);
}

#[test]
fn application_preferences_and_workspace_deletion_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ui.json");
    let mut doc = json!({"version":1,"application":{"future":true}});
    let mut value = Preferences::default();
    value.application.automatic = false;
    value.application.gpu_mib = 2048;
    value.application.ram_mib = 1536;
    value.application.cache_folder = directory
        .path()
        .join("media")
        .to_string_lossy()
        .into_owned();
    value.recent.push(directory.path().join("project.fold"));
    value.workspaces.insert(
        "Editing".into(),
        serde_json::to_value(fold_platform::workspace::Workspace::default()).unwrap(),
    );
    save(&path, &mut doc, value.clone()).unwrap();
    let (loaded, _) = load(&path).unwrap();
    assert_eq!(loaded.application, value.application);
    assert_eq!(loaded.recent, value.recent);
    assert_eq!(loaded.workspaces, value.workspaces);
    value.workspaces.clear();
    save(&path, &mut doc, value).unwrap();
    assert!(load(&path).unwrap().0.workspaces.is_empty());
    assert_eq!(load(&path).unwrap().1["application"]["future"], true);
}
