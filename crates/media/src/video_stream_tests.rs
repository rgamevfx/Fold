use super::*;
use std::process::Stdio;

#[test]
fn prefetched_frames_remain_ordered_and_last_frame_survives_eof() {
    let mut command = Command::new("python3");
    command
        .args([
            "-c",
            "import sys; sys.stdout.buffer.write(bytes(range(48)))",
        ])
        .stdout(Stdio::piped());
    let mut stream = Stream::new(command, 16).unwrap();
    for frame in 0..3 {
        let (bytes, _lease) = stream.read(&Cancel::default()).unwrap();
        assert_eq!(bytes, (frame * 16..frame * 16 + 16).collect::<Vec<u8>>());
    }
    assert!(stream.read(&Cancel::default()).is_err());
}

#[test]
fn blocked_pipe_cancels_and_reaps_promptly() {
    let mut command = Command::new("python3");
    command
        .args(["-c", "import time; time.sleep(60)"])
        .stdout(Stdio::piped());
    let mut stream = Stream::new(command, 16).unwrap();
    let cancel = Cancel::default();
    let token = cancel.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        token.cancel();
    });
    let start = Instant::now();
    assert!(stream.read(&cancel).unwrap_err().message.contains("cancel"));
    assert!(
        stream
            .read(&Cancel::default())
            .unwrap_err()
            .message
            .contains("stopped")
    );
    drop(stream);
    worker.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
}
#[test]
fn failed_child_reports_diagnostics_and_short_frame() {
    let mut command = Command::new("python3");
    command.args(["-c", "import sys; sys.stdout.buffer.write(b'bad'); sys.stderr.write('fixture decoder failure'); sys.exit(2)"])
        .stdout(Stdio::piped());
    let mut stream = Stream::new(command, 16).unwrap();
    let error = stream.read(&Cancel::default()).unwrap_err();
    assert!(error.message.contains("incomplete"), "{error}");
    assert!(error.message.contains("fixture decoder failure"), "{error}");
}
