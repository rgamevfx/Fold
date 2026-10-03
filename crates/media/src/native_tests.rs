use super::*;
use std::os::{fd::AsFd, unix::fs::PermissionsExt};
fn fake_helper(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper");
    std::fs::write(&path,format!("#!/usr/bin/python3\nimport socket,sys,time,os\ns=socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM)\ns.bind(sys.argv[2])\ns.connect(sys.argv[1])\ns.send(b'READY2')\nrequest,ancillary,flags,address=s.recvmsg(24,128)\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (dir, path)
}
fn fixture() -> VideoSource {
    crate::inspect(
        std::path::Path::new(&std::env::var("FOLD_NATIVE_FIXTURE").expect("fixture")),
        &Cancel::default(),
    )
    .unwrap()
}
#[test]
fn geometry_is_explicit() {
    let mut source = VideoSource {
        path: PathBuf::new(),
        fingerprint: String::new(),
        info: crate::VideoInfo {
            width: 46,
            height: 32,
            rate: [30, 1],
            frames: 30,
        },
    };
    assert!(Nv12Layout::for_source(&source).is_err());
    source.info.width = 50;
    assert_eq!(
        Nv12Layout::for_source(&source).unwrap(),
        Nv12Layout {
            dimensions: [50, 32],
            stride: 52,
            uv_offset: 1664,
            bytes: 2496
        }
    );
}
#[test]
#[ignore = "requires verified source fixture; fake helper tests supervision, not NVDEC"]
fn timeout_crash_and_inflight_cancel_reap_helpers() {
    let source = fixture();
    let layout = Nv12Layout::for_source(&source).unwrap();
    let file = std::fs::File::open("/dev/null").unwrap();
    for (body, expected) in [("time.sleep(100)", "deadline"), ("os._exit(23)", "stopped")] {
        let (_dir, helper) = fake_helper(body);
        let mut decoder = NativeDecoder {
            sessions: VecDeque::new(),
            helper,
            timeout: Duration::from_millis(500),
            statistics: NativeStatistics::default(),
        };
        for _ in 0..9 {
            let start = Instant::now();
            let error = decoder
                .decode_into(
                    &source,
                    source.info.time(0).unwrap(),
                    fold_native_video::WriteTicket::new(
                        file.as_fd(),
                        [0; 16],
                        layout.bytes,
                        layout.bytes,
                    ),
                    &Cancel::default(),
                )
                .unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(start.elapsed() < Duration::from_secs(2));
            assert!(decoder.sessions.is_empty());
            let stats = decoder.statistics();
            assert_eq!(stats.requests, 0);
            assert_eq!(stats.codec_call_nanoseconds, 0);
            assert_eq!(stats.copy_ready_nanoseconds, 0);
            assert_eq!(stats.gpu_local_copy_bytes, 0);
        }
    }
    let (_dir, helper) = fake_helper("time.sleep(100)");
    let cancel = Cancel::default();
    let worker = cancel.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        worker.cancel();
    });
    let mut decoder = NativeDecoder {
        sessions: VecDeque::new(),
        helper,
        timeout: Duration::from_secs(3),
        statistics: NativeStatistics::default(),
    };
    let start = Instant::now();
    let error = decoder
        .decode_into(
            &source,
            source.info.time(0).unwrap(),
            fold_native_video::WriteTicket::new(file.as_fd(), [0; 16], layout.bytes, layout.bytes),
            &cancel,
        )
        .unwrap_err();
    thread.join().unwrap();
    assert!(error.contains("cancel"), "{error}");
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(decoder.statistics().gpu_local_copy_bytes, 0);
}
#[test]
#[ignore = "requires verified fixture; deterministic fake-helper telemetry"]
fn completed_telemetry_accumulates_and_saturates() {
    let source = fixture();
    let layout = Nv12Layout::for_source(&source).unwrap();
    let file = std::fs::File::open("/dev/null").unwrap();
    let (_dir, helper) = fake_helper(
        "while True:\n s.send(b'FNVACK02'+request[:8]+(123).to_bytes(8,'little')+(456).to_bytes(8,'little'))\n request,ancillary,flags,address=s.recvmsg(24,128)",
    );
    let mut decoder = NativeDecoder {
        sessions: VecDeque::new(),
        helper,
        timeout: Duration::from_secs(3),
        statistics: NativeStatistics::default(),
    };
    for frame in 0..2 {
        let complete = decoder
            .decode_into(
                &source,
                source.info.time(frame).unwrap(),
                fold_native_video::WriteTicket::new(
                    file.as_fd(),
                    [0; 16],
                    layout.bytes,
                    layout.bytes,
                ),
                &Cancel::default(),
            )
            .unwrap();
        assert_eq!(complete.codec_call_nanoseconds(), 123);
        assert_eq!(complete.copy_ready_nanoseconds(), 456);
    }
    let stats = decoder.statistics();
    assert_eq!(stats.requests, 2);
    assert_eq!(stats.codec_call_nanoseconds, 246);
    assert_eq!(stats.copy_ready_nanoseconds, 912);
    assert_eq!(stats.gpu_local_copy_bytes, layout.bytes * 2);
    decoder.statistics.requests = u64::MAX;
    decoder.statistics.forward_reuses = u64::MAX;
    decoder.statistics.codec_call_nanoseconds = u64::MAX - 1;
    decoder.statistics.copy_ready_nanoseconds = u64::MAX - 1;
    decoder.statistics.gpu_local_copy_bytes = u64::MAX - 1;
    decoder
        .decode_into(
            &source,
            source.info.time(2).unwrap(),
            fold_native_video::WriteTicket::new(file.as_fd(), [0; 16], layout.bytes, layout.bytes),
            &Cancel::default(),
        )
        .unwrap();
    let stats = decoder.statistics();
    assert_eq!(stats.requests, u64::MAX);
    assert_eq!(stats.forward_reuses, u64::MAX);
    assert_eq!(stats.codec_call_nanoseconds, u64::MAX);
    assert_eq!(stats.copy_ready_nanoseconds, u64::MAX);
    assert_eq!(stats.gpu_local_copy_bytes, u64::MAX);
}
