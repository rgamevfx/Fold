#![cfg(feature = "gpu")]
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
};

#[test]
#[ignore = "requires native Vulkan and the private GPU OCIO package"]
fn native_ocio_display_luts_alpha_and_output_identity() {
    let root = std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("FOLD_TEST_COLOR_ROOT"),
    );
    let runtime = fold_color::Runtime::at(&root).unwrap();
    let config = runtime.bundled().unwrap();
    let (host, _) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for config in [&config] {
        for rgba in [
            [-0.2, 2., 0.3, 1.],
            [0.09, 0.09, 0.09, 0.5],
            [0.; 4],
            [0.18, 0.18, 0.18, 1.],
        ] {
            let plan = RenderGraph {
                width: 19,
                height: 7,
                nodes: vec![ImageOp::Solid { rgba }],
                output: 0,
            };
            let expected =
                fold_render::render_aces_with(plan.clone(), config, &mut decoder, &cancel)
                    .unwrap()
                    .over_black();
            let scene = renderer
                .evaluate(plan, Some(config), &mut decoder, &cancel)
                .unwrap();
            for view in [
                fold_color::DisplayTransform::default(),
                fold_color::DisplayTransform {
                    display: "Rec.1886 Rec.709 - Display".into(),
                    view: "ACES 1.0 - SDR Video".into(),
                    look: None,
                },
            ] {
                let p = config.display(fold_color::WORKING_SPACE, &view).unwrap();
                let cpu = expected.to_output(&p).unwrap();
                let mut gpu = renderer.output(&scene, Some(&p), &cancel).unwrap();
                assert_eq!(gpu.statistics.readback_bytes, 0);
                assert_eq!(gpu.owner(), host.id());
                let actual = gpu.readback(&cancel).unwrap();
                assert_eq!(actual.transform_identity(), cpu.transform_identity());
                for (a, b) in actual.rgba().iter().zip(cpu.rgba()) {
                    assert!(a.abs_diff(*b) <= 1, "display {a}!={b}");
                }
                assert!(gpu.gpu_nanoseconds().unwrap().unwrap() > 0.);
            }
            assert!(renderer.output(&scene, None, &cancel).is_err());
        }
    }
    // External 1D and 3D LUTs use the same resource closure/identity as CPU.
    let directory = tempfile::tempdir().unwrap();
    let mut cube = String::from("LUT_3D_SIZE 3\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n");
    for b in 0..3 {
        for g in 0..3 {
            for r in 0..3 {
                cube += &format!(
                    "{} {} {}\n",
                    r as f32 / 2. * (0.8 + 0.1 * g as f32),
                    g as f32 / 2. * (0.7 + 0.15 * b as f32),
                    b as f32 / 2. * (0.6 + 0.2 * r as f32)
                );
            }
        }
    }
    std::fs::write(directory.path().join("test.cube"), cube).unwrap();
    std::fs::write(
        directory.path().join("test.spi1d"),
        "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 3\n{\n0 0 0\n0.5 0.75 1\n}\n",
    )
    .unwrap();
    for (file, interpolation) in [
        ("test.spi1d", "linear"),
        ("test.cube", "linear"),
        ("test.cube", "tetrahedral"),
    ] {
        let mut text = String::from(
            "ocio_profile_version: 2\nsearch_path: .\nroles:\n  default: ACEScg\n  scene_linear: ACEScg\nfile_rules:\n  - !<Rule> {name: Default, colorspace: ACEScg}\ndisplays:\n  Test:\n    - !<View> {name: Raw, colorspace: output}\ncolorspaces:\n",
        );
        for name in [
            "ACEScg",
            fold_color::settings::SRGB_INPUT,
            fold_color::settings::VIDEO_INPUT,
            "output",
        ] {
            text += &format!(
                "  - !<ColorSpace>\n    name: {name}\n    family: test\n    bitdepth: 32f\n    isdata: false\n    allocation: uniform\n"
            );
            if name == "output" {
                text += &format!(
                    "    from_scene_reference: !<FileTransform> {{src: {file}, interpolation: {interpolation}}}\n"
                );
            }
        }
        let path = directory.path().join("config.ocio");
        std::fs::write(&path, text).unwrap();
        let external = runtime.external(&path).unwrap();
        let p = external.conversion("ACEScg", "output").unwrap();
        assert!(!p.gpu_shader().unwrap().textures.is_empty());
        for i in 0..9 {
            let rgba = [i as f32 / 8., 0.375, 0.8125, 1.];
            let plan = RenderGraph {
                width: 3,
                height: 2,
                nodes: vec![ImageOp::Solid { rgba }],
                output: 0,
            };
            let cpu = fold_render::render_aces_with(plan.clone(), &external, &mut decoder, &cancel)
                .unwrap()
                .to_output(&p)
                .unwrap();
            let scene = renderer
                .evaluate(plan, Some(&external), &mut decoder, &cancel)
                .unwrap();
            let gpu = renderer
                .output(&scene, Some(&p), &cancel)
                .unwrap()
                .readback(&cancel)
                .unwrap();
            for (a, b) in gpu.rgba().iter().zip(cpu.rgba()) {
                assert!(a.abs_diff(*b) <= 1, "{file}: {a}!={b}");
            }
            assert!(
                renderer
                    .output(
                        &scene,
                        Some(&config.display("ACEScg", &Default::default()).unwrap()),
                        &cancel
                    )
                    .is_err()
            );
        }
    }
    assert!(host.memory().peak <= host.memory().budget);
}

