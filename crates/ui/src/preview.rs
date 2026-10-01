//! One-shot host texture registration. The engine owns evaluation and display pixels;
//! this host owns only their GPU presentation copy, never an engine resource pool.
use crate::shell::Preview;
use dear_imgui_wgpu::{ExternalTextureId, WgpuRenderer, wgpu};
use fold_platform::DisplayFrame;
use std::sync::mpsc::{Receiver, TryRecvError};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) struct PreviewHost {
    receiver: Option<Receiver<std::result::Result<DisplayFrame, String>>>,
    pub(crate) state: Preview,
    registration: Option<ExternalTextureId>,
    texture: Option<wgpu::Texture>,
}

impl PreviewHost {
    pub(crate) fn new(receiver: Receiver<std::result::Result<DisplayFrame, String>>) -> Self {
        Self {
            receiver: Some(receiver),
            state: Preview::Pending,
            registration: None,
            texture: None,
        }
    }

    pub(crate) fn poll(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) -> Result<()> {
        let Some(receiver) = &self.receiver else {
            return Ok(());
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                Err("Preview worker stopped without a result".into())
            }
        };
        self.receiver = None;
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                self.state = Preview::Failed(error);
                return Ok(());
            }
        };
        let [width, height] = frame.dimensions();
        if width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            self.state = Preview::Failed("Preview exceeds GPU texture dimensions".into());
            return Ok(());
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Fold display copy (opaque sRGB bytes)"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Encoded bytes with a non-sRGB target: no additional transfer transform.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.rgba(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let registration = renderer.register_external_texture(&view)?;
        self.state = Preview::Ready {
            texture: registration.texture_id(),
            dimensions: [width, height],
        };
        self.registration = Some(registration);
        self.texture = Some(texture);
        Ok(())
    }

    pub(crate) fn release(&mut self, renderer: &mut WgpuRenderer) -> Result<()> {
        if let Some(registration) = self.registration.take() {
            renderer.unregister_external_texture(registration)?;
        }
        // Drop handles, never explicitly destroy resources still referenced by GPU work.
        self.texture = None;
        Ok(())
    }
}
