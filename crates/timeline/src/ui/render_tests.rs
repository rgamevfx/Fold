//! Offscreen native rendering for visual review without OS pointer automation.
use super::*;
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{imgui, typography::Typography};
#[test]
#[ignore = "native UI capture to /tmp/fold-timeline-ui"]
fn timeline_normal_and_narrow() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Timeline capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let _fonts = Typography::install(&mut context);
    let mut renderer = WgpuRenderer::new(
        WgpuInitInfo::new(
            device.clone(),
            queue.clone(),
            wgpu::TextureFormat::Rgba8Unorm,
        ),
        &mut context,
    )
    .unwrap();
    let size = wgpu::Extent3d {
        width: 1024,
        height: 480,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Timeline review"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut canvas = canvas::Canvas::default();
    canvas.media_previews = false;
    let tracks = vec![
        crate::Track::new(TrackKind::Video, "V1"),
        crate::Track::new(TrackKind::Video, "V2"),
        crate::Track::new(TrackKind::Audio, "A1"),
    ];
    let mut clips = Vec::new();
    for (i, start, duration, name, icon) in [
        (0, 0, 4, "Interview", ToolbarIcon::View),
        (0, 4, 5, "Cutaway", ToolbarIcon::View),
        (1, 2, 5, "Badge Wave", ToolbarIcon::Composition),
        (2, 0, 9, "Interview audio", ToolbarIcon::Audio),
    ] {
        let id = ObjectId::new();
        clips.push(crate::Clip {
            id,
            track: tracks[i].id,
            link: None,
            asset: Default::default(),
            info: crate::SourceMedia::Video(fold_media::VideoInfo {
                width: 1280,
                height: 720,
                rate: [24, 1],
                frames: 240,
            }),
            start: fold_foundation::Time::new(start, 1).unwrap(),
            source_start: fold_foundation::Time::ZERO,
            duration: fold_foundation::Time::new(duration, 1).unwrap(),
            level: 1.,
            extensions: Default::default(),
        });
        canvas.visuals.insert(
            id,
            canvas::ClipVisual {
                name: name.into(),
                icon,
                kind: "Video",
                texture: None,
                error: None,
            },
        );
    }
    let sequence = Sequence {
        dimensions: [1280, 720],
        rate: [24, 1],
        tracks,
        clips,
        extensions: Default::default(),
    };
    canvas.cache = (0..100)
        .map(|f| (f, fold_ui::sdk::cache_indicator::Coverage::Ready))
        .chain((120..160).map(|f| (f, fold_ui::sdk::cache_indicator::Coverage::Partial)))
        .collect();
    context.io_mut().set_display_size([1024., 480.]);
    context.io_mut().set_delta_time(1. / 60.);
    context.io_mut().add_mouse_pos_event([1000., 479.]);
    std::fs::create_dir_all("/tmp/fold-timeline-ui").unwrap();
    for (name, width) in [("normal", 1024.), ("narrow", 320.)] {
        canvas.fit = true;
        for frame_index in 0..5 {
            let ui = context.frame();
            ui.window("Timeline")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 470.], imgui::Condition::Always)
                .build(|| {
                    controls::toolbar(ui, &mut canvas, &sequence, &[], 60);
                    ui.separator();
                    canvas.draw(
                        ui,
                        canvas::CanvasModel {
                            sequence: &sequence,
                            revision: Default::default(),
                            playhead: 60,
                            selected: &[sequence.clips[2].id],
                            labels: &BTreeMap::new(),
                            placement: None,
                        },
                    );
                    if frame_index >= 2 {
                        assert_eq!(ui.scroll_max_x(), 0.);
                    }
                });
            let frame = context.render(renderer.renderer_consumer().unwrap());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                renderer
                    .render(frame, &mut pass, FramebufferExtent::from_texture(&texture))
                    .unwrap();
            }
            queue.submit([encoder.finish()]);
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024 * 480 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4096),
                    rows_per_image: None,
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let bright = (66..82)
            .flat_map(|y| (16..90).map(move |x| (y * 1024 + x) * 4))
            .filter(|&i| data[i] > 160 && data[i + 1] > 160 && data[i + 2] > 160)
            .count();
        assert!(
            bright > 15,
            "{name}: timecode text must remain visible ({bright} pixels)"
        );
        let mut ppm = b"P6\n1024 480\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold-timeline-ui/{name}.ppm"), ppm).unwrap();
    }
}
