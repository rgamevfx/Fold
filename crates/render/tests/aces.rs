use fold_render::{ImageOp, RenderGraph, WorkingSpace};

#[test]
#[ignore = "requires the privately built OCIO package; set FOLD_TEST_COLOR_ROOT"]
fn aces_working_values_input_and_explicit_output() {
    let runtime = fold_color::Runtime::at(&std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("set FOLD_TEST_COLOR_ROOT"),
    ))
    .unwrap();
    let config = runtime.bundled().unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let render = |plan, decoder: &mut fold_media::Decoder| {
        fold_render::render_aces_with(plan, &config, decoder, &cancel).unwrap()
    };
    let graph = RenderGraph {
        width: 1,
        height: 1,
        nodes: vec![
            ImageOp::Solid {
                rgba: [-0.25, 2., 1., 0.5],
            },
            ImageOp::Grade {
                input: 0,
                gain: [2.; 3],
            },
            ImageOp::Blur {
                input: 1,
                radius: 0,
            },
        ],
        output: 2,
    };
    let frame = render(graph.clone(), &mut decoder);
    assert_eq!(frame.working_space(), WorkingSpace::AcesCg);
    assert_eq!(
        frame.config_identity(),
        Some(config.identity().content_sha256.as_str())
    );
    assert_eq!(frame.pixels(), &[[-0.5, 4., 2., 0.5]]);
    assert!(
        fold_render::render(graph).is_err(),
        "legacy interpretation stays SDR"
    );
    assert!(frame.to_display().is_err());
    assert!(frame.write_ppm(Vec::new()).is_err());
    let display = config
        .display(fold_color::WORKING_SPACE, &Default::default())
        .unwrap();
    assert!(frame.to_output(&display).is_err(), "no implicit alpha loss");
    let scene = frame.over_black();
    let preview = scene.to_output(&display).unwrap();
    assert_eq!(preview.transform_identity(), display.identity());
    let delivery = scene
        .to_output(
            &config
                .display(fold_color::WORKING_SPACE, &Default::default())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        preview.rgba(),
        delivery.rgba(),
        "matched explicit pre-encode settings agree exactly"
    );
    assert_eq!(
        scene.pixels(),
        &[[-0.5, 4., 2., 1.]],
        "output never mutates working pixels"
    );
    assert!(
        scene
            .to_output(
                &config
                    .display(fold_color::LINEAR_SRGB, &Default::default())
                    .unwrap()
            )
            .is_err()
    );
    let media = fold_media::RgbImage::from_rgb([1, 1], vec![255, 0, 0]).unwrap();
    let frame = render(
        RenderGraph {
            width: 1,
            height: 1,
            nodes: vec![ImageOp::Media(media)],
            output: 0,
        },
        &mut decoder,
    );
    let expected = [0.6131122, 0.070195414, 0.02061609, 1.];
    for (a, b) in frame.pixels()[0].iter().zip(expected) {
        assert!((a - b).abs() < 0.00001, "{:?}", frame.pixels());
    }
    let overflow = RenderGraph {
        width: 1,
        height: 1,
        nodes: vec![
            ImageOp::Solid {
                rgba: [f32::MAX, 0., 0., 1.],
            },
            ImageOp::Grade {
                input: 0,
                gain: [16.; 3],
            },
        ],
        output: 1,
    };
    assert!(fold_render::render_aces_with(overflow, &config, &mut decoder, &cancel).is_err());
}
