#![cfg(feature = "native-probe")]
//! Exercise the real registered panels and saved Badge Wave graph scopes.
use fold_app::{packages, session::Session};
use fold_composition::Composition;
use fold_foundation::Time;
use fold_platform::{
    desktop::*,
    panel_context::PanelContext,
    workspace::{EditorInstance, LinkGroup},
};
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{ExtensionUi, PanelRegistry, imgui, typography::Typography};

#[test]
#[ignore = "native renderer; writes /tmp/fold-badge-layer-ui"]
fn badge_hierarchy_and_actual_layer_graphs_normal_and_narrow() {
    let project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/fold-badge-live.fold"),
        32,
    )
    .unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.type_id == fold_composition::COMPOSITION
                && Composition::from_document(d).unwrap().layers.len() == 5
        })
        .unwrap();
    let composition = Composition::from_document(document).unwrap();
    let mut host = Session::new(project);
    let mut registry = PanelRegistry::new(&packages::builtins());
    fold_composition::ui::register(&mut registry).unwrap();
    fold_motion::ui::register(&mut registry).unwrap();
    fold_compositor::ui::register(&mut registry).unwrap();
    let mut layers = registry
        .probe_editor(fold_composition::COMPOSITION)
        .unwrap();
    let mut motion = registry.probe_editor(fold_motion::MOTION).unwrap();
    let mut compositor = registry.probe_editor(fold_compositor::COMPOSITE).unwrap();
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
    layers.editor.initialize(&context, Some(&fonts));
    motion.editor.initialize(&context, Some(&fonts));
    compositor.editor.initialize(&context, Some(&fonts));
    let size = wgpu::Extent3d {
        width: 1280,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Badge layer workflow"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    context.io_mut().set_display_size([1280., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-badge-layer-ui").unwrap();
    let mut graph = None;
    let finish = snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.type_id == fold_composition::COMPOSITION
                && Composition::from_document(d).unwrap().layers.len() == 1
        })
        .unwrap();
    let finish_composition = Composition::from_document(finish).unwrap();
    for (document, composition, index, label) in [
        (document, &composition, 0, "duplicator"),
        (document, &composition, 1, "badge"),
        (document, &composition, 2, "text"),
        (finish, &finish_composition, 0, "compositing"),
    ] {
        let layer = &composition.layers[index];
        for (size_name, width) in [("normal", 820.), ("narrow", 280.)] {
            let mut parent = EditorInstance {
                contribution: fold_composition::package::PANEL.into(),
                document_type: fold_composition::COMPOSITION.into(),
                group: LinkGroup::A,
                navigation: vec![ViewLocation {
                    document: document.id,
                    time: Time::new(13, 4).unwrap(),
                    label: "Badge Wave".into(),
                }],
                selection: Selection {
                    document: Some(document.id),
                    objects: vec![layer.id],
                },
                mapped_navigation: false,
            };
            for frame_index in 0..40 {
                let ui = context.frame();
                ui.window("Composition")
                    .position([0., 0.], imgui::Condition::Always)
                    .size([width, 300.], imgui::Condition::Always)
                    .build(|| {
                        let mut scoped = PanelContext::new(&mut host, &mut parent);
                        layers.editor.draw(ExtensionUi {
                            ui,
                            host: &mut scoped,
                        });
                        if let Some(request) = scoped.take_layer_graph() {
                            graph = Some(request);
                        }
                        assert_eq!(scoped.state().selection.objects, vec![layer.id]);
                        if frame_index >= 3 {
                            assert_eq!(ui.scroll_max_x(), 0.);
                        }
                    });
                let graph = graph
                    .as_mut()
                    .expect("Layer selection opens the native graph");
                assert_eq!(
                    graph.selection.objects,
                    layer.graph_object().into_iter().collect::<Vec<_>>()
                );
                let native = if graph.document_type == fold_motion::MOTION {
                    &mut motion
                } else {
                    &mut compositor
                };
                ui.window("Network")
                    .position([0., 308.], imgui::Condition::Always)
                    .size([width, 482.], imgui::Condition::Always)
                    .build(|| {
                        let mut scoped = PanelContext::new(&mut host, graph);
                        native.editor.draw_network(ExtensionUi {
                            ui,
                            host: &mut scoped,
                        });
                    });
                assert_eq!(
                    layers.editor.layer_for_graph(
                        graph.document().unwrap(),
                        native.editor.selected_layer_object()
                    ),
                    Some((layer.id, composition.locked(layer.id)))
                );
                ui.window("Inspector")
                    .position([width + 8., 0.], imgui::Condition::Always)
                    .size([360., 790.], imgui::Condition::Always)
                    .build(|| {
                        let mut scoped = PanelContext::new(&mut host, &mut parent);
                        layers.inspector.draw(ExtensionUi {
                            ui,
                            host: &mut scoped,
                        });
                        ui.separator();
                        let mut scoped = PanelContext::new(&mut host, graph);
                        native.inspector.draw(ExtensionUi {
                            ui,
                            host: &mut scoped,
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
            std::fs::write(
                format!("/tmp/fold-badge-layer-ui/{label}-{size_name}.ppm"),
                ppm,
            )
            .unwrap();
        }
    }
    assert!(
        snapshot.same_content(&host.snapshot().unwrap()),
        "Viewing layers must not change the project"
    );
}
