//! Opt-in real-device presentation/cache-lifetime regression, not a throughput test.
use super::*;
use fold_platform::desktop::{DesktopCommand, DesktopState, PreviewResult};

#[derive(Default)]
struct Client {
    state: DesktopState,
    generation: u64,
    admitted: Vec<u32>,
}
impl DesktopClient for Client {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, _: DesktopCommand) {}
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
    fn viewer_request(&self, _: PanelInstanceId) -> Option<(u64, fold_foundation::Time)> {
        Some((self.generation, fold_foundation::Time::ZERO))
    }
    fn present_viewer(&mut self, _: PanelInstanceId, generation: u64, key: &PreviewKey) -> bool {
        assert_eq!(generation, self.generation);
        self.admitted.push(key.frame);
        true
    }
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --test-threads=1"]
fn real_textures_hold_until_replacement_and_remain_protected_under_pressure() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    eprintln!("Phase 15A presentation adapter: {:?}", adapter.get_info());
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut context = dear_imgui_rs::Context::create();
    let mut renderer = WgpuRenderer::new(
        dear_imgui_wgpu::WgpuInitInfo::new(
            device.clone(),
            queue.clone(),
            wgpu::TextureFormat::Rgba8Unorm,
        ),
        &mut context,
    )
    .unwrap();
    let mut host = PreviewHost::new();
    host.cache.budget = 2 * 16 * 16 * 4;
    let mut client = Client::default();
    let id = PanelInstanceId(1);
    let first = PreviewKey {
        target: None,
        output: "video".into(),
        content: "test".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
    };
    let next = PreviewKey {
        frame: 1,
        ..first.clone()
    };
    let later = PreviewKey {
        frame: 2,
        ..first.clone()
    };
    let upload = |host: &mut PreviewHost, renderer: &mut WgpuRenderer, key: &PreviewKey| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("15A test image"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let registration = renderer
            .register_external_texture(&texture.create_view(&Default::default()))
            .unwrap();
        host.cache.insert(
            key.clone(),
            Texture {
                registration,
                _texture: texture,
                gpu_lease: None,
                dimensions: [16, 16],
                compression_attempted: false,
                in_flight: Arc::new(AtomicUsize::new(0)),
            },
            16 * 16 * 4,
        );
        host.completed_frame(key);
        host.requested = false;
    };
    host.select_viewers(&[(id, Some(first.clone()))], &mut client);
    upload(&mut host, &mut renderer, &first);
    host.present_viewers(&mut client);
    assert!(matches!(host.state_for_viewer(id), Preview::Ready { .. }));
    for _ in 0..100 {
        host.select_viewers(&[(id, Some(next.clone()))], &mut client);
        host.present_viewers(&mut client);
        assert_eq!(host.presented_key(id), Some(&first));
        assert!(matches!(host.state_for_viewer(id), Preview::Ready { .. }));
        assert!(
            host.evict_unused().is_none(),
            "waiting cannot evict the held image"
        );
    }
    // Real-time demand moves beyond an admitted render before it completes.
    host.select_viewers(&[(id, Some(later.clone()))], &mut client);
    upload(&mut host, &mut renderer, &next);
    host.present_viewers(&mut client);
    assert_eq!(host.presented_key(id), Some(&next));
    assert_eq!(client.admitted, vec![0, 1]);
    let flight = host.cache.peek(&first).unwrap().in_flight.clone();
    flight.store(1, Ordering::Release);
    assert!(host.cache.needs_room(16 * 16 * 4));
    assert!(
        host.evict_unused().is_none(),
        "submitted old image and held replacement both stay protected"
    );
    flight.store(0, Ordering::Release);
    let retired = host.evict_unused().unwrap();
    renderer
        .unregister_external_texture(retired.registration)
        .unwrap();
    assert!(host.cache.peek(&first).is_none());
    assert!(host.cache.peek(&next).is_some());
    // A result from before a seek may populate cache, but cannot replace the held image.
    host.select_viewers(&[(id, Some(later.clone()))], &mut client);
    client.generation += 1;
    let seek = PreviewKey { frame: 10, ..first };
    host.select_viewers(&[(id, Some(seek))], &mut client);
    upload(&mut host, &mut renderer, &later);
    host.present_viewers(&mut client);
    assert_eq!(host.presented_key(id), Some(&next));
    assert_eq!(client.admitted, vec![0, 1]);
    assert!(host.cache.bytes <= host.cache.budget);
    host.select_viewers(&[], &mut client);
    assert!(host.held.is_empty());
    host.release(&mut renderer).unwrap();
}
