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
    EventLoop::new()?.run_app(&mut app)?;
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
            WindowEvent::RedrawRequested => {
                if let Err(error) = desktop.draw() {
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
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = self.preview.release(&mut self.renderer);
    }
}

impl Desktop {
    fn new(
        event_loop: &ActiveEventLoop,
        preview: Box<dyn DesktopClient>,
        mut panels: Vec<crate::sdk::RegisteredPanel>,
    ) -> Result<Self> {
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
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
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
        Ok(Self {
            context,
            platform,
            renderer,
            surface,
            device,
            queue,
            config,
            window,
            shell: Shell::new(panels),
            canvas_pan: crate::sdk::CanvasPan::default(),
            pointer: [-1.0; 2],
            preview: PreviewHost::new(),
            client: preview,
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
        let surface_frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.resize();
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        self.client.poll();
        self.platform
            .prepare_frame(&mut self.context, &self.window)?;
        let ui = self.context.frame();
        self.shell.controls(ui, self.client.as_mut())?;
        self.preview
            .select(self.shell.key(self.client.as_ref()), self.client.as_mut());
        self.preview.poll(
            self.client.as_mut(),
            &self.device,
            &self.queue,
            &mut self.renderer,
        )?;
        self.shell
            .viewer(ui, &self.preview.state, &self.preview.statistics());
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
        self.queue.submit([encoder.finish()]);
        self.preview.submitted(&self.queue);
        self.window.pre_present_notify();
        surface_frame.present();
        Ok(())
    }
}
