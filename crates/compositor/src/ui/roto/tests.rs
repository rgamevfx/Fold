use super::*;
use crate::ui::{IMGUI_TEST_LOCK, canvas, inspector, state, tests::setup};
use fold_project::{EditBatch, Mutation};
use fold_ui::sdk::{ExtensionUi, Panel, ViewerRect, imgui, typography::Typography};
use std::{cell::RefCell, rc::Rc};
#[test]
fn roto_point_edit_keeps_ids_keys_and_corner_pin_maps_the_box() {
    let mut before = Roto::default();
    let shape = crate::roto::Layer::rectangle([10., 20.], [50., 60.], false);
    let id = shape.id;
    let point = shape.shape.as_ref().unwrap().points[0].id;
    before.root.children.push(shape);
    let mut node = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: before.clone(),
    });
    let mut after = before.clone();
    after
        .root
        .find_mut(id)
        .unwrap()
        .shape
        .as_mut()
        .unwrap()
        .points[0]
        .position = [12., 25.];
    let time = Time::new(7, 24).unwrap();
    edit(&mut node, &before, after, time, true).unwrap();
    let path = format!("roto/{id}/point/{point}/position.0");
    let curve = node.animation.curve(&path).unwrap().unwrap();
    assert_eq!(curve.sample(time).unwrap(), 12.);
    let key = curve.keys[0].id;
    let mut after = before.clone();
    after
        .root
        .find_mut(id)
        .unwrap()
        .shape
        .as_mut()
        .unwrap()
        .points[0]
        .position = [14., 25.];
    edit(&mut node, &before, after, time, true).unwrap();
    assert_eq!(
        node.animation.curve(&path).unwrap().unwrap().keys[0].id,
        key
    );
    let b = [0., 0., 100., 100.];
    let q = [[0., 0.], [120., 10.], [100., 100.], [0., 100.]];
    for (p, expected) in transform::corners(b).into_iter().zip(q) {
        let actual = transform::project(b, q, p).unwrap();
        assert!((actual[0] - expected[0]).abs() < 1e-8 && (actual[1] - expected[1]).abs() < 1e-8);
    }
    assert!(
        transform::project(
            b,
            [[0., 0.], [100., 100.], [100., 0.], [0., 100.]],
            [50., 50.]
        )
        .is_err()
    );
}
#[test]
fn roto_viewport_drag_cancel_commit_and_selection_routing() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, mut initial) = setup();
    let document = initial.document.unwrap();
    let mut graph = initial.graph.take().unwrap();
    graph.info.width = 100;
    graph.info.height = 100;
    let mut settings = Roto::default();
    let shape = crate::roto::Layer::rectangle([20., 20.], [70., 70.], false);
    let layer = shape.id;
    let point = shape.shape.as_ref().unwrap().points[0].id;
    settings.root.children.push(shape);
    let node = crate::Node::disconnected(crate::OperationConfig::Roto { settings });
    let id = node.id;
    graph.nodes.push(node);
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let mut state = state::State::default();
    state.sync(&mut host);
    state.selected = vec![id];
    state.roto.borrow_mut().select(id);
    state.roto.borrow_mut().layer = Some(layer);
    state.roto.borrow_mut().points.insert(point);
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 500.]);
    context.io_mut().set_delta_time(1. / 60.);
    let before = host.project.snapshot();
    let rect = ViewerRect {
        editable: true,
        image_current: true,
        canvas_origin: [10., 30.],
        canvas_size: [480., 450.],
        origin: [50., 100.],
        size: [300., 300.],
        dimensions: [100, 100],
    };
    // The point at (20,20) is (110,160) in the viewport.
    for (i, (p, down, escape)) in [
        ([500., 490.], false, false),
        ([500., 490.], false, false),
        ([110., 160.], true, false),
        ([140., 190.], true, false),
        ([140., 190.], true, true),
        ([140., 190.], false, false),
        ([110., 160.], true, false),
        ([140., 190.], true, false),
        ([140., 190.], false, false),
        ([140., 190.], false, false),
        ([140., 190.], false, false),
    ]
    .into_iter()
    .enumerate()
    {
        context.io_mut().add_mouse_pos_event(p);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, down);
        context.io_mut().add_key_event(imgui::Key::Escape, escape);
        let ui = context.frame();
        ui.window("Viewer")
            .position([0., 0.], imgui::Condition::Always)
            .size([500., 490.], imgui::Condition::Always)
            .build(|| {
                let shared = state.roto.clone();
                shared.borrow_mut().draw(
                    ExtensionUi {
                        ui,
                        host: &mut host,
                    },
                    rect,
                    &mut state,
                );
            });
        let _ = context.render_legacy();
        if i == 5 {
            assert_eq!(host.project.snapshot().revision(), before.revision());
            assert_eq!(state.graph.as_ref().unwrap(), &graph);
        }
    }
    assert_ne!(
        host.project.snapshot().revision(),
        before.revision(),
        "drag must commit"
    );
    let graph =
        crate::Composite::from_document(&host.project.snapshot().state().documents[&document])
            .unwrap();
    let crate::OperationConfig::Roto { settings } = graph
        .prepare()
        .unwrap()
        .evaluated_parameters(id, Time::ZERO)
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        settings
            .root
            .find(layer)
            .unwrap()
            .shape
            .as_ref()
            .unwrap()
            .points[0]
            .position,
        [30., 30.]
    );
    host.project.undo().unwrap();
    let restored =
        crate::Composite::from_document(&host.project.snapshot().state().documents[&document])
            .unwrap();
    assert_eq!(
        restored,
        crate::Composite::from_document(&before.state().documents[&document]).unwrap()
    );
}
#[test]
#[ignore = "native Roto inspector/viewport screenshots"]
fn roto_native_normal_and_narrow() {
    use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Roto UI {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    Typography::install(&mut context);
    fold_ui::sdk::appearance::Appearance::default().apply(context.style_mut());
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
        height: 768,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Roto UI review"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let (mut host, initial) = setup();
    let document = initial.document.unwrap();
    let mut graph = initial.graph.unwrap();
    graph.info.width = 640;
    graph.info.height = 480;
    let mut settings = Roto::default();
    let mut shape = crate::roto::Layer::rectangle([100., 100.], [420., 350.], true);
    shape.shape.as_mut().unwrap().feather = 25.;
    let selected = shape.id;
    let points = shape
        .shape
        .as_ref()
        .unwrap()
        .points
        .iter()
        .map(|p| p.id)
        .collect();
    settings.root.children.push(shape);
    let mut node = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: settings.clone(),
    });
    keys::set(&mut node, &settings, selected, Time::ZERO, false).unwrap();
    let id = node.id;
    graph.nodes.push(node);
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let shared = Rc::new(RefCell::new(state::State::default()));
    shared.borrow_mut().sync(&mut host);
    shared.borrow_mut().selected = vec![id];
    {
        let tools = shared.borrow().roto.clone();
        let mut tools = tools.borrow_mut();
        tools.select(id);
        tools.layer = Some(selected);
        tools.points = points;
        tools.show_feather = true;
    }
    let mut inspector = inspector::Inspector(
        shared.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let mut canvas = canvas::Canvas::new(shared.clone());
    context.io_mut().set_display_size([768., 768.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-roto-ui").unwrap();
    context.io_mut().add_mouse_pos_event([760., 760.]);
    for (name, width, viewport) in [
        ("inspector-normal", 420., false),
        ("inspector-narrow", 280., false),
        ("viewport", 750., true),
        ("viewport-narrow", 280., true),
        ("bezier-draft", 750., true),
        ("bspline-draft", 750., true),
        ("bezier-close", 750., true),
    ] {
        let draft = name.contains("draft") || name.contains("close");
        let mut cursor = if draft { [460., 400.] } else { [760., 760.] };
        if draft {
            let tools = shared.borrow().roto.clone();
            let mut tools = tools.borrow_mut();
            tools.layer = None;
            tools.points.clear();
            let spline = if name == "bspline-draft" {
                crate::roto::Spline::BSpline
            } else {
                crate::roto::Spline::Bezier
            };
            tools.tool = if spline == crate::roto::Spline::BSpline {
                Tool::BSpline
            } else {
                Tool::Bezier
            };
            let mut points = vec![
                crate::roto::Point::new([100., 100.]),
                crate::roto::Point::new([300., 150.]),
                crate::roto::Point::new([320., 300.]),
            ];
            points[1].incoming = [-40., -30.];
            points[1].outgoing = [40., 30.];
            let mut layer = crate::roto::Layer::curve(spline, points);
            layer.shape.as_mut().unwrap().closed = false;
            tools.drawing = Some(layer);
        }
        for frame in 0..4 {
            context.io_mut().add_mouse_pos_event(cursor);
            let ui = context.frame();
            ui.window(if viewport {
                "Roto Viewer"
            } else {
                "Roto Inspector"
            })
            .position([0.; 2], imgui::Condition::Always)
            .size([width, 750.], imgui::Condition::Always)
            .build(|| {
                if viewport {
                    let origin = ui.cursor_screen_pos();
                    let available = ui.content_region_avail();
                    let width = (available[0] - 20.).max(1.);
                    if name == "bezier-close" {
                        cursor = [
                            origin[0] + 10. + width * 100. / 640.,
                            origin[1] + 80. + width * 100. / 640.,
                        ];
                    }
                    if draft {
                        let draw = ui.get_window_draw_list();
                        for y in 0..8 {
                            for x in 0..10 {
                                let p = [
                                    origin[0] + 10. + x as f32 * width / 10.,
                                    origin[1] + 80. + y as f32 * width * 0.75 / 8.,
                                ];
                                let shade = if (x + y) % 2 == 0 { 0.16 } else { 0.65 };
                                draw.add_rect(
                                    p,
                                    [p[0] + width / 10., p[1] + width * 0.75 / 8.],
                                    [shade, shade, shade, 1.],
                                )
                                .filled(true)
                                .build();
                            }
                        }
                    }
                    canvas.draw_viewer_overlay(
                        ExtensionUi {
                            ui,
                            host: &mut host,
                        },
                        ViewerRect {
                            editable: true,
                            image_current: true,
                            canvas_origin: origin,
                            canvas_size: available,
                            origin: [origin[0] + 10., origin[1] + 80.],
                            size: [width, width * 0.75],
                            dimensions: [640, 480],
                        },
                    );
                } else {
                    inspector.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                }
                if frame > 1 {
                    assert_eq!(ui.scroll_max_x(), 0., "{name}");
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
            size: 768 * 768 * 4,
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
                    bytes_per_row: Some(3072),
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
        if viewport {
            let controls = data
                .chunks_exact(4)
                .enumerate()
                .filter(|(i, p)| {
                    i / 768 > 70 && p[0] > 150 && u16::from(p[0]) > u16::from(p[1]) * 3 / 2
                })
                .count();
            assert!(controls > 100, "{name}: shape controls must be visible");
        }
        let mut ppm = b"P6\n768 768\n255\n".to_vec();
        for p in data.chunks_exact(4) {
            ppm.extend_from_slice(&p[..3]);
        }
        std::fs::write(format!("/tmp/fold-roto-ui/{name}.ppm"), ppm).unwrap();
    }
}

#[test]
fn roto_shape_keys_are_complete_scoped_and_preserve_last_pose() {
    assert!(Editor::default().auto_key);
    let mut settings = Roto::default();
    let a = crate::roto::Layer::rectangle([10., 20.], [50., 60.], false);
    let shape = a.id;
    let b = crate::roto::Layer::rectangle([1., 2.], [5., 6.], false);
    let other = b.id;
    settings.root.children = vec![a, b];
    let mut node = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: settings.clone(),
    });
    keys::set(&mut node, &settings, shape, Time::ZERO, false).unwrap();
    let later = Time::new(10, 24).unwrap();
    let mut changed = settings.clone();
    changed
        .root
        .find_mut(shape)
        .unwrap()
        .shape
        .as_mut()
        .unwrap()
        .points[0]
        .position[0] = 30.;
    edit(&mut node, &settings, changed.clone(), later, true).unwrap();
    let shape_properties = changed
        .properties()
        .into_iter()
        .filter(|p| keys::geometry(&p.id) == Some(shape));
    for p in shape_properties {
        for (i, value) in p.values.iter().enumerate() {
            let curve = node
                .animation
                .curve(&crate::animation::path(&p.id, i))
                .unwrap()
                .unwrap();
            assert_eq!(curve.keys.len(), 2);
            assert_eq!(curve.at(later).unwrap().value, *value);
        }
    }
    assert!(keys::times(&node, other).unwrap().is_empty());
    assert!(
        node.animation
            .curve(&format!("roto/{shape}/translate.0"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        keys::times(&node, shape)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
        [Time::ZERO, later]
    );
    let before = node.clone();
    changed
        .root
        .find_mut(shape)
        .unwrap()
        .shape
        .as_mut()
        .unwrap()
        .points[1]
        .position[0] += 2.;
    assert!(
        edit(
            &mut node,
            &settings,
            changed.clone(),
            Time::new(5, 24).unwrap(),
            false
        )
        .is_err()
    );
    assert_eq!(node, before);
    keys::set(&mut node, &changed, shape, Time::ZERO, true).unwrap();
    keys::set(&mut node, &changed, shape, later, true).unwrap();
    assert!(keys::times(&node, shape).unwrap().is_empty());
    let crate::OperationConfig::Roto { settings: retained } =
        node.prepare().unwrap().config().clone()
    else {
        unreachable!()
    };
    assert_eq!(
        retained.root.find(shape).unwrap().shape,
        changed.root.find(shape).unwrap().shape
    );
    // Newly drawn shapes receive their first full key at the drawing time.
    let empty = Roto {
        root: crate::roto::Layer {
            children: vec![],
            ..settings.root.clone()
        },
        ..settings.clone()
    };
    let mut fresh = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: empty.clone(),
    });
    edit(&mut fresh, &empty, settings.clone(), later, true).unwrap();
    assert_eq!(keys::times(&fresh, shape).unwrap(), [later].into());
    // Public animation channels expose these same keys for timeline editing.
    let output = crate::Node::new(crate::OperationConfig::Output, vec![fresh.id]);
    let graph = crate::Composite {
        info: fold_media::VideoInfo {
            width: 64,
            height: 64,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![fresh, output],
        extensions: Default::default(),
    };
    let document = graph.document(fold_foundation::DocumentId::new()).unwrap();
    let channels = crate::keyed_parameters::channels(&document, None).unwrap();
    assert!(
        channels
            .iter()
            .any(|c| keys::geometry(&c.property) == Some(shape) && c.curve.at(later).is_some())
    );
}

#[test]
fn roto_key_buttons_navigate_add_remove_and_undo() {
    use fold_platform::desktop::DesktopClient;
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, mut state) = setup();
    let document = state.document.unwrap();
    let mut graph = state.graph.take().unwrap();
    let mut settings = Roto::default();
    let shape = crate::roto::Layer::rectangle([1., 1.], [10., 10.], false);
    let layer = shape.id;
    settings.root.children.push(shape);
    let mut node = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: settings.clone(),
    });
    let id = node.id;
    for time in [Time::ZERO, Time::new(10, 24).unwrap()] {
        keys::set(&mut node, &settings, layer, time, false).unwrap();
    }
    graph.nodes.push(node);
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    state.sync(&mut host);
    let mut editor = Editor {
        target: Some(id),
        layer: Some(layer),
        ..Default::default()
    };
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([400., 200.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut centers = [[0.; 2]; 3];
    let mut draw = |context: &mut imgui::Context,
                    host: &mut crate::ui::tests::Host,
                    state: &mut state::State| {
        let time = Time::new(i64::from(host.state().frame), 24).unwrap();
        let sampled = state.roto_at(id, time).unwrap();
        let ui = context.frame();
        ui.window("Shape keys")
            .position([0., 0.], imgui::Condition::Always)
            .size([280., 180.], imgui::Condition::Always)
            .build(|| {
                let p = ui.cursor_screen_pos();
                let h = ui.frame_height();
                for (i, center) in centers.iter_mut().enumerate() {
                    *center = [
                        p[0] + (h + 4.) / 2.
                            + i as f32 * (h + 4. + ui.clone_style().item_spacing()[0]),
                        p[1] + h / 2.,
                    ];
                }
                keys::row(ui, &mut editor, state, host, id, &sampled, time);
                assert_eq!(ui.scroll_max_x(), 0.);
            });
        let _ = context.render_legacy();
        centers
    };
    let mut positions = draw(&mut context, &mut host, &mut state);
    for button in [2, 1, 0, 1, 1] {
        context.io_mut().add_mouse_pos_event(positions[button]);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, true);
        draw(&mut context, &mut host, &mut state);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, false);
        positions = draw(&mut context, &mut host, &mut state);
    }
    assert_eq!(host.state().frame, 0);
    assert_eq!(
        keys::times(state.graph.as_ref().unwrap().node(id).unwrap(), layer).unwrap(),
        [Time::ZERO].into()
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert!(
        keys::times(state.graph.as_ref().unwrap().node(id).unwrap(), layer)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn roto_open_pen_keeps_anchor_and_live_guide_visible() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 500.]);
    context.io_mut().set_delta_time(1. / 60.);
    for tool in [Tool::Bezier, Tool::BSpline] {
        let (mut host, mut state) = setup();
        let document = state.document.unwrap();
        let mut graph = state.graph.take().unwrap();
        graph.info.width = 100;
        graph.info.height = 100;
        let node = crate::Node::disconnected(crate::OperationConfig::Roto {
            settings: Roto::default(),
        });
        let id = node.id;
        graph.nodes.push(node);
        host.project
            .commit(EditBatch {
                base: host.project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
            })
            .unwrap();
        state.sync(&mut host);
        state.selected = vec![id];
        let mut editor = Editor {
            target: Some(id),
            tool,
            ..Default::default()
        };
        let before = host.project.snapshot().revision();
        for (index, (mouse, down)) in [
            ([500., 490.], false),
            ([500., 490.], false),
            ([110., 160.], true),
            ([110., 160.], false),
            ([230., 240.], false),
            ([230., 240.], true),
            ([230., 240.], false),
            ([330., 180.], true),
            ([330., 180.], false),
            ([110., 160.], false),
            ([110., 160.], true),
            ([110., 160.], false),
        ]
        .into_iter()
        .enumerate()
        {
            context.io_mut().add_mouse_pos_event(mouse);
            context
                .io_mut()
                .add_mouse_button_event(imgui::MouseButton::Left, down);
            let ui = context.frame();
            ui.window("Pen repro")
                .position([0., 0.], imgui::Condition::Always)
                .size([500., 490.], imgui::Condition::Always)
                .build(|| {
                    editor.draw(
                        ExtensionUi {
                            ui,
                            host: &mut host,
                        },
                        ViewerRect {
                            editable: true,
                            image_current: false,
                            canvas_origin: [10., 30.],
                            canvas_size: [480., 450.],
                            origin: [50., 100.],
                            size: [300., 300.],
                            dimensions: [100, 100],
                        },
                        &mut state,
                    );
                });
            let frame = context.render_legacy();
            if index == 4 {
                assert_eq!(
                    editor
                        .drawing
                        .as_ref()
                        .unwrap()
                        .shape
                        .as_ref()
                        .unwrap()
                        .points
                        .len(),
                    1
                );
                let vertices: Vec<_> = frame
                    .draw_data()
                    .draw_lists()
                    .flat_map(|list| list.vtx_buffer().iter())
                    .collect();
                let near = |p: [f32; 2], radius: f32| {
                    vertices
                        .iter()
                        .filter(|v| {
                            (v.pos[0] - p[0]).abs() < radius && (v.pos[1] - p[1]).abs() < radius
                        })
                        .count()
                };
                assert!(
                    near([110., 160.], 9.) >= 4,
                    "open pen must draw its first anchor before closure"
                );
                assert!(
                    near([230., 240.], 9.) >= 2,
                    "open pen must draw a live guide toward the cursor"
                );
                assert_eq!(
                    host.project.snapshot().revision(),
                    before,
                    "drawing guides must not commit keys or render requests"
                );
            }
            if index == 9 {
                let count = frame
                    .draw_data()
                    .draw_lists()
                    .flat_map(|l| l.vtx_buffer().iter())
                    .filter(|v| {
                        let d = (v.pos[0] - 110.).hypot(v.pos[1] - 160.);
                        (8. ..=12.).contains(&d)
                    })
                    .count();
                assert!(
                    count > 8,
                    "hovering the first point must show the enlarged close target"
                );
            }
        }
        assert!(
            editor.drawing.is_none(),
            "clicking the close target must finish the shape"
        );
        let node = state.graph.as_ref().unwrap().node(id).unwrap();
        let crate::OperationConfig::Roto { settings } = node.prepare().unwrap().config().clone()
        else {
            unreachable!()
        };
        assert!(settings.root.children[0].shape.as_ref().unwrap().closed);
        assert_eq!(
            keys::times(node, settings.root.children[0].id).unwrap(),
            [Time::ZERO].into()
        );
        host.project.undo().unwrap();
        assert_eq!(
            crate::Composite::from_document(&host.project.snapshot().state().documents[&document])
                .unwrap(),
            graph,
            "closure and first keys must be one undo step"
        );
    }
}

#[test]
#[ignore = "focused keyed Roto CPU timing"]
fn roto_keyed_evaluation_cost() {
    let mut settings = Roto::default();
    let points = (0..20)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 20.;
            crate::roto::Point::new([50. + a.cos() * 30., 50. + a.sin() * 30.])
        })
        .collect();
    let shape = crate::roto::Layer::curve(crate::roto::Spline::Bezier, points);
    let layer = shape.id;
    settings.root.children.push(shape);
    let mut node = crate::Node::disconnected(crate::OperationConfig::Roto {
        settings: settings.clone(),
    });
    keys::set(&mut node, &settings, layer, Time::ZERO, false).unwrap();
    let id = node.id;
    let output = crate::Node::new(crate::OperationConfig::Output, vec![id]);
    let graph = crate::Composite {
        info: fold_media::VideoInfo {
            width: 100,
            height: 100,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![node, output],
        extensions: Default::default(),
    };
    let prepared = graph.prepare().unwrap();
    let mut elapsed = vec![];
    for i in 0..30 {
        let start = std::time::Instant::now();
        prepared
            .evaluated_parameters(id, Time::new(i, 24).unwrap())
            .unwrap();
        elapsed.push(start.elapsed().as_micros());
    }
    elapsed.sort();
    eprintln!(
        "20-point fully keyed Roto: median evaluation {} us",
        elapsed[elapsed.len() / 2]
    );
}
