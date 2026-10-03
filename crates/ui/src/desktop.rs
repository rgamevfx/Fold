use crate::{preview::PreviewHost, shell::Shell};
use dear_imgui_rs::{ConfigFlags, Context};
use dear_imgui_wgpu::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use fold_platform::desktop::DesktopClient;
use std::{error::Error, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[cfg(target_os = "linux")]
#[path = "native_drop.rs"]
mod native_drop;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) fn run(
    preview: Box<dyn DesktopClient>,
    panels: Vec<crate::sdk::RegisteredPanel>,
) -> Result<()> {
    let mut app = App {
        preview: Some(preview),
        panels: Some(panels),
        desktop: None,
        error: None,
    };
    let mut builder = EventLoop::builder();
    // The pinned winit release has no Wayland data-device/file-drop support.
    // Use XWayland on the reference Linux desktop until that backend supports it.
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    builder.build()?.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct App {
    preview: Option<Box<dyn DesktopClient>>,
    panels: Option<Vec<crate::sdk::RegisteredPanel>>,
    desktop: Option<Desktop>,
    error: Option<Box<dyn Error>>,
}

impl App {
    fn stop(&mut self, event_loop: &ActiveEventLoop, error: Box<dyn Error>) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.desktop.is_none() {
            match Desktop::new(
                event_loop,
                self.preview.take().expect("single desktop initialization"),
                self.panels.take().expect("single panel initialization"),
            ) {
                Ok(desktop) => self.desktop = Some(desktop),
                Err(error) => self.stop(event_loop, error),
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        let Some(desktop) = self.desktop.as_mut() else {
            return;
        };
        if id != desktop.window.id() {
            return;
        }
        // Translate before the backend queues input, so ImGui derives correct
        // click/drag state. Never reinterpret a gesture halfway through.
        let previous_shift = desktop.canvas_pan.effective_shift();
        match &mut event {
            WindowEvent::ModifiersChanged(modifiers) => {
                desktop.canvas_pan.shift(modifiers.state().shift_key());
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical::<f32>(desktop.platform.hidpi_factor());
                desktop.pointer = [position.x, position.y];
            }
            WindowEvent::MouseInput { state, button, .. } => match *button {
                winit::event::MouseButton::Left => {
                    let over_canvas = desktop.shell.accepts_background_pan(desktop.pointer);
                    if desktop
                        .canvas_pan
                        .left_button(state.is_pressed(), over_canvas)
                        == dear_imgui_rs::MouseButton::Middle
                    {
                        *button = winit::event::MouseButton::Middle;
                    }
                }
                winit::event::MouseButton::Middle => desktop.canvas_pan.middle(state.is_pressed()),
                _ => {}
            },
            WindowEvent::Focused(false) => desktop.canvas_pan.reset(),
            _ => {}
        }
        if let Err(error) =
            desktop
                .platform
                .handle_window_event(&mut desktop.context, &desktop.window, &event)
        {
            self.stop(event_loop, error.into());
            return;
        }
        let shift = desktop.canvas_pan.effective_shift();
        if shift != previous_shift || matches!(event, WindowEvent::ModifiersChanged(_)) {
            desktop
                .context
                .io_mut()
                .add_key_event(dear_imgui_rs::Key::ModShift, shift);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => desktop.resize(),
            WindowEvent::HoveredFile(_) => desktop.external_drag = true,
            WindowEvent::HoveredFileCancelled => desktop.external_drag = false,
            WindowEvent::DroppedFile(path) => {
                desktop.external_drag = false;
                #[cfg(target_os = "linux")]
                {
                    desktop.pointer = desktop
                        .drop_pointer
                        .as_ref()
                        .and_then(|p| p.position(desktop.platform.hidpi_factor()))
                        .unwrap_or([-1.; 2]);
                }
                if desktop.dropped_files.len() < 33 {
                    desktop.dropped_files.push(path);
                }
            }
            WindowEvent::RedrawRequested => {
                #[cfg(target_os = "linux")]
                if desktop.external_drag {
                    desktop.pointer = desktop
                        .drop_pointer
                        .as_ref()
                        .and_then(|p| p.position(desktop.platform.hidpi_factor()))
                        .unwrap_or([-1.; 2]);
                }
                desktop
                    .shell
                    .external_drag(desktop.external_drag.then_some(desktop.pointer));
                if !desktop.dropped_files.is_empty() {
                    let paths = std::mem::take(&mut desktop.dropped_files);
                    desktop
                        .shell
                        .files_dropped(desktop.pointer, &paths, desktop.client.as_mut());
                }
                let result = desktop.draw();
                #[cfg(feature = "native-probe")]
                if result.is_ok() && desktop.probe_done {
                    event_loop.exit();
                }
                if let Err(error) = result {
                    self.stop(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(desktop) = &self.desktop {
            let size = desktop.window.inner_size();
            if size.width > 0 && size.height > 0 {
                desktop.window.request_redraw();
            }
        }
    }
}

struct Desktop {
    // Native editor contexts must be destroyed before their owning ImGui context.
    shell: Shell,
    canvas_pan: crate::sdk::CanvasPan,
    pointer: [f32; 2],
    dropped_files: Vec<std::path::PathBuf>,
    external_drag: bool,
    #[cfg(target_os = "linux")]
    drop_pointer: Option<native_drop::DropPointer>,
    // Context tears down backend attachments while their resources are still alive.
    context: Context,
    platform: WinitPlatform,
    renderer: WgpuRenderer,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
    preview: PreviewHost,
    client: Box<dyn DesktopClient>,
    #[cfg(feature = "native-probe")]
    probe: Option<crate::native_probe::Probe>,
    #[cfg(feature = "native-probe")]
    probe_done: bool,
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.shell.save_workspace(&mut self.context);
        let _ = self.preview.release(&mut self.renderer);
    }
}

impl Desktop {
    fn new(
        event_loop: &ActiveEventLoop,
        mut preview: Box<dyn DesktopClient>,
        mut panels: Vec<crate::sdk::RegisteredPanel>,
    ) -> Result<Self> {
        #[cfg(feature = "native-probe")]
        let probe = crate::native_probe::Probe::from_env()?;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Fold")
                    .with_inner_size(LogicalSize::new(1280.0, 800.0)),
            )?,
        );
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        eprintln!("Fold presentation adapter: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_features: adapter.features()
                    & (wgpu::Features::FLOAT32_FILTERABLE
                        | wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                        | wgpu::Features::TEXTURE_COMPRESSION_BC),
                ..Default::default()
            }))?;
        let mut preview_host = PreviewHost::new();
        // One host budget/device for scene evaluation and held display leases.
        // CPU is an explicit startup fallback, never a silent per-effect bypass.
        match std::env::var("FOLD_RENDER_BACKEND").as_deref() {
            Ok("cpu") => eprintln!("Fold render backend: CPU reference"),
            Ok("gpu") | Err(std::env::VarError::NotPresent) => {
                let host = fold_platform::gpu::Host::from_device(
                    device.clone(),
                    queue.clone(),
                    512 * 1024 * 1024,
                )?;
                preview_host.attach_host(&host);
                preview.set_render_host(host);
                eprintln!("Fold render backend: shared GPU");
            }
            _ => return Err("FOLD_RENDER_BACKEND must be cpu or gpu".into()),
        }
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("GPU has no compatible surface configuration")?;
        // Display frames already carry the engine's exact sRGB output transform.
        // Avoid the ImGui backend's approximate pow(2.2) sRGB-target correction.
        config.format = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or("GPU has no supported non-sRGB SDR surface")?;
        config.present_mode = wgpu::PresentMode::Fifo;
        #[cfg(feature = "native-probe")]
        if probe.is_some() {
            eprintln!(
                "Fold native probe surface: {config:?}; scale: {}",
                window.scale_factor()
            );
        }
        surface.configure(&device, &config);
        let mut context = Context::create();
        // Workspace state must never leak into the current project directory.
        context.set_ini_filename(None::<String>)?;
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE | ConfigFlags::NAV_ENABLE_KEYBOARD);
        let mut platform = WinitPlatform::new(&mut context)?;
        platform.attach_window(window.clone(), HiDpiMode::Default, &mut context)?;
        let renderer = WgpuRenderer::new(
            WgpuInitInfo::new(device.clone(), queue.clone(), config.format),
            &mut context,
        )?;
        for registered in &mut panels {
            registered.panel.initialize(&context);
        }
        let shell = Shell::new(panels);
        #[cfg(feature = "native-probe")]
        let shell = {
            let mut shell = shell;
            if probe.is_some() {
                shell.probe_full_quality();
            }
            shell
        };
        Ok(Self {
            context,
            platform,
            renderer,
            surface,
            device,
            queue,
            config,
            window: window.clone(),
            shell,
            canvas_pan: crate::sdk::CanvasPan::default(),
            pointer: [-1.0; 2],
            dropped_files: Vec::new(),
            external_drag: false,
            #[cfg(target_os = "linux")]
            drop_pointer: native_drop::DropPointer::new(&window),
            preview: preview_host,
            client: preview,
            #[cfg(feature = "native-probe")]
            probe,
            #[cfg(feature = "native-probe")]
            probe_done: false,
        })
    }

    fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            self.window.request_redraw();
        }
    }

    fn draw(&mut self) -> Result<()> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        #[cfg(feature = "native-probe")]
        let draw_start = std::time::Instant::now();
        let surface_frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.resize();
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        #[cfg(feature = "native-probe")]
        let acquired = std::time::Instant::now();
        self.client.poll();
        self.platform
            .prepare_frame(&mut self.context, &self.window)?;
        self.shell
            .workspace_frame(&mut self.context, self.client.as_mut());
        self.shell.prepare_frame(&mut self.context);
        let ui = self.context.frame();
        self.shell.controls(ui, self.client.as_mut())?;
        let demands = self.shell.keys(self.client.as_ref());
        self.preview.select_viewers(&demands, self.client.as_mut());
        self.preview.poll(
            self.client.as_mut(),
            &self.device,
            &self.queue,
            &mut self.renderer,
        )?;
        self.preview.present_viewers(self.client.as_mut());
        for (id, _) in demands {
            self.shell.presentation(
                id,
                self.preview.presented_key(id).cloned(),
                self.preview.viewer_error(id).map(str::to_owned),
                self.client.as_ref(),
            );
            let preview = self.preview.state_for_viewer(id);
            self.shell.viewer_instance(
                ui,
                id,
                &preview,
                &self.preview.statistics(),
                self.client.as_mut(),
            );
        }
        self.shell.viewer_overlays(ui, self.client.as_mut());
        self.platform.prepare_render(ui, &self.window)?;
        let frame = self.context.render(self.renderer.renderer_consumer()?);
        let view = surface_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Fold UI"),
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
            self.renderer.render(
                frame,
                &mut pass,
                FramebufferExtent::from_texture(&surface_frame.texture),
            )?;
        }
        #[cfg(feature = "native-probe")]
        let submit_start = std::time::Instant::now();
        self.queue.submit([encoder.finish()]);
        #[cfg(feature = "native-probe")]
        let submitted = std::time::Instant::now();
        self.preview.submitted(&self.queue);
        #[cfg(feature = "native-probe")]
        let completion = self.probe.as_ref().map(|probe| {
            let value = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let result = value.clone();
            let start = probe.start;
            self.queue.on_submitted_work_done(move || {
                result.store(
                    start.elapsed().as_nanos() as u64,
                    std::sync::atomic::Ordering::Release,
                );
            });
            value
        });
        #[cfg(feature = "native-probe")]
        let present_start = std::time::Instant::now();
        self.window.pre_present_notify();
        surface_frame.present();
        #[cfg(feature = "native-probe")]
        if let Some(probe) = &mut self.probe {
            let end = std::time::Instant::now();
            let (frame, ready) = self.preview.probe_frame();
            probe.push(crate::native_probe::Sample {
                frame,
                ready,
                start_ms: probe.ms(draw_start),
                acquire_ms: (acquired - draw_start).as_secs_f64() * 1000.,
                draw_ms: (end - draw_start).as_secs_f64() * 1000.,
                submit_ms: (submitted - submit_start).as_secs_f64() * 1000.,
                submitted_ms: probe.ms(submitted),
                present_ms: (end - present_start).as_secs_f64() * 1000.,
                upload: self
                    .preview
                    .probe_upload
                    .map(|(a, b, bytes)| (probe.ms(a), probe.ms(b), bytes)),
                completion_ns: completion.unwrap(),
            });
            if probe.advance(self.client.as_mut(), frame, ready)? {
                // One nonblocking poll may observe the last completion; missing
                // callbacks remain null in the report, never fabricated as zero.
                let _ = self.device.poll(wgpu::PollType::Poll);
                probe.finish(true)?;
                self.probe_done = true;
            } else {
                self.shell.probe_time(self.client.as_mut());
            }
        }
        Ok(())
    }
}
