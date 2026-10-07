#![cfg(feature = "native-probe")]
//! Actual Composition channel discovery, editing and native normal/narrow captures.
use fold_app::{packages, session::Session};
use fold_composition::{Composition, Layer};
use fold_foundation::{DocumentId, ObjectId, Time};
static UI: std::sync::Mutex<()> = std::sync::Mutex::new(());
use fold_platform::{
    desktop::*,
    image_source::Source,
    panel_context::PanelContext,
    workspace::{EditorInstance, LinkGroup},
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{ExtensionUi, PanelRegistry, imgui, typography::Typography};
#[test]
#[ignore = "native renderer; writes /tmp/fold-composition-animation-ui"]
fn tracks_and_curves_edit_the_same_graph_keys() {
    let id = DocumentId::new();
    let graph_id = DocumentId::new();
    let mut motion = fold_motion::Motion::new_scene();
    let object =
        fold_motion::authoring::scene::create_object(&mut motion, "fold.motion.rectangle").unwrap();
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 120.);
    curve.insert(Time::new(2, 1).unwrap(), 300.);
    curve.keys[0].interpolation = fold_animation::Interpolation::Bezier;
    fold_motion::authoring::node(&mut motion, object)
        .unwrap()
        .animation
        .insert("width.0".into(), curve);
    let mut source = DocumentRef {
        document: graph_id,
        output: "video".into(),
        extensions: Default::default(),
    };
    source.extensions.insert(
        fold_composition::graph::OBJECT.into(),
        serde_json::to_value(object).unwrap(),
    );
    let mut layer = Layer::new(
        "Rectangle",
        Some(Source::Output(source)),
        motion.info.clone(),
    )
    .unwrap();
    layer.origin = Time::new(1, 2).unwrap();
    layer.source_start = Time::new(1, 4).unwrap();
    let layer_id = layer.id;
    let composition = Composition {
        info: motion.info.clone(),
        layers: vec![layer],
        extensions: Default::default(),
    };
    let mut document = motion.document(graph_id).unwrap();
    document
        .extensions
        .insert(fold_composition::LAYER_GRAPH.into(), true.into());
    let mut project = Project::new(32);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(document),
                Mutation::PutDocument(composition.document(id).unwrap()),
            ],
        })
        .unwrap();
    exercise(
        project,
        id,
        graph_id,
        object,
        layer_id,
        "width.0",
        187.5,
        [93., 327.],
        Time::new(11, 12).unwrap(),
        "rectangle",
    );
}
#[test]
#[ignore = "native renderer; exercises the saved Rust Orbit demo"]
fn rust_orbit_saved_project_keys_work_in_tracks_and_curves() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/fold-rust-orbit.fold");
    let project = fold_project::load(&path, 32).unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_composition::COMPOSITION)
        .expect("Orbit must open as a Composition");
    let composition = Composition::from_document(document).unwrap();
    let layer = &composition.layers[0];
    let Some(Source::Output(source)) = &layer.source else {
        panic!("Orbit must retain its graph");
    };
    assert_eq!(
        snapshot.state().documents[&source.document]
            .extensions
            .get(fold_composition::LAYER_GRAPH),
        Some(&serde_json::Value::Bool(true)),
        "Orbit source must follow Composition layer graph navigation"
    );
    let channels = packages::builtins()
        .animation_channels(
            &snapshot.state().documents[&source.document],
            layer.graph_object(),
        )
        .unwrap();
    assert_eq!(
        channels
            .iter()
            .map(|c| c.target.path.as_str())
            .collect::<Vec<_>>(),
        ["radius.0", "speed.0"]
    );
    exercise(
        project,
        document.id,
        source.document,
        layer.graph_object().unwrap(),
        layer.id,
        "radius.0",
        120.,
        [-17.885, 137.985],
        Time::new(7, 6).unwrap(),
        "orbit",
    );
}
#[allow(clippy::too_many_arguments)]
fn exercise(
    project: Project,
    id: DocumentId,
    graph_id: DocumentId,
    object: ObjectId,
    layer_id: ObjectId,
    path: &str,
    value: f64,
    bounds: [f64; 2],
    moved_time: Time,
    artifact: &str,
) {
    let _guard = UI.lock().unwrap();
    let mut host = Session::new(project);
    let mut registry = PanelRegistry::new(&packages::builtins());
    fold_composition::ui::register(&mut registry).unwrap();
    let mut layers = registry
        .probe_editor(fold_composition::COMPOSITION)
        .unwrap();
    let mut parent = EditorInstance {
        contribution: fold_composition::package::PANEL.into(),
        document_type: fold_composition::COMPOSITION.into(),
        group: LinkGroup::A,
        navigation: vec![ViewLocation {
            document: id,
            time: Time::new(1, 1).unwrap(),
            label: "Composition".into(),
        }],
        selection: Selection {
            document: Some(id),
            objects: vec![layer_id],
        },
        mapped_navigation: false,
    };
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
    let size = wgpu::Extent3d {
        width: 1280,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Composition animation workflow"),
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
    std::fs::create_dir_all("/tmp/fold-composition-animation-ui").unwrap();
    let before = host.snapshot().unwrap();
    let mut button = [0.; 2];
    let mut curve_key = [0.; 2];
    for (size_name, width, toggle, insert) in [
        ("tracks-normal", 900., false, true),
        ("curves-normal", 900., true, false),
        ("curves-narrow", 280., false, false),
        ("tracks-narrow", 280., true, false),
    ] {
        for frame_index in 0..12 {
            context
                .io_mut()
                .add_key_event(imgui::Key::I, insert && frame_index == 4);
            if toggle && frame_index < 6 {
                context.io_mut().add_mouse_pos_event(button);
                context
                    .io_mut()
                    .add_mouse_button_event(imgui::MouseButton::Left, frame_index == 3);
            }
            if size_name == "curves-normal" && (6..=9).contains(&frame_index) {
                context.io_mut().add_mouse_pos_event(if frame_index >= 8 {
                    [curve_key[0] + 20., curve_key[1] - 30.]
                } else {
                    curve_key
                });
                context.io_mut().add_mouse_button_event(
                    imgui::MouseButton::Left,
                    frame_index == 7 || frame_index == 8,
                );
            } else if frame_index >= 6 {
                context.io_mut().add_mouse_pos_event([-100., -100.]);
            }
            let ui = context.frame();
            ui.window("Composition")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 790.], imgui::Condition::Always)
                .focused(true)
                .build(|| {
                    let origin = ui.cursor_screen_pos();
                    button = [
                        origin[0] + 8.,
                        origin[1] + ui.frame_height_with_spacing() + 8.,
                    ];
                    let mut scoped = PanelContext::new(&mut host, &mut parent);
                    layers.editor.draw(ExtensionUi {
                        ui,
                        host: &mut scoped,
                    });
                    if frame_index == 5 && size_name == "curves-normal" {
                        let min = ui.item_rect_min();
                        let max = ui.item_rect_max();
                        let body_y = min[1] + ui.text_line_height() + 6.;
                        let height = max[1] - body_y - 16.;
                        let ppframe = (max[0] - min[0] - 210. - 8.) / 120.;
                        curve_key = [
                            min[0] + 210. + 24. * ppframe,
                            max[1]
                                - 8.
                                - ((value - bounds[0]) / (bounds[1] - bounds[0])) as f32 * height,
                        ];
                    }
                    if frame_index >= 3 {
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
            format!("/tmp/fold-composition-animation-ui/{artifact}-{size_name}.ppm"),
            ppm,
        )
        .unwrap();
    }
    let after = host.snapshot().unwrap();
    assert_eq!(
        after.revision().0,
        before.revision().0 + 2,
        "insertion and curve drag each commit once; switching views does not edit"
    );
    let channels = host.animation_channels(graph_id, Some(object)).unwrap();
    let channel = channels.iter().find(|c| c.target.path == path).unwrap();
    assert_eq!(
        channel.curve.keys.len(),
        3,
        "I keys the graph parameter from Composition"
    );
    let middle = &channel.curve.keys[1];
    assert_eq!(
        middle.time, moved_time,
        "curve drag maps back to exact graph source time"
    );
    assert!(
        middle.value > value,
        "curve drag changes graph parameter value"
    );
    if artifact == "orbit" {
        let output = DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        };
        let render = |snapshot: &fold_project::Snapshot| {
            fold_render::render(
                packages::builtins()
                    .video(snapshot, &output, Time::new(1, 1).unwrap(), 640, 360)
                    .unwrap(),
            )
            .unwrap()
            .pixels()
            .to_vec()
        };
        assert_ne!(
            render(&before),
            render(&after),
            "Orbit's key edit must change rendered artwork"
        );
        let saved = std::path::Path::new("/tmp/fold-composition-animation-ui/orbit-edited.fold");
        fold_project::save(&after, saved).unwrap();
        let reopened = fold_project::load(saved, 32).unwrap();
        assert_eq!(
            render(&after),
            render(&reopened.snapshot()),
            "edited Orbit survives save/reopen"
        );
    }
    host.command(DesktopCommand::Undo);
    host.command(DesktopCommand::Undo);
    assert!(before.same_content(&host.snapshot().unwrap()));
    host.command(DesktopCommand::Redo);
    host.command(DesktopCommand::Redo);
    assert!(after.same_content(&host.snapshot().unwrap()));
}
