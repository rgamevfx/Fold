//! Actual inspector controls rendered through the shared native backend.
use super::*;
use crate::{ChannelMapping, ChannelSource, Node, OperationConfig};
use fold_project::{EditBatch, Mutation};
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{ExtensionUi, Panel, imgui, typography::Typography};
#[test]
#[ignore = "native UI capture; writes /tmp/fold-shuffle-ui"]
fn shuffle_inspector_normal_and_narrow() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Shuffle capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let _fonts = Typography::install(&mut context);
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
        width: 512,
        height: 480,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Shuffle review"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let (mut host, initial) = tests::setup();
    let document = initial.document.unwrap();
    let mut graph = initial.graph.unwrap();
    let solid = graph.nodes[0].id;
    let mapping = |from: &str, to: &str| {
        ["red", "green", "blue", "alpha"]
            .into_iter()
            .map(|c| ChannelMapping {
                destination: format!("{to}.{c}").try_into().unwrap(),
                source: ChannelSource::A(format!("{from}.{c}").try_into().unwrap()),
            })
            .collect::<Vec<_>>()
    };
    let mut layers = Node::disconnected(OperationConfig::Shuffle {
        mappings: mapping("rgba", "diffuse")
            .into_iter()
            .chain(mapping("rgba", "specular"))
            .collect(),
    });
    layers.set_input(0, solid).unwrap();
    let mut shuffle = Node::disconnected(OperationConfig::Shuffle {
        mappings: mapping("diffuse", "rgba")
            .into_iter()
            .chain(mapping("specular", "reflection"))
            .collect(),
    });
    shuffle.set_input(0, layers.id).unwrap();
    let selected = shuffle.id;
    graph.nodes.extend([layers, shuffle]);
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let shared = Rc::new(RefCell::new(state::State::default()));
    let mut inspector = inspector::Inspector(
        shared.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    shared.borrow_mut().sync(&mut host);
    shared.borrow_mut().selected = vec![selected];
    context.io_mut().set_display_size([512., 480.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-shuffle-ui").unwrap();
    let mut sources = super::source::Sources::default();
    for (name, width) in [
        ("grade-driven-normal", 420.),
        ("grade-driven-narrow", 280.),
        ("grade-normal", 420.),
        ("grade-narrow", 280.),
        ("grade-split-normal", 420.),
        ("grade-split-narrow", 280.),
        ("transform-normal", 420.),
        ("transform-narrow", 280.),
        ("transform-shutter-normal", 420.),
        ("transform-shutter-narrow", 280.),
        ("blur-normal", 420.),
        ("blur-narrow", 280.),
        ("source-normal", 420.),
        ("source-narrow", 280.),
        ("normal", 420.),
        ("narrow", 280.),
        ("relative-normal", 420.),
        ("relative-narrow", 280.),
        ("shader-normal", 420.),
        ("shader-narrow", 280.),
        ("shader-interface-normal", 420.),
        ("shader-interface-narrow", 280.),
    ] {
        if std::env::var_os("FOLD_NODE_CAPTURE").is_some()
            && (name.starts_with("source")
                || name.starts_with("relative")
                || name.starts_with("shader"))
        {
            continue;
        }
        if name.starts_with("grade")
            || name.starts_with("transform")
            || name.starts_with("blur")
            || name.starts_with("relative")
            || name.starts_with("shader")
        {
            let mut graph = crate::Composite::from_document(
                &host.project.snapshot().state().documents[&document],
            )
            .unwrap();
            let parameters = if name.starts_with("grade") {
                OperationConfig::ColorGrade {
                    settings: Default::default(),
                }
            } else if name.starts_with("transform") {
                OperationConfig::TransformImage {
                    settings: crate::Transform {
                        pivot: [960., 540.],
                        ..Default::default()
                    },
                }
            } else if name.starts_with("blur") {
                OperationConfig::Blur {
                    size: [0.; 2],
                    edges: Default::default(),
                    filter: Default::default(),
                }
            } else if name.starts_with("relative") {
                OperationConfig::RelativeTransform {
                    settings: Default::default(),
                }
            } else {
                OperationConfig::Shader {
                    settings: Default::default(),
                }
            };
            let node = Node::new(parameters, vec![solid]);
            let id = node.id;
            if name.starts_with("grade-split") {
                for property in ["gain", "gamma"] {
                    inspector.3.insert((id, property.into()));
                }
            }
            graph.nodes.push(node);
            if name.starts_with("grade-driven") {
                let reference = graph
                    .prepare()
                    .unwrap()
                    .node(solid)
                    .unwrap()
                    .property_reference("color", 0)
                    .unwrap();
                graph
                    .node_mut(id)
                    .unwrap()
                    .animation
                    .set_binding(
                        "gain.0".into(),
                        fold_graph::binding::Binding::Link {
                            reference: reference.into(),
                        },
                    )
                    .unwrap();
                let prepared = graph.prepare().unwrap();
                let mut edited = graph.node(id).unwrap().clone();
                let property = prepared
                    .node(id)
                    .unwrap()
                    .animated_properties()
                    .into_iter()
                    .find(|p| p.id == "gain")
                    .unwrap();
                super::references::Context {
                    graph: &prepared,
                    document,
                    drafts: &mut inspector.4,
                }
                .action(
                    &mut edited,
                    &property,
                    0,
                    fold_foundation::Time::ZERO,
                    fold_ui::sdk::parameter_menu::Action::EditExpression,
                    &mut false,
                )
                .unwrap();
            }
            host.project
                .commit(EditBatch {
                    base: host.project.snapshot().revision(),
                    mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
                })
                .unwrap();
            shared.borrow_mut().sync(&mut host);
            shared.borrow_mut().selected = vec![id];
        }
        if name == "normal" || name == "narrow" {
            shared.borrow_mut().selected = vec![selected];
        }
        for frame_index in 0..8 {
            if name.starts_with("transform-shutter") && (frame_index == 1 || frame_index == 2) {
                context.io_mut().add_mouse_pos_event([45., 316.]);
                context
                    .io_mut()
                    .add_mouse_button_event(imgui::MouseButton::Left, frame_index == 1);
            }
            if name.starts_with("shader-interface") {
                if frame_index == 1 || frame_index == 2 {
                    context.io_mut().add_mouse_pos_event([30., 61.]);
                    context
                        .io_mut()
                        .add_mouse_button_event(imgui::MouseButton::Left, frame_index == 1);
                }
                if frame_index == 3 || frame_index == 4 {
                    context.io_mut().add_mouse_pos_event([45., 103.]);
                    context
                        .io_mut()
                        .add_mouse_button_event(imgui::MouseButton::Left, frame_index == 3);
                }
            }
            if frame_index == 5 {
                context.io_mut().add_mouse_pos_event([500., 470.]);
            }
            let ui = context.frame();
            ui.window("Node Inspector")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 470.], imgui::Condition::Always)
                .build(|| {
                    if name.starts_with("source") {
                        sources.draw(ui, &mut shared.borrow_mut(), &mut host);
                    } else {
                        inspector.draw(ExtensionUi {
                            ui,
                            host: &mut host,
                        });
                    }
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
            size: 512 * 480 * 4,
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
                    bytes_per_row: Some(2048),
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
        let mut ppm = b"P6\n512 480\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold-shuffle-ui/{name}.ppm"), ppm).unwrap();
    }
}
