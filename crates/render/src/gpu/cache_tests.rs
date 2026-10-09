use super::*;
use crate::gpu::validation::Status;
use crate::{ImageOp, RenderGraph, gpu::Renderer};

#[test]
#[ignore = "requires native Vulkan"]
fn native_bc7_unavailable_features_and_allocation_pressure_leave_host_usable() {
    let instance = Host::create_instance(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    })
    .unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let unsupported = Host::from_device(instance, adapter, device, queue, 1024 * 1024).unwrap();
    assert!(
        PresentationCompressor::new(unsupported.clone())
            .err()
            .unwrap()
            .contains("unavailable")
    );
    unsupported.check().unwrap();
    let (small, _) = pollster::block_on(Host::headless(1)).unwrap();
    assert!(
        PresentationCompressor::new(small.clone())
            .err()
            .unwrap()
            .contains("budget")
    );
    assert_eq!(small.memory().allocated, 0);
    small.check().unwrap();
}

#[test]
#[ignore = "requires native Vulkan BC7 support"]
fn native_bc7_padding_sampling_and_lifetimes() {
    let (host, adapter) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    eprintln!("BC7 adapter: {adapter:?}");
    let start = std::time::Instant::now();
    let mut compressor = PresentationCompressor::new(host.clone()).unwrap();
    eprintln!("BC7 pipeline setup: {:?}", start.elapsed());
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    for [width, height] in [[479, 273], [480, 480], [1, 1], [4, 4], [64, 64]] {
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).flat_map(move |x| {
                    // Gradients, saturated edges, and thin high-contrast title-like strokes.
                    if width == 64 {
                        [128; 3]
                    } else if width == 4 {
                        [
                            (x * 67 + y * 41) as u8,
                            (x * 13 + y * 83) as u8,
                            (x * 101 + y * 31) as u8,
                        ]
                    } else if y > height / 2 && x % 13 < 2 {
                        [255, 255, 255]
                    } else {
                        [
                            (x * 255 / width) as u8,
                            (y * 255 / height) as u8,
                            if x < width / 2 { 32 } else { 240 },
                        ]
                    }
                })
            })
            .collect();
        // Footage replaces the viewport fixtures, not the deliberately constructed
        // tiny-block and alpha-edge witnesses. Report lossy error for diagnostics;
        // playback never runs a quality-comparison or rejection pass.
        let video = std::env::var("FOLD_TEST_VIDEO").ok().filter(|_| width > 64);
        let media = if let Some(path) = &video {
            let source = fold_media::inspect(std::path::Path::new(path), &cancel).unwrap();
            fold_media::Decoder::default()
                .decode(
                    &source,
                    fold_foundation::Time::ZERO,
                    [width, height],
                    &cancel,
                )
                .unwrap()
        } else {
            fold_media::RgbImage::from_rgb([width, height], pixels).unwrap()
        };
        let mut graph = RenderGraph {
            width,
            height,
            nodes: vec![
                ImageOp::Media(media),
                ImageOp::Opacity {
                    input: 0,
                    opacity: 0.8,
                },
            ],
            output: 1,
        };
        if width == 64 {
            graph.nodes.push(ImageOp::Crop {
                input: 1,
                rect: [8, 8, 56, 56],
            });
            graph.nodes.push(ImageOp::Blur {
                input: 2,
                radius: 1,
            });
            graph.output = 3; // Real spatial alpha/coverage edges, matted before BC7.
        }
        let scene = renderer
            .evaluate(graph, None, &mut fold_media::Decoder::default(), &cancel)
            .unwrap();
        let mut source = renderer.output(&scene, None, &cancel).unwrap();
        let expected = source.readback(&cancel).unwrap();
        let start = std::time::Instant::now();
        let compressed = compressor.compress(&source).unwrap();
        while !compressed.is_ready().unwrap() {
            host.poll().unwrap();
            assert!(start.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        eprintln!(
            "BC7 {width}x{height}: wall={:?} GPU={:?} bytes={}",
            start.elapsed(),
            compressed
                .compression
                .as_ref()
                .unwrap()
                .gpu_nanoseconds()
                .ok()
                .flatten(),
            compressed.storage_bytes()
        );
        assert_eq!(compressed.dimensions(), [width, height]);
        assert_eq!(
            compressed.storage_bytes(),
            u64::from(width.div_ceil(4) * height.div_ceil(4) * 16)
        );
        assert_eq!(compressed.transform_identity(), source.transform_identity());
        assert_eq!(
            compressed.statistics.readback_bytes,
            source.statistics.readback_bytes
        );
        let output = host
            .image_format([width, height], wgpu::TextureFormat::Rgba8Unorm)
            .unwrap();
        let group = host.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &compressor.pad.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&compressed.image.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&output.view),
                },
            ],
        });
        let mut encoder = host.device().create_command_encoder(&Default::default());
        let status = Status::new(&host).unwrap();
        status.start(&mut encoder);
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&compressor.pad);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        status.encode(&mut encoder);
        host.submit(
            encoder,
            vec![compressed.image.clone(), output.clone()],
            vec![],
        );
        let mut sampled = compressed.clone();
        sampled.image = output;
        // Inspect decoded blocks without changing the original display.
        sampled.parent = source.ready.clone();
        sampled.ready = status.submitted();
        sampled.compression = None;
        let actual = sampled.readback(&cancel).unwrap();
        let errors: Vec<_> = actual
            .rgba()
            .iter()
            .zip(expected.rgba())
            .map(|(a, b)| f64::from(a.abs_diff(*b)))
            .collect();
        let rms = (errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64).sqrt();
        let max = errors.into_iter().fold(0f64, f64::max);
        eprintln!("BC7 lossy playback quality: RMS={rms:.3} max={max}");
        assert_eq!(
            compressed.statistics.status_readback_bytes, source.statistics.status_readback_bytes,
            "BC7 retention adds no quality/status readback"
        );
        assert_eq!(
            source.readback(&cancel).unwrap().rgba(),
            expected.rgba(),
            "compression cannot modify the original"
        );
        assert!(actual.rgba().chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert!(
            compressed.clone().readback(&cancel).is_err(),
            "cache is not a delivery source"
        );
    }
    assert!(host.memory().peak <= host.memory().budget);
    eprintln!("BC7 host memory: {:?}", host.memory());
}
