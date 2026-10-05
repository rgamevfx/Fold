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
