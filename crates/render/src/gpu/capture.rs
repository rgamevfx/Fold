//! Explicit diagnostic thumbnails of presentation textures, including BC7 caches.
use super::Host;
use fold_media::Cancel;
impl Host {
    /// Caller retains the presentation lease for this call. Normal playback never reads pixels back.
    pub fn capture_presentation(
        &self,
        source: &wgpu::Texture,
        dimensions: [u32; 2],
        identity: String,
        cancel: &Cancel,
    ) -> Result<crate::DisplayFrame, String> {
        cancel.check()?;
        let [sw, sh] = dimensions;
        if sw == 0 || sh == 0 || sw > source.width() || sh > source.height() {
            return Err("invalid capture dimensions".into());
        }
        let scale = (1024.0 / f64::from(sw.max(sh))).min(1.0);
        let width = (f64::from(sw) * scale).round().max(1.0) as u32;
        let height = (f64::from(sh) * scale).round().max(1.0) as u32;
        let stride = (width * 4).div_ceil(256) * 256;
        let reservation = self.reserve(
            u64::from(width) * u64::from(height) * 4 + u64::from(stride) * u64::from(height),
        )?;
        let target = self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Agent viewer thumbnail"),
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
        let shader=self.device().create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("Agent capture shader"),source:wgpu::ShaderSource::Wgsl(format!(r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{
 let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.)); return vec4(p[i],0.,1.);
}}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{
 let xy=min(vec2<i32>(p.xy*vec2<f32>({sw}.0/{width}.0,{sh}.0/{height}.0)),vec2<i32>({sw_minus},{sh_minus}));
 return textureLoad(image,xy,0);
}}"#,sw_minus=sw-1,sh_minus=sh-1).into())});
        let pipeline = self
            .device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Agent capture"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });
        let view = source.create_view(&Default::default());
        let binding = self.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Agent capture readback"),
            size: u64::from(stride) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let target_view = target.create_view(&Default::default());
        let mut encoder = self.device().create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &binding, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            target.size(),
        );
        self.submit(encoder, vec![], vec![reservation]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        let start = std::time::Instant::now();
        loop {
            self.poll()?;
            cancel.check()?;
            match rx.try_recv() {
                Ok(v) => {
                    v.map_err(|e| e.to_string())?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("capture mapping disconnected".into());
                }
                _ => {}
            }
            if start.elapsed().as_secs() > 10 {
                return Err("capture timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let storage = fold_media::budget::reserve_output(u64::from(width) * u64::from(height) * 4)?;
        let mapped = buffer.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped.chunks_exact(stride as usize) {
            rgba.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(crate::DisplayFrame {
            width,
            height,
            rgba,
            _storage: storage,
            transform_identity: identity,
            timing: None,
        })
    }
}
