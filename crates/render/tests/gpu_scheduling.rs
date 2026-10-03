#![cfg(all(feature = "native-video", target_os = "linux"))]
use fold_media::{Cancel, Decoder};
use fold_render::{
    ImageOp, RenderGraph,
    gpu::{Host, Renderer},
    scheduling::{Class, Scheduler},
};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires native GPU/helper, source fixture and pinned OCIO runtime"]
fn video_prepares_while_graphics_is_busy_and_cancels_its_wait() {
    let source = fold_media::inspect(
        std::path::Path::new(&std::env::var("FOLD_NATIVE_FIXTURE").unwrap()),
        &Cancel::default(),
    )
    .unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("FOLD_TEST_COLOR_ROOT").unwrap());
    let (host, _) = pollster::block_on(Host::headless(128 * 1024 * 1024)).unwrap();
    let scheduler = Scheduler::shared();
    let held = scheduler.enter(Class::Viewer, &Cancel::default()).unwrap();
    let cancel = Cancel::default();
    let token = cancel.clone();
    let worker_host = host.clone();
    let worker = std::thread::spawn(move || {
        let runtime = fold_color::Runtime::at(&root).unwrap();
        let config = runtime.bundled().unwrap();
        let mut renderer = Renderer::new(worker_host).unwrap();
        let graph = RenderGraph {
            width: source.info.width,
            height: source.info.height,
            nodes: vec![ImageOp::Video {
                source,
                time: fold_foundation::Time::ZERO,
            }],
            output: 0,
        };
        renderer
            .evaluate_scheduled(
                graph,
                Some(&config),
                &mut Decoder::from_environment().unwrap(),
                &token,
                Class::Export,
            )
            .err()
            .expect("waiting request must be cancelled")
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    while scheduler.queue_depth().0 == 0 && Instant::now() < deadline && !worker.is_finished() {
        host.poll().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    let prepared = host.memory().decoded > 0 && scheduler.queue_depth().0 == 1;
    cancel.cancel();
    drop(held);
    let error = worker.join().unwrap();
    assert!(
        prepared,
        "source must become ready before waiting for graphics admission"
    );
    assert!(error.contains("cancel"), "{error}");
    assert_eq!(scheduler.queue_depth().0, 0);
    assert!(scheduler.enter(Class::Viewer, &Cancel::default()).is_ok());
}