#[test]
#[ignore = "requires Vulkan, FFmpeg and FOLD_TEST_COLOR_ROOT"]
fn prores_xq_alpha_matches_cpu_through_aces_and_over() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alpha.mov");
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x32:rate=24",
            "-vf",
            "format=yuva444p10le,geq=lum='p(X,Y)':cb='p(X,Y)':cr='p(X,Y)':a='X/W*1023'",
            "-frames:v",
            "2",
            "-c:v",
            "prores_ks",
            "-profile:v",
            "5",
            "-pix_fmt",
            "yuva444p10le",
            "-threads",
            "1",
            "-filter_threads",
            "1",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cancel = fold_media::Cancel::default();
    let source = fold_media::inspect(&path, &cancel).unwrap();
    let runtime = fold_color::Runtime::at(&std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").unwrap(),
    ))
    .unwrap();
    let config = runtime.bundled().unwrap();
    let graph = RenderGraph {
        width: 64,
        height: 32,
        nodes: vec![
            ImageOp::Video {
                source,
                time: fold_foundation::Time::ZERO,
            },
            ImageOp::Solid {
                rgba: [0.05, 0.1, 0.2, 1.],
            },
            ImageOp::Over {
                foreground: 0,
                background: 1,
            },
        ],
        output: 2,
    };
    // Independent provider fragments can read the same video. Keep a later
    // consumer alive across intervening operations, including opacity fusion.
    let mut duplicated = graph.clone();
    duplicated.nodes.push(duplicated.nodes[0].clone());
    duplicated.nodes.push(ImageOp::Opacity {
        input: 3,
        opacity: 0.5,
    });
    duplicated.nodes.push(ImageOp::Over {
        foreground: 4,
        background: 2,
    });
    duplicated.output = 5;
    let mut cpu_decoder = fold_media::Decoder::default();
    let expected =
        fold_render::render_aces_with(graph.clone(), &config, &mut cpu_decoder, &cancel).unwrap();
    let (host, _) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    // Software-only codecs must work in a project configured for native NVDEC.
    let mut decoder = fold_media::Decoder::with_backend(fold_media::DecodeBackend::CudaNative);
    let mut gpu = renderer
        .evaluate(graph, Some(&config), &mut decoder, &cancel)
        .unwrap();
    let actual = gpu.readback(&cancel).unwrap();
    let error = actual
        .pixels()
        .iter()
        .zip(expected.pixels())
        .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
        .fold(0f32, f32::max);
    assert!(error < 0.0001, "ACES composite error {error}");
    assert!(actual.pixels().iter().all(|p| p[3] == 1.));
    for [width, height] in [[64, 32], [32, 16]] {
        duplicated.width = width;
        duplicated.height = height;
        let expected =
            fold_render::render_aces_with(duplicated.clone(), &config, &mut cpu_decoder, &cancel)
                .unwrap();
        let mut gpu = renderer
            .evaluate(duplicated.clone(), Some(&config), &mut decoder, &cancel)
            .unwrap();
        assert_eq!(
            gpu.statistics.native_video_nodes, 1,
            "duplicate source must not reconstruct/color-convert twice"
        );
        let actual = gpu.readback(&cancel).unwrap();
        let error = actual
            .pixels()
            .iter()
            .zip(expected.pixels())
            .flat_map(|(a, b)| (0..4).map(move |i| (a[i] - b[i]).abs()))
            .fold(0f32, f32::max);
        assert!(error < 0.0001, "shared alpha source error {error}");
    }
}

#[test]
#[ignore = "requires native Vulkan and the private GPU OCIO package"]
fn raster_alpha_srgb_cpu_gpu_and_retained_source_agree() {
    let dir = tempfile::tempdir().unwrap();
    let raw = dir.path().join("source.rgba");
    let png = dir.path().join("source.png");
    std::fs::write(&raw, [128u8, 64, 32, 128].repeat(4)).unwrap();
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "2x2",
            "-i",
        ])
        .arg(&raw)
        .args(["-frames:v", "1", "-threads", "1"])
        .arg(&png)
        .output()
        .unwrap();
    assert!(result.status.success());
    let cancel = fold_media::Cancel::default();
    let source = fold_media::ingest::inspect_linked(&png, &cancel).unwrap();
    let (image, _) = fold_media::still::load(&png, &source.fingerprint, &cancel).unwrap();
    let graph = RenderGraph {
        width: 2,
        height: 2,
        nodes: vec![ImageOp::Media(image)],
        output: 0,
    };
    let root = std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("FOLD_TEST_COLOR_ROOT"),
    );
    let runtime = fold_color::Runtime::at(&root).unwrap();
    let config = runtime.bundled().unwrap();
    let (host, _) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    for color in [None, Some(&config)] {
        let expected = match color {
            Some(config) => {
                fold_render::render_aces_with(graph.clone(), config, &mut decoder, &cancel).unwrap()
            }
            None => fold_render::render(graph.clone()).unwrap(),
        };
        if color.is_none() {
            let alpha = 128. / 255.;
            let linear = ((128f32 / 255. + 0.055) / 1.055).powf(2.4) * alpha;
            assert!((expected.pixels()[0][0] - linear).abs() < 1e-6);
            assert!((expected.pixels()[0][3] - alpha).abs() < 1e-6);
        }
        for _ in 0..2 {
            let actual = renderer
                .evaluate(graph.clone(), color, &mut decoder, &cancel)
                .unwrap()
                .readback(&cancel)
                .unwrap();
            for (a, b) in actual.pixels().iter().zip(expected.pixels()) {
                for c in 0..4 {
                    assert!((a[c] - b[c]).abs() < 0.0001, "channel {c}: {a:?} != {b:?}");
                }
            }
        }
    }
}
