use fold_media::Cancel;
use fold_render::{ImageOp, RenderGraph};

#[test]
#[ignore = "allocates full 8K frames and exports a 512 MiB EXR"]
fn native_eight_k_preview_and_export_keep_source_resolution() {
    let count = 8192usize * 8192;
    let mut bytes = [128u8, 64, 32].repeat(count);
    bytes[(count - 1) * 3..].copy_from_slice(&[255, 0, 0]);
    let image = fold_media::RgbImage::from_rgb([8192, 8192], bytes).unwrap();
    let graph = RenderGraph {
        width: 8192,
        height: 8192,
        nodes: vec![ImageOp::Media(image.clone())],
        output: 0,
    };
    let native = fold_render::render(graph.clone()).unwrap();
    assert_eq!(native.dimensions(), [8192, 8192]);
    assert_eq!(*native.pixels().last().unwrap(), [1., 0., 0., 1.]);
    // Preview demand is transient; it never mutates the source or authored graph.
    let preview = fold_render::render(RenderGraph {
        width: 128,
        height: 128,
        ..graph.clone()
    })
    .unwrap();
    assert_eq!(preview.dimensions(), [128, 128]);
    assert_eq!(image.dimensions(), [8192, 8192]);
    assert_eq!([graph.width, graph.height], [8192, 8192]);
    assert!((preview.pixels()[0][0] - native.pixels()[0][0]).abs() < 1e-6);
    drop(preview);
    // Full-size display conversion must not encounter the former 64 MiB cap.
    assert_eq!(native.to_display().unwrap().rgba().len(), count * 4);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native.exr");
    fold_media::exr::write::write(
        &path,
        [8192, 8192],
        native.pixels(),
        &[0, 1, 2, 3],
        fold_media::exr::write::Precision::Half,
        fold_media::exr::write::Compression::None,
        &Cancel::default(),
    )
    .unwrap();
    drop(native);
    assert!(std::fs::metadata(&path).unwrap().len() > 256 * 1024 * 1024);
    let info = fold_media::exr::inspect(&path, &Cancel::default()).unwrap();
    assert_eq!(info.dimensions, [8192, 8192]);
    let source = fold_media::exr::Source {
        info,
        path: path.clone(),
        fingerprint: fold_media::fingerprint(&path, &Cancel::default()).unwrap(),
    };
    let roundtrip = fold_media::exr::decode(
        &source,
        [Some("R"), Some("G"), Some("B"), Some("A")],
        [8192, 8192],
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(*roundtrip.pixels.last().unwrap(), [1., 0., 0., 1.]);
    eprintln!(
        "8K source, 128-square preview and native EXR export/reopen passed; working peak={} MiB",
        fold_media::budget::working_usage().peak / 1048576
    );
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan and at least 4 GiB GPU allowance"]
fn native_gpu_eight_k_upload_preview_and_readback() {
    use fold_render::gpu::{Host, Renderer};
    let image =
        fold_media::RgbImage::from_rgb([8192, 8192], [128u8, 64, 32].repeat(8192 * 8192)).unwrap();
    let graph = RenderGraph {
        width: 8192,
        height: 8192,
        nodes: vec![ImageOp::Media(image.clone())],
        output: 0,
    };
    let cancel = Cancel::default();
    let (host, adapter) = pollster::block_on(Host::headless(4 * 1024 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let mut scene = renderer
        .evaluate(graph.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(scene.dimensions(), [8192, 8192]);
    let result = scene.readback(&cancel).unwrap();
    let expected = image.linear_pixels().next().unwrap();
    for index in [0, 4096 * 8192 + 4096, 8192 * 8192 - 1] {
        for c in 0..4 {
            assert!((result.pixels()[index][c] - expected[c]).abs() < 1e-6);
        }
    }
    drop(result);
    drop(scene);
    let mut preview = renderer
        .evaluate(
            RenderGraph {
                width: 128,
                height: 128,
                ..graph
            },
            None,
            &mut decoder,
            &cancel,
        )
        .unwrap();
    assert_eq!(preview.dimensions(), [128, 128]);
    let result = preview.readback(&cancel).unwrap();
    for c in 0..4 {
        assert!((result.pixels()[0][c] - expected[c]).abs() < 0.0001);
    }
    assert_eq!(image.dimensions(), [8192, 8192]);
    assert!(host.memory().peak <= host.memory().budget);
    eprintln!(
        "GPU {}: 8K source/readback and reduced preview passed; peak={} MiB",
        adapter.name,
        host.memory().peak / 1048576
    );
}
