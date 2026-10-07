use super::*;
use fold_platform::desktop::{DesktopState, PreviewKey, PreviewResult};
use fold_project::{Document, EditBatch, Mutation, Project};
struct Host {
    project: Project,
    state: DesktopState,
    commands: Vec<DesktopCommand>,
    document: DocumentId,
    settings: DocumentSettings,
}
impl Host {
    fn new() -> Self {
        let project = Project::with_settings(
            8,
            [(
                fold_platform::color::PROJECT_KEY.into(),
                serde_json::to_value(fold_platform::color::ProjectColor::default()).unwrap(),
            )]
            .into(),
        );
        let mut host = Self {
            project,
            state: Default::default(),
            commands: vec![],
            document: DocumentId::new(),
            settings: DocumentSettings::fixed(ProjectDefaults::default().format),
        };
        host.project
            .commit(EditBatch {
                base: host.project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(Document {
                    id: host.document,
                    type_id: "test.document".into(),
                    package_id: "test".into(),
                    schema_version: 1,
                    revision: Default::default(),
                    dependencies: vec![],
                    assets: vec![],
                    payload: b"{}".to_vec(),
                    extensions: Default::default(),
                })],
            })
            .unwrap();
        host
    }
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn document_settings(&self, id: DocumentId) -> Result<Option<DocumentSettings>, String> {
        Ok((id == self.document).then(|| self.settings.clone()))
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        self.commands.push(command);
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn drafts_validate_exact_rates_and_keep_explicit_target() {
    let host = Host::new();
    let mut dialog = Dialog::new(&host, Some(host.document)).unwrap();
    dialog.document.as_mut().unwrap().format.rate = [24000, 1001];
    dialog.document.as_mut().unwrap().format.width = 1920;
    let request = dialog.request().unwrap();
    let args: DocumentEdit = serde_json::from_slice(&request.arguments).unwrap();
    assert_eq!(args.document, host.document);
    assert_eq!(args.settings.format.rate, [24000, 1001]);
    assert_eq!(host.settings.format.width, 1280);
    assert!(host.commands.is_empty());
    dialog.document.as_mut().unwrap().format.rate[1] = 0;
    assert!(dialog.request().is_err());
    let mut project = Dialog::new(&host, None).unwrap();
    project.bundled = false;
    project.color_path = Some("relative/config.ocio".into());
    assert!(project.request().is_err());
}
#[test]
fn cancel_and_narrow_draw_do_not_mutate_project() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let _fonts = crate::sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut host = Host::new();
    let before = host.project.snapshot();
    for width in [440., 320.] {
        for target in [None, Some(host.document)] {
            let mut dialog = Dialog::new(&host, target).unwrap();
            dialog.defaults.cache_folder = "draft/only".into();
            for _ in 0..3 {
                let ui = context.frame();
                ui.window(&dialog.title)
                    .size([width, 600.], imgui::Condition::Always)
                    .build(|| {});
                assert!(dialog.draw(ui, &mut host));
                assert!(context.render_legacy().total_vtx_count() > 100);
            }
            context.io_mut().add_key_event(imgui::Key::Escape, true);
            assert!(!dialog.draw(context.frame(), &mut host));
            context.end_frame();
            context.io_mut().add_key_event(imgui::Key::Escape, false);
            context.frame();
            context.end_frame();
        }
    }
    assert!(host.commands.is_empty());
    assert!(host.project.snapshot().same_content(&before));
}

#[test]
#[ignore = "native capture; writes /tmp/fold-settings-ui"]
fn settings_native_normal_and_narrow() {
    use crate::sdk::render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("Settings capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let _fonts = crate::sdk::typography::Typography::install(&mut context);
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
        height: 800,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Settings UI"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    context.io_mut().set_display_size([768., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    std::fs::create_dir_all("/tmp/fold-settings-ui").unwrap();
    let mut host = Host::new();
    for (name, width, target, timeline) in [
        ("project-normal", 440., None, false),
        ("project-narrow", 320., None, false),
        ("composition", 440., Some(host.document), false),
        ("timeline-narrow", 320., Some(host.document), true),
    ] {
        host.settings.timeline = timeline;
        host.settings.output_duration = timeline.then(|| fold_foundation::Time::new(5, 1).unwrap());
        let mut dialog = Dialog::new(&host, target).unwrap();
        context.load_ini_settings(&format!(
            "[Window][{}]\nPos=10,10\nSize={},{}\nCollapsed=0\n",
            dialog.title,
            width,
            if target.is_none() { 620. } else { 470. }
        ));
        for _ in 0..12 {
            let ui = context.frame();
            assert!(dialog.draw(ui, &mut host));
            let frame = context.render(renderer.renderer_consumer().unwrap());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
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
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 768 * 800 * 4,
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
                    bytes_per_row: Some(768 * 4),
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
        let mut bright = 0;
        for y in 80..96 {
            for x in 20..70 {
                let offset = (y * 768 + x) * 4;
                bright += usize::from(
                    data[offset..offset + 3]
                        .iter()
                        .all(|channel| *channel > 150),
                );
            }
        }
        assert!(bright > 30, "{name}: resolution value must remain readable");
        let mut ppm = b"P6\n768 800\n255\n".to_vec();
        for pixel in data.chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(format!("/tmp/fold-settings-ui/{name}.ppm"), ppm).unwrap();
    }
}

#[test]
fn gear_targets_its_editor_and_successful_auto_range_apply_closes() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([700., 600.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut host = Host::new();
    host.state.selection.document = Some(DocumentId::new());
    let id = host.document;
    let mut center = [0.; 2];
    let mut draw = |context: &mut imgui::Context, host: &mut Host| {
        let ui = context.frame();
        ui.window("Editor toolbar")
            .position([0., 0.], imgui::Condition::Always)
            .size([320., 100.], imgui::Condition::Always)
            .build(|| {
                button(ui, host, Some(id));
                let min = ui.item_rect_min();
                let max = ui.item_rect_max();
                center = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
            });
        context.end_frame();
        center
    };
    for _ in 0..3 {
        draw(&mut context, &mut host);
    }
    let center = draw(&mut context, &mut host);
    context.io_mut().add_mouse_pos_event(center);
    draw(&mut context, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    draw(&mut context, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    draw(&mut context, &mut host);
    assert!(
        matches!(host.commands.as_slice(), [DesktopCommand::OpenSettings(Some(target))] if *target == id)
    );
    host.settings.timeline = true;
    let mut dialog = Dialog::new(&host, Some(id)).unwrap();
    dialog.pending = true;
    dialog.document.as_mut().unwrap().format.rate = [30, 1];
    host.settings.format.rate = [30, 1];
    // Auto duration is derived by the provider at the new rate.
    host.settings.format.frames = 150;
    assert!(!dialog.draw(context.frame(), &mut host));
    context.end_frame();
}
