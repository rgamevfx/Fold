//! Image-quality regressions: compare reduced output to integrated native pixels.
#[cfg(feature = "gpu")]
static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_render::{ImageOp, RenderGraph};
fn video() -> (tempfile::TempDir, fold_media::VideoSource) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stripes.mp4");
    let info = fold_media::VideoInfo {
        width: 64,
        height: 32,
        frames: 1,
        rate: [24, 1],
    };
    let cancel = fold_media::Cancel::default();
    let mut encoder = fold_media::Encoder::new(&path, &info, 1, cancel.clone()).unwrap();
    let pixels: Vec<u8> = (0..32)
        .flat_map(|_| (0..64).flat_map(|x| [if x % 2 == 0 { 0 } else { 255 }; 3]))
        .collect();
    encoder.write_rgb(&pixels).unwrap();
    encoder.finish().unwrap();
    let source = fold_media::inspect(&path, &cancel).unwrap();
    (dir, source)
}
fn graph(source: &fold_media::VideoSource, dimensions: [u32; 2]) -> RenderGraph {
    RenderGraph {
        width: dimensions[0],
        height: dimensions[1],
        nodes: vec![ImageOp::Video {
            source: source.clone(),
            time: fold_foundation::Time::ZERO,
        }],
        output: 0,
    }
}
#[test]
#[ignore = "requires FFmpeg"]
fn downsampling_preserves_stripe_energy_in_linear_light() {
    let (_dir, source) = video();
    let full = fold_render::render(graph(&source, [64, 32])).unwrap();
    let small = fold_render::render(graph(&source, [32, 16])).unwrap();
    for y in 0..16 {
        for x in 0..32 {
            for c in 0..4 {
                let expected = (0..2)
                    .flat_map(|dy| (0..2).map(move |dx| (2 * y + dy) * 64 + 2 * x + dx))
                    .map(|i| full.pixels()[i][c])
                    .sum::<f32>()
                    / 4.;
                assert!(
                    (small.pixels()[y * 32 + x][c] - expected).abs() < 0.00002,
                    "aliasing at {x},{y}: {} instead of area average {expected}",
                    small.pixels()[y * 32 + x][c]
                );
            }
        }
    }
}

fn stripes() -> ImageOp {
    ImageOp::Media(
        fold_media::RgbImage::from_rgb(
            [64, 32],
            (0..32)
                .flat_map(|y| {
                    (0..64).flat_map(move |x| [if (x + y) % 2 == 0 { 0 } else { 255 }; 3])
                })
                .collect(),
        )
        .unwrap(),
    )
}
#[test]
fn still_reconstruction_antialiases_and_keeps_native_pixels_exact() {
    for dimensions in [[32, 16], [1, 1]] {
        let frame = fold_render::render(RenderGraph {
            width: dimensions[0],
            height: dimensions[1],
            nodes: vec![stripes()],
            output: 0,
        })
        .unwrap();
        for pixel in frame.pixels() {
            assert!((pixel[0] - 0.5).abs() < 1e-6);
            assert_eq!(pixel[3], 1.);
        }
    }
    let native = fold_render::render(RenderGraph {
        width: 64,
        height: 32,
        nodes: vec![stripes()],
        output: 0,
    })
    .unwrap();
    assert_eq!(native.pixels()[0], [0., 0., 0., 1.]);
    assert_eq!(native.pixels()[1], [1.; 4]);
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires Vulkan"]
fn gpu_reconstruction_matches_cpu_for_odd_sizes_enlargement_and_regions() {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap();
    use fold_render::{
        gpu::{Host, Renderer},
        region::Region,
    };
    let (host, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for dimensions in [[32, 16], [1, 1], [31, 17], [127, 65], [21, 61], [64, 32]] {
        let graph = RenderGraph {
            width: dimensions[0],
            height: dimensions[1],
            nodes: vec![stripes()],
            output: 0,
        };
        for region in [
            Region::full(dimensions),
            Region {
                x: dimensions[0] / 3,
                y: dimensions[1] / 3,
                width: (dimensions[0] / 3).max(1),
                height: (dimensions[1] / 3).max(1),
            },
        ] {
            let expected =
                fold_render::region::crop(fold_render::render(graph.clone()).unwrap(), region)
                    .unwrap();
            let mut result = renderer
                .evaluate_region(graph.clone(), region, None, &mut decoder, &cancel, None)
                .unwrap();
            assert_eq!(result.statistics.readback_bytes, 0);
            let actual = result.readback(&cancel).unwrap();
            assert_eq!(actual.dimensions(), region.dimensions());
            let error = actual
                .pixels()
                .iter()
                .flatten()
                .zip(expected.pixels().iter().flatten())
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(error < 0.00002, "{dimensions:?} {region:?}: error {error}");
        }
    }
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires Vulkan, FFmpeg and FOLD_TEST_COLOR_ROOT"]
fn aces_video_filtering_precedes_display_and_matches_cpu() {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap();
    let root = std::path::PathBuf::from(
        std::env::var_os("FOLD_TEST_COLOR_ROOT").expect("set FOLD_TEST_COLOR_ROOT"),
    );
    let config = fold_color::Runtime::at(&root).unwrap().bundled().unwrap();
    let (_dir, source) = video();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let plan = graph(&source, [31, 17]);
    let region = fold_render::region::Region {
        x: 5,
        y: 3,
        width: 11,
        height: 7,
    };
    let full = fold_render::render_aces_with(plan.clone(), &config, &mut decoder, &cancel).unwrap();
    let expected = fold_render::region::crop(full, region).unwrap();
    let (host, _) =
        pollster::block_on(fold_render::gpu::Host::headless(128 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut result = renderer
        .evaluate_region(plan, region, Some(&config), &mut decoder, &cancel, None)
        .unwrap();
    let actual = result.readback(&cancel).unwrap();
    assert_eq!(actual.config_identity(), expected.config_identity());
    let error = actual
        .pixels()
        .iter()
        .flatten()
        .zip(expected.pixels().iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    assert!(error < 0.0001, "working color resize error {error}");
}
