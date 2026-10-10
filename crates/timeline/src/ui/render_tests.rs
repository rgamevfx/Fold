//! Offscreen native rendering for visual review without OS pointer automation.
use super::*;
use fold_ui::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use fold_ui::sdk::{imgui, typography::Typography};
#[test]
#[ignore = "native UI capture to /tmp/fold-timeline-ui"]
fn timeline_normal_and_narrow() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Timeline capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = wgpu::Extent3d {
        width: 1024,
        height: 480,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Timeline review"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut canvas = canvas::Canvas::default();
    canvas.media_previews = false;
    let tracks = vec![
        crate::Track::new(TrackKind::Video, "V1"),
        crate::Track::new(TrackKind::Video, "V2"),
        crate::Track::new(TrackKind::Audio, "A1"),
    ];
    let mut clips = Vec::new();
    for (i, start, duration, name, icon) in [
        (0, 0, 4, "Interview", ToolbarIcon::View),
        (0, 4, 5, "Cutaway", ToolbarIcon::View),
        (1, 2, 5, "Badge Wave", ToolbarIcon::Composition),
        (2, 0, 9, "Interview audio", ToolbarIcon::Audio),
    ] {
        let id = ObjectId::new();
        clips.push(crate::Clip {
            effects: vec![],
            id,
            track: tracks[i].id,
            link: None,
            asset: Default::default(),
            info: crate::SourceMedia::Video(fold_media::VideoInfo {
                width: 1280,
                height: 720,
                rate: [24, 1],
                frames: 240,
            }),
            start: fold_foundation::Time::new(start, 1).unwrap(),
            source_start: fold_foundation::Time::ZERO,
            duration: fold_foundation::Time::new(duration, 1).unwrap(),
            level: 1.,
            extensions: Default::default(),
        });
        canvas.visuals.insert(
            id,
            canvas::ClipVisual {
                name: name.into(),
                icon,
                kind: "Video",
                texture: None,
                error: None,
            },
        );
    }
    let sequence = Sequence {
        transitions: vec![],
        dimensions: [1280, 720],
        rate: [24, 1],
        tracks,
        clips,
        extensions: Default::default(),
    };
    let document = DocumentId::new();
    let definition = DocumentId::new();
    let mut project = fold_project::Project::new(8);
    let mut definition_sequence = sequence.clone();
    definition_sequence.clips.clear();
    let mut inspected = sequence.clone();
    let asset = fold_foundation::AssetId::new();
    for clip in &mut inspected.clips {
        clip.asset = asset;
        clip.source_start = fold_foundation::Time::new(1, 1).unwrap();
    }
    let effect = fold_platform::effects::Instance {
        id: ObjectId::new(),
        definition: fold_project::DocumentRef {
            document: definition,
            output: "video".into(),
            extensions: Default::default(),
        },
        version: 1,
        enabled: true,
        animation: Default::default(),
        values: Default::default(),
        extensions: Default::default(),
    };
    inspected.clips[0].effects = vec![effect.clone()];
    inspected.transitions.push(crate::Transition {
        id: ObjectId::new(),
        outgoing: inspected.clips[0].id,
        incoming: inspected.clips[1].id,
        start: fold_foundation::Time::new(7, 2).unwrap(),
        duration: fold_foundation::Time::new(1, 1).unwrap(),
        effect,
    });
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                fold_project::Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: "Interview.mov".into(),
                    fingerprint: "fixture".into(),
                    extensions: Default::default(),
                }),
                fold_project::Mutation::PutDocument(
                    definition_sequence.document(definition).unwrap(),
                ),
                fold_project::Mutation::PutDocument(inspected.document(document).unwrap()),
                fold_project::Mutation::PutItem(fold_project::Item {
                    id: fold_project::ItemId::Document(definition),
                    name: "Glow".into(),
                    parent: Default::default(),
                    order: 0,
                    extensions: Default::default(),
                }),
            ],
        })
        .unwrap();
    let mut host = InspectorHost {
        project,
        state: Default::default(),
    };
    let mut inspector = inspector::Inspector::default();
    canvas.cache = (0..100)
        .map(|f| (f, fold_ui::sdk::cache_indicator::Coverage::Ready))
        .chain((120..160).map(|f| (f, fold_ui::sdk::cache_indicator::Coverage::Partial)))
        .collect();
    std::fs::create_dir_all("/tmp/fold-timeline-ui").unwrap();
    for (name, width) in [
        ("normal", 1024.),
        ("narrow", 320.),
        ("menu-normal", 1024.),
        ("menu-narrow", 320.),
        ("clip-normal", 520.),
        ("clip-narrow", 280.),
        ("transition-normal", 520.),
        ("transition-narrow", 280.),
    ] {
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
        context.io_mut().set_display_size([1024., 480.]);
        context.io_mut().set_delta_time(1. / 60.);
        context.io_mut().add_mouse_pos_event([1000., 479.]);
        canvas.fit = true;
        host.state.selection = fold_platform::desktop::Selection {
            document: Some(document),
            objects: vec![if name.starts_with("transition") {
                inspected.transitions[0].id
            } else {
                inspected.clips[0].id
            }],
        };
        for frame_index in 0..8 {
            if name.starts_with("menu") {
                context
                    .io_mut()
                    .add_mouse_pos_event([if width < 400. { 180. } else { 220. }, 220.]);
                context
                    .io_mut()
                    .add_mouse_button_event(imgui::MouseButton::Right, frame_index == 3);
            }
            let ui = context.frame();
            ui.window("Timeline")
                .position([0., 0.], imgui::Condition::Always)
                .size([width, 470.], imgui::Condition::Always)
                .build(|| {
                    if name.starts_with("clip") || name.starts_with("transition") {
                        inspector.draw(ExtensionUi {
                            ui,
                            host: &mut host,
                        });
                        if frame_index >= 2 {
                            assert_eq!(ui.scroll_max_x(), 0., "{name}");
                        }
                        return;
                    }
                    controls::toolbar(ui, &mut canvas, &sequence, &[], 60);
                    ui.separator();
                    canvas.draw(
                        ui,
                        canvas::CanvasModel {
                            sequence: &sequence,
                            revision: Default::default(),
                            playhead: 60,
                            selected: &[sequence.clips[2].id],
                            labels: &BTreeMap::new(),
                            placement: None,
                        },
                    );
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
            size: 1024 * 480 * 4,
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
                    bytes_per_row: Some(4096),
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
        let bright = (66..82)
            .flat_map(|y| (16..90).map(move |x| (y * 1024 + x) * 4))
            .filter(|&i| data[i] > 160 && data[i + 1] > 160 && data[i + 2] > 160)
            .count();
        assert!(
            name.contains("clip") || name.contains("transition") || bright > 15,
            "{name}: timecode text must remain visible ({bright} pixels)"
        );
        let mut ppm = b"P6\n1024 480\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold-timeline-ui/{name}.ppm"), ppm).unwrap();
    }
}

struct InspectorHost {
    project: fold_project::Project,
    state: fold_platform::desktop::DesktopState,
}
impl fold_platform::desktop::DesktopClient for InspectorHost {
    fn state(&self) -> &fold_platform::desktop::DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn command(&mut self, command: fold_platform::desktop::DesktopCommand) {
        panic!("Unexpected command: {command:?}");
    }
    fn poll(&mut self) {}
    fn request_preview(&mut self, _: fold_platform::desktop::PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<fold_platform::desktop::PreviewResult> {
        None
    }
    fn effect_interface(&self, _: DocumentId) -> Result<fold_platform::effects::Interface, String> {
        Ok(fold_platform::effects::Interface {
            version: 1,
            images: vec!["source".into()],
            controls: vec![],
            extensions: Default::default(),
        })
    }
}
