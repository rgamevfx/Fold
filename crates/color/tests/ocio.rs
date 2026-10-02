use fold_color::{DisplayTransform, Runtime, WORKING_SPACE};
use std::{fs, path::PathBuf};

fn runtime() -> Runtime {
    Runtime::at(&PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT")
            .expect("set FOLD_TEST_COLOR_ROOT to the built package"),
    ))
    .unwrap()
}

#[test]
fn missing_runtime_is_not_a_silent_fallback() {
    let directory = tempfile::tempdir().unwrap();
    assert!(Runtime::at(directory.path()).is_err());
    assert!(Runtime::at(std::path::Path::new("relative")).is_err());
    assert!(fold_color::validate_pixel([-1., 3., 0., 0.5]).is_ok());
    assert!(fold_color::validate_pixel([f32::NAN, 0., 0., 1.]).is_err());
    assert!(fold_color::validate_pixel([0., 0., 0., 1.01]).is_err());
}

#[test]
#[ignore = "requires the privately built OCIO package; set FOLD_TEST_COLOR_ROOT"]
fn bundled_cpu_reference_hdr_alpha_and_catalog() {
    let runtime = runtime();
    let config = runtime.bundled().unwrap();
    assert_eq!(
        config.identity().resources["config.ocio"],
        fold_color::BUNDLED_SHA256
    );
    let catalog = config.catalog().unwrap();
    assert!(catalog.spaces.iter().any(|s| s == WORKING_SPACE));
    assert!(catalog.displays["sRGB - Display"].contains(&"ACES 1.0 - SDR Video".into()));
    let identity = config.conversion(WORKING_SPACE, WORKING_SPACE).unwrap();
    let source = [
        [-0.25, 2., 8., 1.],
        [0.09, 0.18, 0.36, 0.5],
        [3., -1., 5., 0.],
    ];
    let mut pixels = source;
    identity.apply(&mut pixels).unwrap();
    assert_eq!(pixels[0], source[0]);
    assert_eq!(pixels[1], source[1]);
    assert_eq!(pixels[2], [0.; 4]);
    let display = config
        .display(WORKING_SPACE, &DisplayTransform::default())
        .unwrap();
    let mut reference = [[0.18, 0.18, 0.18, 1.], [0.09, 0.09, 0.09, 0.5], [0.; 4]];
    display.apply(&mut reference).unwrap();
    // OCIO 2.4.2 / ACES 1.3 Studio 2.2.0 CPU reference, independent Python spike.
    for c in 0..3 {
        assert!(
            (reference[0][c] - 0.35595).abs() < 0.0002,
            "{:?}",
            reference[0]
        );
        assert!((reference[1][c] - reference[0][c] * 0.5).abs() < 0.00001);
    }
    assert_eq!(reference[2], [0.; 4]);
    assert_eq!(reference[1][3], 0.5);
    assert!(config.conversion("not a space", WORKING_SPACE).is_err());
    assert!(
        config
            .display(
                WORKING_SPACE,
                &DisplayTransform {
                    view: "not a view".into(),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(display.apply(&mut [[f32::INFINITY, 0., 0., 1.]]).is_err());
    // A processor retains its native library and pinned resources after config/runtime drop.
    drop(config);
    drop(runtime);
    identity.apply(&mut pixels).unwrap();
}

const EXTERNAL: &str = "ocio_profile_version: 2\nsearch_path: .\nroles:\n  default: linear\n  scene_linear: linear\nfile_rules:\n  - !<Rule> {name: Default, colorspace: linear}\ndisplays:\n  Test:\n    - !<View> {name: Raw, colorspace: output}\ncolorspaces:\n  - !<ColorSpace>\n    name: linear\n    family: test\n    bitdepth: 32f\n    isdata: false\n    allocation: uniform\n  - !<ColorSpace>\n    name: output\n    family: test\n    bitdepth: 32f\n    isdata: false\n    allocation: uniform\n    from_scene_reference: !<FileTransform> {src: test.spi1d, interpolation: linear}\n";

#[test]
#[ignore = "requires the privately built OCIO package; set FOLD_TEST_COLOR_ROOT"]
fn external_resources_are_pinned_content_identified_and_fail_closed() {
    let runtime = runtime();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.ocio");
    let lut = root.path().join("test.spi1d");
    fs::write(&path, EXTERNAL).unwrap();
    fs::write(
        &lut,
        "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 3\n{\n0 0 0\n0.5 1 2\n}\n",
    )
    .unwrap();
    let config = runtime.external(&path).unwrap();
    let first = config.conversion("linear", "output").unwrap();
    let mut pixel = [[0.5, 0.5, 0.5, 1.]];
    first.apply(&mut pixel).unwrap();
    assert_eq!(pixel, [[0.25, 0.5, 1., 1.]]);
    fs::write(
        &lut,
        "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 3\n{\n0 0 0\n1 1 1\n}\n",
    )
    .unwrap();
    let changed = runtime.external(&path).unwrap();
    assert_ne!(config.identity(), changed.identity());
    assert_ne!(
        first.identity(),
        changed.conversion("linear", "output").unwrap().identity()
    );
    let relocated = tempfile::tempdir().unwrap();
    fs::copy(&path, relocated.path().join("config.ocio")).unwrap();
    fs::copy(&lut, relocated.path().join("test.spi1d")).unwrap();
    assert_eq!(
        changed.identity(),
        runtime
            .external(&relocated.path().join("config.ocio"))
            .unwrap()
            .identity()
    );
    fs::remove_file(&lut).unwrap();
    let missing = runtime.external(&path).unwrap();
    assert!(missing.conversion("linear", "output").is_err());
    let mut pixel = [[0.5, 0.5, 0.5, 1.]];
    config
        .conversion("linear", "output")
        .unwrap()
        .apply(&mut pixel)
        .unwrap();
    assert_eq!(
        pixel,
        [[0.25, 0.5, 1., 1.]],
        "original package remains pinned even for a new processor"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, root.path().join("link.ocio")).unwrap();
        assert!(runtime.external(&path).is_err());
    }
}
