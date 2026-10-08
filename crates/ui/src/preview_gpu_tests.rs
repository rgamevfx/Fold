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
            frames: self.state.frames.max(2),
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
        region: None,
        target: None,
        output: "video".into(),
        content: "test".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
        channels: Default::default(),
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
                retention_attempted: false,
                timeline: false,
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
    host.clear_unused(&mut renderer, &mut client).unwrap();
    assert!(
        host.cache.peek(&first).is_some(),
        "in-flight cache entry must survive clear"
    );
    assert!(
        host.cache.peek(&next).is_some(),
        "displayed image must survive clear"
    );
    flight.store(0, Ordering::Release);
    host.clear_unused(&mut renderer, &mut client).unwrap();
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
        region: None,
        target: Some((document, fold_foundation::Time::ZERO)),
        output: "video".into(),
        content: "review".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
        channels: Default::default(),
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
        false,
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
    assert_eq!(host.reviews[&id].resident(host.cache.keys()), 3);
    host.release(&mut renderer).unwrap();
}

#[test]
#[ignore = "requires native GPU; tiered preparation and graph-free replay"]
fn native_range_spills_beyond_gpu_and_replays_without_render_demands() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let (engine, _) =
        pollster::block_on(fold_platform::gpu::Host::headless(256 * 1024 * 1024)).unwrap();
    let device = engine.device();
    let queue = engine.queue();
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
    host.attach_host(&engine);
    host.set_cache_budget(2 * 1920 * 1080 * 4);
    host.configure_retention(fold_platform::application::Application {
        viewer_ram_mib: 64,
        ..Default::default()
    });
    let id = PanelInstanceId(1);
    let document = fold_foundation::DocumentId::new();
    let template = PreviewKey {
        region: None,
        target: Some((document, fold_foundation::Time::ZERO)),
        output: "video".into(),
        content: "tier-test".into(),
        frame: 0,
        dimensions: [1920, 1080],
        view: 1,
        channels: Default::default(),
    };
    let mut client = Client::default();
    client.state.frames = 16;
    client.transport = Some(fold_platform::desktop::ViewerTransport {
        mode: fold_platform::desktop::PlaybackMode::RealTime,
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
    let mut rendered = std::collections::HashSet::new();
    let mut preparing = false;
    let start = std::time::Instant::now();
    loop {
        assert!(
            start.elapsed().as_secs() < 40,
            "range preparation stalled: {:?}, {:?}",
            host.review_state(id),
            host.retention_error()
        );
        host.select_viewers(&[(id, Some(template.clone()))], &mut client);
        if host.pending.is_none()
            && let Some(demand) = client.demands.first().cloned()
        {
            assert!(
                rendered.insert(demand.key.clone()),
                "duplicate graph evaluation"
            );
            let pixels = fold_platform::presentation::Pixels {
                description: fold_platform::presentation::Description {
                    dimensions: [1920, 1080],
                    format: fold_platform::presentation::Format::Rgba8,
                },
                data: [64, 128, 192, 255].repeat(1920 * 1080),
            };
            host.result_key = Some(demand.key);
            host.pending = Some(Prepared::Gpu(Box::new(
                engine.restore_presentation(&pixels).unwrap(),
            )));
            host.admitted.clear();
            if !demand.background {
                host.admitted.insert(id, client.generation);
            }
        }
        host.poll(&mut client, device, queue, &mut renderer)
            .unwrap();
        host.present_viewers(&mut client);
        if !preparing && host.retention_usage()[0].1 > 0 {
            host.review_action(id, crate::review::Action::Prepare, &mut client);
            preparing = true;
        }
        if host.review_state(id).1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(rendered.len(), 16);
    assert!(
        host.cache.keys().count() < 16,
        "range must exceed GPU capacity"
    );
    host.review_action(id, crate::review::Action::PlayCached, &mut client);
    let mut times = vec![];
    for offset in 0..16 {
        let key = host.reviews[&id].key(offset);
        client.transport.as_mut().unwrap().time = key.target.unwrap().1;
        let start = std::time::Instant::now();
        loop {
            host.select_viewers(&[(id, Some(key.clone()))], &mut client);
            assert!(
                client.demands.is_empty(),
                "cached playback must not request graph work: offset={offset}, demands={:?}, review={:?}, playing={:?}",
                client.demands,
                host.review_state(id),
                client.transport
            );
            host.poll(&mut client, device, queue, &mut renderer)
                .unwrap();
            host.present_viewers(&mut client);
            if host.cache.peek(&key).is_some() {
                break;
            }
            assert!(start.elapsed().as_secs() < 10, "cache restore stalled");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    eprintln!(
        "Tiered 16-frame 1080p replay: graph requests=0, P50={:.3}ms P95={:.3}ms; {}",
        times[8],
        times[15],
        host.statistics()
    );
    host.release(&mut renderer).unwrap();
}
