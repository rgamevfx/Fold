//! Native-backend captures of the actual Motion controls, not a UI mockup.
use super::*;
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{ExtensionUi, Panel, imgui, typography::Typography};
#[test]
#[ignore = "native renderer; writes /tmp/fold-motion-v2-ui"]
fn motion_tools_and_inspector_render_normal_and_narrow() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Motion UI capture adapter: {:?}", adapter.get_info());
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
    };
    canvas.initialize(&context, Some(&fonts));
    let mut inspector = inspector::Inspector { state: shared };
    context.io_mut().set_display_size([1280., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    for (name, width, properties) in [
        ("composition-normal", 850., 400.),
        ("composition-narrow", 280., 280.),
        ("composition-modifier-normal", 850., 400.),
        ("composition-modifier-narrow", 280., 280.),
        ("composition-nested-normal", 850., 400.),
        ("composition-nested-narrow", 280., 280.),
        ("composition-settings", 850., 400.),
        ("images-bake-normal", 850., 400.),
        ("images-bake-narrow", 390., 280.),
        ("images-bake-compact-normal", 850., 400.),
        ("images-bake-compact-narrow", 390., 280.),
        ("images-source-normal", 850., 400.),
        ("images-source-narrow", 390., 280.),
        ("images-grid-normal", 850., 400.),
        ("images-grid-narrow", 390., 280.),
        ("images-sample-normal", 850., 400.),
        ("images-sample-narrow", 390., 280.),
        ("images-ascii-normal", 850., 400.),
        ("images-ascii-narrow", 390., 280.),
        ("normal", 850., 400.),
        ("narrow", 390., 280.),
        ("scene-drop-normal", 850., 400.),
        ("scene-drop-narrow", 390., 280.),
        ("network", 850., 400.),
        ("duplicator-normal", 850., 400.),
        ("duplicator-narrow", 390., 280.),
        ("duplicator-network", 850., 400.),
        ("wave-driver", 850., 400.),
        ("group-interface", 850., 400.),
        ("count-menu", 850., 400.),
        ("oscillator-normal", 850., 400.),
        ("oscillator-narrow", 390., 280.),
        ("oscillator-custom", 850., 400.),
        ("color-ramp-normal", 850., 400.),
        ("color-ramp-narrow", 390., 280.),
        ("color-ramp-network", 850., 400.),
        ("solver-normal", 850., 400.),
        ("solver-narrow", 390., 280.),
        ("drift-normal", 850., 400.),
        ("drift-narrow", 390., 280.),
        ("particles-normal", 850., 400.),
        ("particles-narrow", 390., 280.),
        ("particles-source-normal", 850., 400.),
        ("particles-source-narrow", 390., 280.),
        ("particles-palette-normal", 850., 400.),
        ("particles-palette-narrow", 390., 280.),
        ("wrangle-normal", 850., 400.),
        ("wrangle-narrow", 390., 280.),
    ] {
        if let Ok(filter) = std::env::var("FOLD_MOTION_CAPTURE_FILTER") {
            if !name.starts_with(&filter) {
                continue;
            }
        }
        if name.starts_with("images-") && name.ends_with("normal") {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let kind = if name.contains("bake") {
                    "fold.motion.image_bake"
                } else if name.contains("source") {
                    "fold.motion.image"
                } else if name.contains("sample") {
                    "fold.motion.sample_image"
                } else if name.contains("ascii") {
                    "fold.motion.image_ascii"
                } else {
                    "fold.motion.image_grid"
                };
                let id = crate::authoring::add_node(m, kind)?;
                if name.contains("bake-compact") {
                    let node = crate::authoring::node(m, id)?;
                    node.settings["format"] = serde_json::json!("Bc7");
                    node.settings["clamp_sdr"] = serde_json::json!(true);
                }
                if kind == "fold.motion.image_ascii" {
                    let font = fold_foundation::ObjectId::new();
                    m.fonts.insert(font, crate::document::Font::builtin());
                    crate::authoring::node(m, id)?.settings["font"] = serde_json::json!(font);
                }
                Ok(Some(id))
            });
            assert!(state.error.is_empty(), "{}", state.error);
        }
        if name == "wrangle-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let points = crate::authoring::add_node(m, "fold.motion.linear_points")?;
                let code = crate::authoring::add_node(m, "fold.motion.wrangle")?;
                crate::authoring::node(m, code)?.connect("content", points, "points");
                Ok(Some(code))
            });
            assert!(state.error.is_empty(), "{}", state.error);
        }
        if name == "particles-source-normal" || name == "particles-palette-normal" {
            let mut state = inspector.state.borrow_mut();
            let owner = state
                .inspector_return
                .map(|(_, parent)| parent)
                .unwrap_or(state.selected[0]);
            let key = if name == "particles-source-normal" {
                "points"
            } else {
                "color"
            };
            let crate::graph::Input::Link(link) = &state
                .motion
                .as_ref()
                .unwrap()
                .graph
                .node(owner)
                .unwrap()
                .inputs[key]
            else {
                panic!()
            };
            let driver = link.node;
            state.inspector_return = Some((driver, owner));
            state.network_object = Some(owner);
            state.selected = vec![driver];
        }
        if name == "particles-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let source = crate::authoring::scene::create_object(m, "fold.motion.text")?;
                crate::authoring::particles::from_object(m, source).map(Some)
            });
            assert!(state.error.is_empty(), "{}", state.error);
        }
        if name == "solver-normal" || name == "drift-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let source = crate::authoring::scene::create_object(m, "fold.motion.rectangle")?;
                if name == "solver-normal" {
                    let solver = crate::authoring::add_node(m, "fold.motion.solver")?;
                    crate::authoring::node(m, solver)?.connect("initial", source, "content");
                    Ok(Some(solver))
                } else {
                    crate::authoring::drift::from_object(m, source).map(Some)
                }
            });
        }
        if name == "duplicator-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let source = crate::authoring::scene::assets::badge(m)?;
                let id = crate::authoring::scene::assets::duplicate(m, source)?;
                crate::authoring::scene::assets::attach_copy_wave(m, id)?;
                crate::authoring::scene::assets::attach_copy_palette(m, id)?;
                Ok(Some(id))
            });
        }
        if name == "wave-driver" {
            let mut state = inspector.state.borrow_mut();
            let owner = state.selected[0];
            let driver = crate::authoring::scene::assets::copy_driver(
                state.motion.as_ref().unwrap(),
                owner,
                "offset_y",
            )
            .unwrap();
            state.inspector_return = Some((driver, owner));
            state.network_object = Some(owner);
            state.selected = vec![driver];
        }
        if name == "group-interface" {
            let mut state = inspector.state.borrow_mut();
            let group = state
                .motion
                .as_ref()
                .unwrap()
                .graph
                .node(state.selected[0])
                .unwrap()
                .settings::<crate::nodes::interface::GroupSettings>()
                .unwrap()
                .group
                .unwrap();
            state.interface_target = Some(group);
        }
        if name == "count-menu" {
            let mut state = inspector.state.borrow_mut();
            state.interface_target = None;
            let owner = state.inspector_return.unwrap().1;
            state.selected = vec![owner];
        }
        if name == "oscillator-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let badge = crate::authoring::scene::assets::badge(m)?;
                let copies = crate::authoring::scene::assets::duplicate(m, badge)?;
                let driver =
                    crate::authoring::scene::assets::attach_oscillator(m, copies, "offset_y")?;
                Ok(Some(driver))
            });
        }
        if name == "oscillator-custom" {
            let mut state = inspector.state.borrow_mut();
            let selected = state.selected[0];
            state.change(&mut host, |m| {
                crate::authoring::node(m, selected)?
                    .set("shape", crate::fields::Datum::Text("Custom".into()));
                Ok(Some(selected))
            });
        }
        if name == "color-ramp-normal" {
            let mut state = inspector.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                let badge = crate::authoring::scene::assets::badge(m)?;
                let copies = crate::authoring::scene::assets::duplicate(m, badge)?;
                let ramp = crate::authoring::scene::assets::attach_color_ramp(m, copies, "color")?;
                crate::authoring::scene::assets::set_color_ramp(
                    m,
                    ramp,
                    crate::nodes::color_ramp::Settings {
                        stops: vec![
                            crate::nodes::color_ramp::stop(0., [1., 0., 0., 1.]),
                            crate::nodes::color_ramp::stop(0.5, [0., 1., 0., 0.25]),
                            crate::nodes::color_ramp::stop(1., [0., 0., 1., 1.]),
                        ],
                        ..Default::default()
                    },
                )?;
                Ok(Some(copies))
            });
            let owner = state.selected[0];
            let ramp = crate::authoring::scene::assets::copy_driver(
                state.motion.as_ref().unwrap(),
                owner,
                "color",
            )
            .unwrap();
            state.network_object = Some(owner);
            state.inspector_return = Some((ramp, owner));
            state.selected = vec![ramp];
        }
        if name.starts_with("composition-modifier-") {
            use fold_ui::sdk::graph_canvas::{GraphContext, GraphEvent};
            let mut state = canvas.state.borrow_mut();
            state.create(&mut host);
            state.change(&mut host, |m| {
                crate::authoring::add_content(m, "fold.motion.rectangle").map(Some)
            });
            let rectangle = state.selected[0];
            state.change(&mut host, |m| {
                crate::authoring::group_node(m, rectangle).map(Some)
            });
            let instance = state.selected[0];
            graph::Context {
                state: &mut state,
                host: &mut host,
            }
            .event(GraphEvent::Navigate(Some(instance)))
            .unwrap();
            state.change(&mut host, |m| {
                let node = crate::authoring::node(m, rectangle)?;
                node.set("width", crate::fields::Datum::Scalar(120.));
                let mut curve = fold_animation::Curve::default();
                curve.insert(fold_foundation::Time::ZERO, 120.);
                curve.insert(fold_foundation::Time::new(1, 1).unwrap(), 240.);
                node.animation.insert("width.0".into(), curve);
                Ok(Some(rectangle))
            });
        }
        if name.starts_with("composition-nested-") {
            let mut state = canvas.state.borrow_mut();
            state.create(&mut host);
            state.change_scene(&mut host, |m| {
                crate::authoring::scene::create_object(m, "fold.motion.rectangle").map(Some)
            });
            super::composition::nest(&mut state, &mut host, false);
        }
        if name == "composition-settings" {
            let mut state = canvas.state.borrow_mut();
            state.composition_settings = state.motion.as_ref().map(|m| m.info.clone());
        }
        for frame_index in 0..12 {
            if name.starts_with("scene-drop-") || name == "network" {
                context
                    .io_mut()
                    .add_key_event(imgui::Key::Escape, frame_index == 0);
                if frame_index == 0 {
                    context
                        .io_mut()
                        .add_mouse_button_event(imgui::MouseButton::Left, false);
                }
            }
            if name.starts_with("scene-drop-") {
                context
                    .io_mut()
                    .add_mouse_pos_event([100., if frame_index < 5 { 662. } else { 724. }]);
                if frame_index == 4 {
                    context
                        .io_mut()
                        .add_mouse_button_event(imgui::MouseButton::Left, true);
                }
            }

            if name == "oscillator-normal" {
                context
                    .io_mut()
                    .add_key_event(imgui::Key::Escape, frame_index == 0);
                context.io_mut().add_mouse_pos_event([700., 790.]);
            }

            if name == "count-menu" {
                context.io_mut().add_mouse_pos_event([940., 307.]);
                context
                    .io_mut()
                    .add_mouse_button_event(imgui::MouseButton::Right, frame_index == 4);
            }
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
            ui.window("Composition")
                .position([0., 508.], imgui::Condition::Always)
                .size([width, 280.], imgui::Condition::Always)
                .build(|| {
                    if name == "network"
                        || name == "duplicator-network"
                        || name == "wave-driver"
                        || name == "color-ramp-network"
                    {
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
