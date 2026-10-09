#[test]
#[ignore = "requires the packaged OCIO runtime; set FOLD_COLOR_ROOT"]
fn headless_cli_checkpoint_and_rerender() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("demo.fold");
    let image = dir.path().join("demo.ppm");
    let again = dir.path().join("again.ppm");
    let invoke = |command: &str, output: &std::path::Path| {
        std::process::Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .arg(command)
            .arg(&project)
            .arg(output)
            .output()
            .unwrap()
    };
    let result = invoke("checkpoint", &image);
    assert!(result.status.success(), "{:?}", result);
    assert!(invoke("render", &again).status.success());
    let bytes = std::fs::read(&image).unwrap();
    assert_eq!(bytes, std::fs::read(&again).unwrap());
    let header = b"P6\n64 64\n255\n";
    assert!(bytes.starts_with(header));
    assert_eq!(bytes.len(), header.len() + 64 * 64 * 3);
    assert!(
        bytes[header.len()..]
            .chunks_exact(3)
            .all(|p| p == [0, 176, 213])
    );
    assert!(!invoke("render", &image).status.success());
    assert_eq!(bytes, std::fs::read(&image).unwrap());
    assert!(!invoke("checkpoint", &again).status.success());
}
