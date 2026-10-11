use super::*;
use crate::sdk::typography::{TextRole, push_role};

#[test]
fn appearance_changes_reach_existing_and_new_graph_consumers() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = Typography::install(&mut context);
    let existing = fonts.clone();
    let mut settings = Settings::new(fonts.clone(), None);
    let mut value = Appearance::default();
    value.application_text = 20.;
    value.graph.labels = 24.;
    value.graph.titles = 30.;
    value.colors.text = [0.8, 0.9, 1.];
    settings.edit(value);
    settings.prepare_frame(&mut context);
    assert_eq!(existing.appearance(), value);
    assert_eq!(fonts.clone().appearance(), value);
    assert_eq!(context.style().font_size_base(), 20.);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([800., 600.]);
    context.io_mut().set_delta_time(1. / 60.);
    let ui = context.frame();
    ui.window("roles").build(|| {
        assert_eq!(ui.style_color(imgui::StyleColor::Text), [0.8, 0.9, 1., 1.]);
        let _role = push_role(ui, TextRole::Title, existing.appearance().graph);
        assert_eq!(ui.current_font_size(), 30.);
    });
    drop(context.render_legacy());
    let mut invalid = value;
    invalid.graph.labels = f32::NAN;
    assert!(fonts.set_appearance(invalid).is_err());
    assert_eq!(fonts.appearance(), value);
    settings.edit(Appearance::default());
    assert_eq!(existing.appearance(), Appearance::default());
}

#[test]
fn settings_draw_at_normal_and_narrow_sizes_without_changes() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut settings = Settings::new(fonts.clone(), None);
    settings.open = true;
    for (width, application) in [(640., false), (440., false), (640., true), (440., true)] {
        settings.application_category = application;
        settings.application_draft.automatic = false;
        for _ in 0..3 {
            settings.prepare_frame(&mut context);
            let ui = context.frame();
            ui.window("Settings##fold.settings")
                .size([width, 480.], imgui::Condition::Always)
                .build(|| {});
            settings.draw(ui);
            assert!(context.render_legacy().total_vtx_count() > 100);
        }
    }
    assert_eq!(fonts.appearance(), Appearance::default());
    assert!(!settings.edited);
}

#[test]
#[ignore = "native capture; writes /tmp/fold58-settings-ui"]
fn playback_settings_native_normal_and_narrow() {
    use crate::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Settings capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = crate::sdk::typography::Typography::install(&mut context);
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
        width: 768,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Settings UI"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    context.io_mut().set_display_size([768., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold58-settings-ui").unwrap();
    let mut settings = Settings::new(fonts, None);
    settings.application_category = true;
    settings.application_draft.automatic = false;
    settings.application_draft.timeline_disk_gib = 20;
    settings.application_draft.composition_disk_gib = 10;
    settings.open = true;
    for (name, width, scroll, automatic) in [
        ("normal-top", 640., 100., false),
        ("normal-bottom", 640., -100., false),
        ("narrow-top", 440., 100., false),
        ("narrow-bottom", 440., -100., false),
        ("automatic-normal", 640., 100., true),
        ("automatic-narrow", 440., 100., true),
    ] {
        settings.application_draft.automatic = automatic;
        context.load_ini_settings(&format!(
            "[Window][Settings##fold.settings]\nPos=10,10\nSize={},780\nCollapsed=0\n",
            width
        ));
        for step in 0..12 {
            context.io_mut().add_mouse_pos_event([width - 60., 300.]);
            if step == 4 {
                context.io_mut().add_mouse_wheel_event([0., scroll]);
            }
            let ui = context.frame();
            ui.window("Settings##fold.settings")
                .position([10., 10.], imgui::Condition::Always)
                .size([width, 780.], imgui::Condition::Always)
                .build(|| {});
            settings.draw(ui);
            let frame = context.render(renderer.renderer_consumer().unwrap());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
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
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 768 * 800 * 4,
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
                    bytes_per_row: Some(768 * 4),
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
        let mut ppm = b"P6\n768 800\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold58-settings-ui/{name}.ppm"), ppm).unwrap();
    }
}
