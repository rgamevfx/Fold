//! Opt-in real-device presentation/cache-lifetime regression, not a throughput test.
use super::*;
use fold_platform::desktop::{DesktopCommand, DesktopState, PreviewResult};

#[derive(Default)]
struct Client {
    state: DesktopState,
    generation: u64,
    admitted: Vec<u32>,
    transport: Option<fold_platform::desktop::ViewerTransport>,
    demands: Vec<PreviewDemand>,
}
impl DesktopClient for Client {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        if let DesktopCommand::ViewerTransport { transport, .. } = command {
            self.transport = Some(transport);
            self.generation += 1;
        }
    }
    fn viewer_transport(
        &self,
        _: PanelInstanceId,
    ) -> Option<fold_platform::desktop::ViewerTransport> {
        self.transport.clone()
    }
    fn video_info(
        &self,
        _: fold_foundation::DocumentId,
    ) -> std::result::Result<fold_platform::VideoInfo, String> {
        Ok(fold_platform::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 2,
        })
    }
    fn preview_demands(&mut self, demands: Vec<PreviewDemand>) {
        self.demands = demands;
    }

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
        host.admitted.insert(id, 0); // The simulated worker admitted these frames before the seek.
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
    // Bounded preparation uses the same resident textures. Cached replay is
    // explicit, keeps the chosen mode, and stops if another consumer evicts a frame.
    host.cache.budget = 5 * 16 * 16 * 4;
    let document = fold_foundation::DocumentId::new();
    let template = PreviewKey {
        target: Some((document, fold_foundation::Time::ZERO)),
        output: "video".into(),
        content: "review".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
    };
    let second = PreviewKey {
        frame: 1,
        target: Some((document, fold_foundation::Time::new(1, 24).unwrap())),
        ..template.clone()
    };
    client.transport = Some(fold_platform::desktop::ViewerTransport {
        mode: fold_platform::desktop::PlaybackMode::EveryFrame,
        output: fold_platform::workspace::DocumentRef {
            document,
            output: "video".into(),
            extensions: Default::default(),
        },
        time: fold_foundation::Time::ZERO,
        range: Default::default(),
        looping: false,
        playing: false,
    });
    host.select_viewers(&[(id, Some(template.clone()))], &mut client);
    upload(&mut host, &mut renderer, &template);
    upload(&mut host, &mut renderer, &second);
    host.review_action(id, crate::review::Action::Prepare, &mut client);
    host.select_viewers(&[(id, Some(template.clone()))], &mut client);
    assert!(host.review_state(id).1);
    assert!(
        client.demands.is_empty(),
        "resident preparation must not request decode or upload"
    );
    host.review_action(id, crate::review::Action::PlayCached, &mut client);
    assert!(!client.transport.as_ref().unwrap().playing);
    assert_eq!(
        client.transport.as_ref().unwrap().mode,
        fold_platform::desktop::PlaybackMode::EveryFrame
    );
    client.transport.as_mut().unwrap().mode = fold_platform::desktop::PlaybackMode::RealTime;
    host.review_action(id, crate::review::Action::PlayCached, &mut client);
    host.select_viewers(&[(id, Some(template))], &mut client);
    assert!(client.transport.as_ref().unwrap().playing);
    assert!(client.demands.is_empty());
    let evicted = host.cache.evict(|key, _| key == &second).unwrap();
    renderer
        .unregister_external_texture(evicted.registration)
        .unwrap();
    client.transport.as_mut().unwrap().time = second.target.unwrap().1;
    host.select_viewers(&[(id, Some(second.clone()))], &mut client);
    assert!(!client.transport.as_ref().unwrap().playing);
    assert_eq!(
        client.transport.as_ref().unwrap().mode,
        fold_platform::desktop::PlaybackMode::RealTime
    );
    assert!(host.review_state(id).0.unwrap().contains("frame evicted"));
    host.release(&mut renderer).unwrap();

    // A range can contain older cache hits before preparation begins. Its new
    // frames must evict unrelated entries first, not those earlier range hits.
    let mut host = PreviewHost::new();
    host.cache.budget = 5 * 16 * 16 * 4;
    let mut plan = crate::review::Plan::new(
        1,
        second.clone(),
        [24, 1],
        fold_platform::desktop::PlaybackRange {
            start: Some(0),
            end: Some(2),
        },
        3,
        host.cache.budget,
    );
    let first = plan.key(0);
    let middle = plan.key(1);
    let last = plan.key(2);
    let unrelated = PreviewKey {
        content: "other".into(),
        ..middle.clone()
    };
    upload(&mut host, &mut renderer, &middle);
    upload(&mut host, &mut renderer, &unrelated);
    upload(&mut host, &mut renderer, &first);
    plan.cursor = 2;
    host.consumers.push(first);
    host.reviews.insert(id, plan);
    let evicted = host.evict_unused().unwrap();
    renderer
        .unregister_external_texture(evicted.registration)
        .unwrap();
    assert!(host.cache.peek(&unrelated).is_none());
    assert!(host.cache.peek(&middle).is_some());
    upload(&mut host, &mut renderer, &last);
    assert_eq!(
        host.reviews[&id].resident(|k| host.cache.peek(k).is_some()),
        3
    );
    host.release(&mut renderer).unwrap();
}
