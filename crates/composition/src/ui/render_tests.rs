//! Native renderer capture of the common layer editor, graph and Inspector.
use super::*;
use sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use sdk::{imgui, typography::Typography};
#[test]
#[ignore = "native capture; writes /tmp/fold-common-composition-ui"]
fn common_composition_normal_and_narrow() {
    let _guard = tests::IMGUI_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Composition capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = Typography::install(&mut context);
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
        width: 1280,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Common composition UI"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let (mut host, shared) = tests::setup();
    let mut canvas = Canvas::new(shared.clone());
    canvas.initialize(&context, Some(&fonts));
    let mut inspector = inspector::Inspector(shared);
    context.io_mut().set_display_size([1280., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-common-composition-ui").unwrap();
    for (name, width) in [
        ("normal", 850.),
        ("narrow", 280.),
        ("settings", 850.),
        ("add-layer-menu", 850.),
        ("add-layer-menu-narrow", 280.),
    ] {
        if name == "settings" {
            let mut state = inspector.0.borrow_mut();
            state.settings = state.composition.as_ref().map(|c| c.info.clone());
        }
        if name.starts_with("add-layer-menu") {
            inspector.0.borrow_mut().settings = None;
        }
        // Let native graph camera framing settle after changing panel width.
        for frame_index in 0..40 {
            if name.starts_with("add-layer-menu") {
                context
                    .io_mut()
                    .add_key_event(imgui::Key::Escape, frame_index == 0);
            }
            let ui = context.frame();
            ui.window("Composition")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 790.], imgui::Condition::Always)
                .build(|| {
                    if name.starts_with("add-layer-menu") && frame_index == 2 {
                        ui.open_popup("Composition");
                    }
                    canvas.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                    if frame_index >= 2 {
                        assert_eq!(ui.scroll_max_x(), 0.);
                    }
                });
            ui.window("Inspector")
                .position([width + 10., 0.], imgui::Condition::Always)
                .size(
                    [if width < 400. { 280. } else { 400. }, 790.],
                    imgui::Condition::Always,
                )
                .build(|| {
                    inspector.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
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
            size: 1280 * 800 * 4,
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
                    bytes_per_row: Some(5120),
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
        let mut ppm = b"P6\n1280 800\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold-common-composition-ui/{name}.ppm"), ppm).unwrap();
    }
}
