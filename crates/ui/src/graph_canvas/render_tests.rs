//! Real backend coverage for dynamic atlas updates, DPI and node presentation.
use super::*;
use crate::sdk::typography::Typography;
use dear_imgui_wgpu::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};

#[test]
#[ignore = "requires native GPU; writes review captures to /tmp/fold-node-ui"]
fn graph_typography_renders_across_zoom_dpi_and_ui_scale() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Node typography adapter: {:?}", adapter.get_info());
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
        width: 1024,
        height: 768,
        depth_or_array_layers: 1,
    };
    std::fs::create_dir_all("/tmp/fold-node-ui").unwrap();
    let mut surfaces = Vec::new();
    for (name, width, dpi, scale, wheel) in [
        ("normal", 1000., 1., 1., 0.),
        ("zoom-in", 1000., 1., 1., 3.),
        ("overview", 1000., 1., 1., -5.),
        ("narrow", 380., 1., 1., 0.),
        ("large-ui", 1000., 1., 1.5, 0.),
        ("dpi-2", 500., 2., 1., 0.),
        ("eight-nodes", 1000., 1., 1., 0.),
        ("settings", 640., 1., 1., 0.),
        ("settings-narrow", 440., 1., 1., 0.),
        ("settings-application", 640., 1., 1., 0.),
        ("settings-application-narrow", 440., 1., 1., 0.),
        ("menu-file", 1024., 1., 1., 0.),
        ("menu-file-narrow", 440., 1., 1., 0.),
        ("menu-edit", 1024., 1., 1., 0.),
        ("menu-workspace", 1024., 1., 1., 0.),
        ("menu-unsaved", 440., 1., 1., 0.),
    ] {
        if std::env::var("FOLD_UI_REVIEW_FILTER").is_ok_and(|prefix| !name.starts_with(&prefix)) {
            continue;
        }
        let size = wgpu::Extent3d {
            width: if name.starts_with("menu") {
                width as u32
            } else {
                1024
            },
            ..size
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("node typography review"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut graph = Context::new();
        graph
            .graph
            .nodes
            .truncate(if name == "eight-nodes" { 8 } else { 2 });
        graph
            .graph
            .wires
            .truncate(if name == "eight-nodes" { 7 } else { 1 });
        for (i, node) in graph.graph.nodes.iter_mut().enumerate() {
            node.label = ["Text", "Transform"][i % 2].into();
            node.summary = ["Editable text", "Position, rotation and scale"][i % 2].into();
            node.inputs = ["Content", "Position", "Rotation", "Scale", "Opacity"]
                .map(|label| PortView {
                    key: label.into(),
                    label: label.into(),
                    color: GRAPH_COLORS.image,
                })
                .to_vec();
            node.inputs[0].key = "image".into();
            node.outputs = vec![PortView {
                key: "image".into(),
                label: "Content".into(),
                color: GRAPH_COLORS.image,
            }];
            node.position =
                (name == "eight-nodes").then_some([(i % 4) as f32 * 300., (i / 4) as f32 * 280.]);
        }
        let mut settings = crate::settings::Settings::new(fonts.clone(), None);
        settings.open = name.starts_with("settings");
        settings.application_category = name.starts_with("settings-application");
        settings.application_draft.automatic = false;
        settings.adapter = "NVIDIA GeForce GTX 1070".into();
        let mut project_menu = crate::project_menu::ProjectMenu::default();
        let mut workspace_menu = crate::workspace_presets::Menu::default();
        workspace_menu.active = Some("Editing".into());
        settings.preferences.workspaces.insert(
            "Editing".into(),
            serde_json::to_value(fold_platform::workspace::Workspace::default()).unwrap(),
        );
        let mut host = MenuHost::default();
        host.state.dirty = true;
        host.state.can_undo = true;
        if name == "menu-unsaved" {
            project_menu.request(crate::project_menu::Action::Quit, &mut host);
        }
        let mut canvas = GraphCanvas::default();
        canvas.initialize(&context, Some(&fonts));
        context.style_mut().set_font_scale_main(scale);
        context.io_mut().set_display_size([
            if name.starts_with("menu") {
                width
            } else {
                1024. / dpi
            },
            768. / dpi,
        ]);
        context.io_mut().set_display_framebuffer_scale([dpi; 2]);
        context.io_mut().set_delta_time(1. / 60.);
        for frame in 0..35 {
            context.io_mut().add_mouse_pos_event(if frame < 25 {
                [width / 2., 300. / dpi]
            } else {
                [-100., -100.]
            });
            if frame == 20 && wheel != 0. {
                context.io_mut().add_mouse_wheel_event([0., wheel]);
            }
            if name.starts_with("menu") && name != "menu-unsaved" {
                if frame < 2 {
                    context
                        .io_mut()
                        .add_key_event(imgui::Key::Escape, frame == 0);
                }
                context.io_mut().add_mouse_pos_event([-100., -100.]);
            }
            settings.prepare_frame(&mut context);
            let ui = context.frame();
            if name.starts_with("menu") {
                ui.main_menu_bar(|| {
                    if frame == 10 && name != "menu-unsaved" {
                        ui.open_popup(if name == "menu-edit" {
                            "Edit"
                        } else if name == "menu-workspace" {
                            "Workspace"
                        } else {
                            "File"
                        });
                    }
                    project_menu.menus(ui, &mut host, &["/projects/Opening titles.fold".into()]);
                    workspace_menu.draw(ui, Some(&settings));
                    if let Some(_menu) = ui.begin_menu("Panels") {
                        ui.menu_item("New viewer");
                    }
                    if let Some(_menu) = ui.begin_menu("Fold") {
                        ui.menu_item("Settings…");
                    }
                });
                project_menu.confirmation(ui, &mut host);
                workspace_menu.dialog(ui, Some(&settings));
            } else if name.starts_with("settings") {
                ui.window("Settings##fold.settings")
                    .position([0.; 2], imgui::Condition::Always)
                    .size([width, 560.], imgui::Condition::Always)
                    .build(|| {});
                settings.draw(ui);
            } else {
                ui.window("Network typography")
                    .position([0.; 2], imgui::Condition::Always)
                    .size([width, 740. / dpi], imgui::Condition::Always)
                    .build(|| {
                        // Different raster densities must never move labels/pins.
                        let mut reference = None;
                        for density in [1., 1.5, 2., 3., 4., 6., 8.] {
                            let _font =
                                ui.push_font_with_size(Some(fonts.canvas_font(density)), 0.);
                            let measured = ui.calc_text_size("Position Rotation Scale");
                            if let Some(reference) = reference {
                                assert_eq!(
                                    measured, reference,
                                    "density changed logical text metrics"
                                );
                            }
                            reference = Some(measured);
                            if frame == 0 && name == "normal" {
                                let mut baked = ui.current_baked_font();
                                for c in "Position Rotation Scale".chars() {
                                    baked.glyph_or_fallback(c);
                                }
                                surfaces.push(baked.metrics_total_surface());
                            }
                        }
                        canvas.draw(ui, &mut graph);
                        // Observe the font actually bound while drawing the graph,
                        // using its measured screen transform as the independent
                        // zoom oracle (the native zoom getter is inverse scale).
                        assert_eq!(
                            canvas.raster_font,
                            Some(fonts.canvas_font(super::scale(&canvas) / 100.))
                        );
                    });
            }
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
        let zoom = super::scale(&canvas) / 100.;
        eprintln!("{name}: zoom {zoom:.3}, UI scale {scale}, DPI {dpi}");
        let row_bytes = (size.width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row_bytes) * u64::from(size.height),
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
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: None,
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let bright_text = data
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, pixel)| {
                let y = i / 1024;
                y > 160 && y < 700 && pixel[..3].iter().all(|v| *v > 180)
            })
            .count();
        if matches!(name, "normal" | "zoom-in") {
            assert!(
                bright_text > 100,
                "{name}: node labels disappeared ({bright_text} bright pixels)"
            );
        }
        let mut ppm = format!("P6\n{} {}\n255\n", size.width, size.height).into_bytes();
        for row in data.chunks_exact(row_bytes as usize) {
            for pixel in row[..size.width as usize * 4].chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
        }
        std::fs::write(format!("/tmp/fold-node-ui/{name}.ppm"), ppm).unwrap();
    }
    assert!(
        surfaces.is_empty() || surfaces.last().unwrap() > &(surfaces[0] * 8),
        "higher density must allocate higher-resolution glyphs: {surfaces:?}"
    );
}

#[derive(Default)]
struct MenuHost {
    state: fold_platform::desktop::DesktopState,
}
impl fold_platform::desktop::DesktopClient for MenuHost {
    fn state(&self) -> &fold_platform::desktop::DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, _: fold_platform::desktop::DesktopCommand) {}
    fn request_preview(&mut self, _: fold_platform::desktop::PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<fold_platform::desktop::PreviewResult> {
        None
    }
}
