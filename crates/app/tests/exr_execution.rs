//! Physical source preparation, sharing, staging lifetime and invalidation.
use exr::prelude::*;
use fold_media::{Cancel, exr as media};
use fold_render::{ImageOp, RenderGraph};

fn fixture(path: &std::path::Path, value: f32) -> RenderGraph {
    sized_fixture(path, value, [17, 9])
}
fn sized_fixture(path: &std::path::Path, value: f32, dimensions: [u32; 2]) -> RenderGraph {
    Image::from_channels(
        (dimensions[0] as usize, dimensions[1] as usize),
        SpecificChannels::rgba(|Vec2(x, y)| {
            (value + x as f32 / 128., y as f32 / 64., 0.125f32, 0.5f32)
        }),
    )
    .write()
    .non_parallel()
    .to_file(path)
    .unwrap();
    let cancel = Cancel::default();
    let source = media::Source {
        info: media::inspect(path, &cancel).unwrap(),
        fingerprint: fold_media::fingerprint(path, &cancel).unwrap(),
        path: path.into(),
    };
    RenderGraph {
        width: dimensions[0],
        height: dimensions[1],
        output: 0,
        nodes: vec![ImageOp::Exr {
            alpha: Default::default(),
            source,
            channels: ["R", "G", "B", "A"].map(|c| Some(c.into())),
            space: None,
        }],
    }
}
#[test]
fn preparation_retains_shared_samples_and_skips_unreachable_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut graph = fixture(&dir.path().join("source.exr"), 0.125);
    let unused = fixture(&dir.path().join("unused.exr"), 0.25);
    graph.nodes.extend(unused.nodes);
    std::fs::remove_file(dir.path().join("unused.exr")).unwrap();
    let cancel = Cancel::default();
    let held = fold_render::prepare_sources(&graph, &cancel).unwrap();
    let ImageOp::Exr {
        source, channels, ..
    } = &graph.nodes[0]
    else {
        panic!()
    };
    let request = media::cache::Request {
        source: source.clone(),
        channels: channels.clone(),
        dimensions: [17, 9],
    };
    let actual = media::cache::request(request.clone(), true, &cancel)
        .unwrap()
        .wait(&cancel)
        .unwrap();
    media::cache::clear(); // active preparation retains its identity as well as memory
    let again = media::cache::request(request, true, &cancel)
        .unwrap()
        .wait(&cancel)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&actual, &again));
    let usage = fold_media::budget::preparation_usage();
    let pressure = fold_media::budget::reserve_preparation(usage.budget - usage.bytes).unwrap();
    assert!(fold_render::prepare_sources(&graph, &cancel).is_err());
    // Foreground execution still has access to the prepared decoded samples.
    assert!(fold_render::render(graph.clone()).is_ok());
    drop(pressure);
    drop(held);
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(fold_render::prepare_sources(&graph, &cancelled).is_err());
    graph.nodes[0] = ImageOp::Opacity {
        input: 1,
        opacity: 0.5,
    };
    assert!(fold_render::prepare_sources(&graph, &cancel).is_err());
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn shared_gpu_inputs_and_reused_uploads_preserve_held_frames_and_source_changes() {
    use fold_render::gpu::{Host, Renderer};
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    let (host, _) = pollster::block_on(Host::headless(16 * 1024 * 1024)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let mut second = Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let first = fixture(&dir.path().join("first.exr"), 0.125);
    let mut held = renderer
        .evaluate(first.clone(), None, &mut decoder, &cancel)
        .unwrap();
    let original = held.readback(&cancel).unwrap();
    let shared = second
        .evaluate(first.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(shared.statistics.resident_still_nodes, 1);
    assert_eq!(shared.statistics.upload_bytes, 0);
    for alpha in [
        fold_render::operations::SourceAlpha::Straight,
        fold_render::operations::SourceAlpha::Ignore,
        fold_render::operations::SourceAlpha::Premultiplied,
    ] {
        let mut graph = first.clone();
        let ImageOp::Exr { alpha: value, .. } = &mut graph.nodes[0] else {
            unreachable!()
        };
        *value = alpha;
        let expected = fold_render::render(graph.clone()).unwrap();
        let actual = renderer
            .evaluate(graph, None, &mut decoder, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
        assert_eq!(actual.pixels(), expected.pixels());
    }
    for index in 0..12 {
        let graph = fixture(&dir.path().join(format!("{index}.exr")), index as f32 / 64.);
        let expected = fold_render::render(graph.clone()).unwrap();
        let mut actual = renderer
            .evaluate(graph, None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(actual.statistics.upload_allocations, 0);
        assert_eq!(actual.statistics.upload_reuses, 1);
        assert_eq!(
            actual.readback(&cancel).unwrap().pixels(),
            expected.pixels()
        );
    }
    assert_eq!(held.readback(&cancel).unwrap().pixels(), original.pixels());
    // Abandon an encoder after its first source upload. Unsubmitted staging
    // must remap safely and must not poison the next frame or leak reservations.
    let fresh = fixture(&dir.path().join("abandoned.exr"), 0.2);
    let mut failed = fresh.clone();
    let mut stale = first.nodes[0].clone();
    if let ImageOp::Exr { source, .. } = &mut stale {
        source.fingerprint = "sha256:stale".into();
    }
    failed.nodes.push(stale);
    failed.nodes.push(ImageOp::Over {
        foreground: 0,
        background: 1,
    });
    failed.output = 2;
    assert!(
        renderer
            .evaluate(failed, None, &mut decoder, &cancel)
            .is_err()
    );
    host.poll().unwrap();
    let mut recovered = renderer
        .evaluate(fresh.clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(
        recovered.readback(&cancel).unwrap().pixels(),
        fold_render::render(fresh).unwrap().pixels()
    );
    // File replacement cannot be hidden by the shared GPU/CPU caches.
    let _replacement = fixture(&dir.path().join("first.exr"), 0.25);
    assert!(
        renderer
            .evaluate(first, None, &mut decoder, &cancel)
            .err()
            .unwrap()
            .message
            .contains("fingerprint")
    );
    assert!(host.memory().allocated <= host.memory().budget);
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native Vulkan"]
fn gpu_source_byte_budget_evicts_without_reusing_held_images() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(4 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host.clone()).unwrap();
    let mut decoder = fold_media::Decoder::default();
    // The 1.33 MiB source budget holds two of these 512 KiB inputs.
    let graphs: Vec<_> = (0..3)
        .map(|i| {
            sized_fixture(
                &dir.path().join(format!("{i}.exr")),
                i as f32 / 16.,
                [256, 128],
            )
        })
        .collect();
    let mut held = renderer
        .evaluate(graphs[0].clone(), None, &mut decoder, &cancel)
        .unwrap();
    let original = held.readback(&cancel).unwrap();
    for graph in &graphs[1..] {
        renderer
            .evaluate(graph.clone(), None, &mut decoder, &cancel)
            .unwrap()
            .readback(&cancel)
            .unwrap();
    }
    let mut evicted = renderer
        .evaluate(graphs[0].clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(evicted.statistics.cpu_adapter_nodes, 1);
    assert_eq!(evicted.statistics.upload_bytes, 256 * 128 * 16);
    assert_eq!(
        evicted.readback(&cancel).unwrap().pixels(),
        original.pixels()
    );
    let warm = renderer
        .evaluate(graphs[0].clone(), None, &mut decoder, &cancel)
        .unwrap();
    assert_eq!(warm.statistics.resident_still_nodes, 1);
    assert_eq!(warm.statistics.upload_bytes, 0);
    assert_eq!(held.readback(&cancel).unwrap().pixels(), original.pixels());
    assert!(host.memory().peak <= host.memory().budget);
}
