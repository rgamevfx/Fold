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
    let crate::OperationConfig::Roto { settings } =
        graph.node(id).unwrap().prepare().unwrap().config().clone()
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
    let node = crate::Node::disconnected(crate::OperationConfig::Roto { settings });
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
    ] {
        for frame in 0..4 {
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
