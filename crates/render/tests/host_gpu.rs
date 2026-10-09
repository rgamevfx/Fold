#![cfg(feature = "gpu")]
use fold_render::gpu::Host;
use std::sync::Arc;

#[test]
#[ignore = "requires a native Vulkan adapter"]
fn concurrent_gpu_host_creation() {
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                pollster::block_on(Host::headless(8 * 1024 * 1024))
                    .unwrap()
                    .0
            })
        })
        .collect();
    let hosts: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_ne!(hosts[0].id(), hosts[1].id());
}
