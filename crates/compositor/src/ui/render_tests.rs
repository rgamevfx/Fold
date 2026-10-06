//! Actual inspector controls rendered through the shared native backend.
use super::*;
use crate::{ChannelMapping, ChannelSource, Node, Parameters};
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
    let mut layers = Node::disconnected(Parameters::Shuffle {
        mappings: mapping("rgba", "diffuse")
            .into_iter()
            .chain(mapping("rgba", "specular"))
            .collect(),
    });
    layers.inputs[0] = Some(solid);
    let mut shuffle = Node::disconnected(Parameters::Shuffle {
        mappings: mapping("diffuse", "rgba")
            .into_iter()
            .chain(mapping("specular", "reflection"))
            .collect(),
    });
    shuffle.inputs[0] = Some(layers.id);
    let selected = shuffle.id;
    graph.nodes.extend([layers, shuffle]);
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(document).unwrap())],
        })
        .unwrap();
    let shared = Rc::new(RefCell::new(state::State::default()));
    let mut inspector = inspector::Inspector(shared.clone(), Default::default());
    shared.borrow_mut().sync(&mut host);
    shared.borrow_mut().selected = vec![selected];
    context.io_mut().set_display_size([512., 480.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-shuffle-ui").unwrap();
    for (name, width) in [("normal", 420.), ("narrow", 280.)] {
        for frame_index in 0..5 {
            let ui = context.frame();
            ui.window("Node Inspector")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 470.], imgui::Condition::Always)
                .build(|| {
                    inspector.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
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
