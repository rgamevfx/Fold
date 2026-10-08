use fold_platform::application::Application;
#[test]
fn machine_preferences_validate_and_configure_real_media_limits() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = Application {
        automatic: false,
        ram_mib: 384,
        cache_folder: dir.path().join("cache").to_string_lossy().into_owned(),
        ..Default::default()
    };
    app.configure_media().unwrap();
    assert!(app.cache_path().is_dir());
    let decoded = fold_media::budget::decoded_usage();
    let working = fold_media::budget::working_usage();
    assert_eq!(decoded.budget + working.budget, 384 * 1024 * 1024);
    let lease = fold_media::budget::reserve_working(working.budget).unwrap();
    assert!(fold_media::budget::reserve_working(1).is_err());
    drop(lease);
    assert!(fold_media::budget::reserve_working(1).is_ok());
    app.viewer_mib = app.gpu_mib;
    assert!(app.validate().is_err());
    app = Application {
        cache_folder: "relative/path".into(),
        ..Default::default()
    };
    assert!(app.validate().is_err());
    app = Application {
        automatic: true,
        ram_mib: 384,
        ..Default::default()
    };
    assert_eq!(app.effective().ram_mib, 768);
}

#[test]
fn old_preferences_and_independent_large_cache_limits() {
    let old: Application = serde_json::from_str(
        r#"{"automatic":false,"gpu_mib":4096,"viewer_mib":1024,"ram_mib":768}"#,
    )
    .unwrap();
    assert_eq!(old.viewer_ram_mib, 1024);
    assert_eq!(old.timeline_disk_gib, 0);
    let mut settings = Application {
        viewer_ram_mib: 32768,
        timeline_disk_gib: 20,
        composition_disk_gib: 100,
        ..Default::default()
    };
    settings.validate().unwrap();
    let active = settings.effective();
    assert_eq!(active.viewer_ram_mib, 32768);
    assert_eq!(active.timeline_disk_gib, 20);
    assert_ne!(active.playback_path(true), active.playback_path(false));
    settings.timeline_cache_folder = "relative".into();
    assert!(settings.validate().is_err());
}
