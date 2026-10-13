use super::*;

#[test]
#[ignore = "requires native Vulkan"]
fn pressure_evicts_only_to_low_water_and_keeps_recent_entries() {
    let budget = 1024 * 1024;
    let (host, _) = pollster::block_on(Host::headless(budget)).unwrap();
    let mut renderer = Renderer::new(host.clone()).unwrap();
    let status = validation::Status::new(&host).unwrap();
    let mut encoder = host.device().create_command_encoder(&Default::default());
    status.start(&mut encoder);
    status.encode(&mut encoder);
    host.submit(encoder, vec![], vec![]);
    let ready = status.submitted();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready.ready().unwrap() {
        host.poll().unwrap();
        assert!(std::time::Instant::now() < deadline);
    }
    let mut last = None;
    for index in 0..8 {
        let program = Arc::new(crate::coverage::Program {
            nodes: vec![crate::coverage::Op::Constant(index as f32 / 8.)],
            output: 0,
        });
        let image = host
            .image_format([64, 64], wgpu::TextureFormat::R32Float)
            .unwrap();
        renderer.coverage.retain(
            coverage::Retained {
                program: program.clone(),
                size: [64, 64],
                image,
            },
            ready.clone(),
            budget,
        );
        last = Some(program);
    }
    let _held = host.reserve(budget * 65 / 100).unwrap();
    let evicted = renderer.relieve_pressure();
    assert_eq!(evicted, 7);
    assert!(renderer.coverage.get(&last.unwrap(), [64, 64]).is_some());
    assert_eq!(renderer.relieve_pressure(), 0);
}
