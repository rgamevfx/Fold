//! Native-backend captures of the actual Motion controls, not a UI mockup.
use super::*;
use dear_imgui_wgpu::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{ExtensionUi, Panel, imgui, typography::Typography};
#[test]
#[ignore = "native renderer; writes /tmp/fold-motion-v2-ui"]
fn motion_tools_and_inspector_render_normal_and_narrow() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
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
        label: Some("Motion UX review"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    std::fs::create_dir_all("/tmp/fold-motion-v2-ui").unwrap();
    let mut host = tests::Host::new();
    let shared = state::Shared::default();
    {
        let mut state = shared.borrow_mut();
        state.create(&mut host);
        state.change_scene(&mut host, |m| {
            let badge = crate::authoring::scene::assets::badge(m)?;
            let path = crate::authoring::scene::create_object(m, "fold.motion.path")?;
            crate::authoring::scene::assets::along_path(m, badge, path).map(Some)
        });
    }
    let mut canvas = canvas::Canvas {
        state: shared.clone(),
        canvas: Default::default(),
        overlay: Default::default(),
        animation: Default::default(),
        network_animation: Default::default(),
    };
    canvas.initialize(&context, Some(&fonts));
    let mut inspector = inspector::Inspector { state: shared };
    context.io_mut().set_display_size([1280., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    for (name, width, properties) in [
        ("normal", 850., 400.),
        ("narrow", 390., 280.),
        ("network", 850., 400.),
    ] {
        for _ in 0..12 {
            let ui = context.frame();
            ui.window("Motion viewer")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 500.], imgui::Condition::Always)
                .build(|| {
                    let origin = ui.cursor_screen_pos();
                    let area = ui.content_region_avail();
                    let inset = tools::insets(ui);
                    canvas.draw_viewer_overlay(
                        ExtensionUi {
                            ui,
                            host: &mut host,
                        },
                        fold_ui::sdk::ViewerRect {
                            editable: true,
                            image_current: true,
                            canvas_origin: origin,
                            canvas_size: area,
                            origin: [origin[0] + inset[0], origin[1] + inset[1]],
                            size: [area[0] - inset[0], area[1] - inset[1]],
                            dimensions: [1280, 720],
                        },
                    );
                });
            ui.window("Inspector")
                .position([width + 8., 0.], imgui::Condition::Always)
                .size([properties, 790.], imgui::Condition::Always)
                .build(|| {
                    inspector.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    })
                });
            ui.window("Motion")
                .position([0., 508.], imgui::Condition::Always)
                .size([width, 280.], imgui::Condition::Always)
                .build(|| {
                    if name == "network" {
                        canvas.draw_network(ExtensionUi {
                            ui,
                            host: &mut host,
                        })
                    } else {
                        canvas.draw(ExtensionUi {
                            ui,
                            host: &mut host,
                        })
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
        for p in data.chunks_exact(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(format!("/tmp/fold-motion-v2-ui/{name}.ppm"), ppm).unwrap();
    }
}
