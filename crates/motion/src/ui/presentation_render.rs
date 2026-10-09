//! Native renderer captures for visual review of Inspector controls.
use super::*;
use sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-presentation.ppm"]
fn presentation_inspector_capture() {
    capture(Capture::Presentation);
}
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-literals.ppm"]
fn literal_inspector_capture() {
    capture(Capture::Literals);
}
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-instances.ppm"]
fn instance_inspector_capture() {
    capture(Capture::Instances);
}
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-typed-keys.ppm"]
fn typed_key_inspector_capture() {
    capture(Capture::TypedKeys);
}
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-override-keys.ppm"]
fn override_key_inspector_capture() {
    capture(Capture::OverrideKeys);
}
#[test]
#[ignore = "requires GPU; writes /tmp/fold-motion-instance-menu.ppm"]
fn instance_menu_capture() {
    capture(Capture::InstanceMenu);
}
#[derive(Clone, Copy)]
enum Capture {
    Presentation,
    Literals,
    Instances,
    TypedKeys,
    OverrideKeys,
    InstanceMenu,
}
fn capture(kind: Capture) {
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let _fonts = sdk::typography::Typography::install(&mut context);
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
        width: 720,
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let document = crate::package::create(fold_foundation::DocumentId::new()).unwrap();
    let original = crate::model::Construction::from_document(&document).unwrap();
    let definition = original.root.definition;
    let node = original.definition(definition).unwrap().nodes[0].id;
    let mut value = presentation::group(&original, definition, &[node]).unwrap();
    value.presentation.groups[0].name = "Title layout".into();
    value
        .presentation
        .annotations
        .insert(node, "Align with the title baseline.".into());
    let group = value.presentation.groups[0].id;
    value
        .presentation
        .annotations
        .insert(group, "Shared layout controls".into());
    if matches!(
        kind,
        Capture::Instances | Capture::OverrideKeys | Capture::InstanceMenu
    ) {
        value.root.clock.offset = fold_foundation::Time::new(1001, 24000).unwrap();
        value.root.clock.rate = crate::types::Ratio::new(-1001, 1000).unwrap();
        let input = value.definitions[0].inputs[0].id;
        value.root.overrides.insert(
            input,
            crate::model::Parameter::new(Binding::Literal {
                value: Literal::Distance {
                    value: 160.,
                    unit: crate::types::Unit::Canvas,
                },
            })
            .unwrap(),
        );
        for (name, default, protected) in [
            (
                "Title",
                Literal::Text {
                    value: "Opening title".into(),
                },
                false,
            ),
            ("Copies", Literal::Integer { value: 3 }, true),
        ] {
            value.definitions[0].inputs.push(crate::model::Input {
                id: fold_foundation::ObjectId::new(),
                name: name.into(),
                ty: default.ty(),
                default,
                protected,
                extensions: Default::default(),
            });
        }
    }
    let vector = |y| Literal::Vector {
        value: vec![0., y],
        unit: crate::types::Unit::Canvas,
        space: crate::types::Space::Screen,
    };
    let mut typed_track = crate::binding::Track {
        id: fold_foundation::ObjectId::new(),
        ty: vector(0.).ty(),
        combination: crate::binding::Combination::Replace,
        base: None,
        extensions: Default::default(),
        keys: [
            (fold_foundation::Time::ZERO, 80.),
            (fold_foundation::Time::new(3, 5).unwrap(), 0.),
        ]
        .into_iter()
        .map(|(time, y)| crate::binding::Key {
            id: fold_foundation::ObjectId::new(),
            time,
            value: vector(y),
            interpolation: crate::binding::Interpolation::Linear,
            extensions: Default::default(),
        })
        .collect(),
    };
    if matches!(kind, Capture::OverrideKeys) {
        let input = value.definitions[0].inputs[0].id;
        let mut track = typed_track.clone();
        track.ty = value.definitions[0].inputs[0].ty.clone();
        for (index, key) in track.keys.iter_mut().enumerate() {
            key.value = Literal::Distance {
                value: 160. + index as f64 * 80.,
                unit: crate::types::Unit::Canvas,
            };
        }
        value.root.overrides.get_mut(&input).unwrap().value =
            serde_json::to_value(Binding::Track { track }).unwrap();
    }
    if matches!(kind, Capture::TypedKeys) {
        typed_track.combination = crate::binding::Combination::Add;
        typed_track.base = Some(vector(100.));
    }
    let unchanged_track = typed_track.clone();
    let unchanged = value.clone();
    context.io_mut().add_mouse_pos_event([-100., -100.]);
    context.io_mut().set_display_size([720., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut controls = vec![
        (
            "Copies",
            Literal::Integer {
                value: i64::MAX - 42,
            },
        ),
        (
            "Title",
            Literal::Text {
                value: "Opening title".into(),
            },
        ),
        (
            "Duration",
            Literal::Duration {
                value: fold_foundation::Time::new(1001, 24000).unwrap(),
            },
        ),
        (
            "Fill",
            Literal::Color {
                value: [-0.2, 3., 0.5, 0.8],
                encoding: crate::types::Encoding::AcesCg,
                alpha: crate::types::Alpha::Straight,
            },
        ),
    ];
    controls.push((
        "Orientation",
        Literal::Rotation {
            value: [0.5, 0.5, 0.5, 0.5],
            space: crate::types::Space::World,
        },
    ));
    controls.push((
        "Asset",
        Literal::AssetRef {
            value: crate::types::AssetReference::Definition(definition),
        },
    ));
    let original_controls = controls.clone();
    let mut literal_controls = literal::Controls::default();
    literal_controls.assets = vec![(definition, "Title artwork".into())];
    for _ in 0..3 {
        let ui = context.frame();
        for (title, x, width) in [("Inspector", 0., 400.), ("Narrow inspector", 420., 260.)] {
            ui.window(title)
                .position([x, 0.], sdk::imgui::Condition::Always)
                .size([width, 780.], sdk::imgui::Condition::Always)
                .build(|| {
                    let mut response = EditResponse::default();
                    let _width = ui.push_item_width((ui.content_region_avail()[0] * 0.43).max(60.));
                    if matches!(kind, Capture::Literals) {
                        for (index, (label, value)) in controls.iter_mut().enumerate() {
                            let _id = ui.push_id(&index.to_string());
                            literal(ui, label, value, &mut response, &mut literal_controls);
                        }
                    } else if matches!(kind, Capture::TypedKeys) {
                        keyframes::draw(
                            ui,
                            "Position",
                            &mut typed_track,
                            Some(fold_foundation::Time::new(1, 4).unwrap()),
                            &mut response,
                            &mut literal_controls,
                        );
                    } else if matches!(
                        kind,
                        Capture::Instances | Capture::OverrideKeys | Capture::InstanceMenu
                    ) {
                        let inputs = value.definitions[0].inputs.clone();
                        let locked = crate::protection::locked_ids(&value).unwrap();
                        if matches!(kind, Capture::InstanceMenu) && x > 0. {
                            let _instance = ui.push_id(&value.root.id.to_string());
                            let _input = ui.push_id(&inputs[0].id.to_string());
                            ui.open_popup("input-options");
                        }
                        instance::draw(
                            ui,
                            &mut value.root,
                            &inputs,
                            &locked,
                            None,
                            &mut response,
                            &mut literal_controls,
                        );
                    } else {
                        presentation::draw(ui, &mut value, definition, &[node], &mut response);
                    }
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
    let row = (size.width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row) * u64::from(size.height),
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
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range();
    let mut ppm = format!("P6\n{} {}\n255\n", size.width, size.height).into_bytes();
    for line in bytes.chunks_exact(row as usize) {
        for pixel in line[..size.width as usize * 4].chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
    }
    assert_eq!(controls, original_controls);
    assert_eq!(value, unchanged);
    assert_eq!(typed_track, unchanged_track);
    std::fs::write(
        match kind {
            Capture::Literals => "/tmp/fold-motion-literals.ppm",
            Capture::Presentation => "/tmp/fold-motion-presentation.ppm",
            Capture::Instances => "/tmp/fold-motion-instances.ppm",
            Capture::TypedKeys => "/tmp/fold-motion-typed-keys.ppm",
            Capture::OverrideKeys => "/tmp/fold-motion-override-keys.ppm",
            Capture::InstanceMenu => "/tmp/fold-motion-instance-menu.ppm",
        },
        ppm,
    )
    .unwrap();
}
