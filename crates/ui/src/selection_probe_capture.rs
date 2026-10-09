//! Native render/readback for the opt-in workspace integration probe.
use crate::sdk::{
    imgui,
    render_backend::{FramebufferExtent, WgpuInitInfo, WgpuRenderer, wgpu},
};
pub(super) struct Capture {
    renderer: WgpuRenderer,
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: Option<wgpu::Texture>,
}
impl Capture {
    pub fn new(context: &mut imgui::Context) -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        eprintln!("Panel linking capture adapter: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let renderer = WgpuRenderer::new(
            WgpuInitInfo::new(
                device.clone(),
                queue.clone(),
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            context,
        )
        .unwrap();
        Self {
            renderer,
            device,
            queue,
            texture: None,
        }
    }
    pub fn draw(&mut self, context: &mut imgui::Context, width: u32, height: u32) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Panel linking capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let frame = context.render(self.renderer.renderer_consumer().unwrap());
        let mut encoder = self.device.create_command_encoder(&Default::default());
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
            self.renderer
                .render(frame, &mut pass, FramebufferExtent::from_texture(&texture))
                .unwrap();
        }
        self.queue.submit([encoder.finish()]);
        self.texture = Some(texture);
    }
    pub fn save(&self, path: &std::path::Path) {
        let texture = self.texture.as_ref().unwrap();
        let size = texture.size();
        let stride = (size.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(stride) * u64::from(size.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let mut ppm = format!("P6\n{} {}\n255\n", size.width, size.height).into_bytes();
        for row in data.chunks_exact(stride as usize) {
            for pixel in row[..size.width as usize * 4].chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
        }
        std::fs::write(path, ppm).unwrap();
    }
}
