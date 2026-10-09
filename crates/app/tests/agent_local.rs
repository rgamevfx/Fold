#![cfg(target_os = "linux")]
use fold_app::{agent::local, session::Session};
use fold_platform::desktop::DesktopClient;
use serde_json::{Value, json};
use std::{
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn cli_and_socket_use_the_native_dispatcher_and_private_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let path = session.enable_agent_transport(directory.path()).unwrap();
    assert_eq!(
        std::fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let request =
        serde_json::to_vec(&json!({"version":2,"command":{"method":"discover"}})).unwrap();
    let expected: Value = serde_json::from_slice(&session.dispatch_agent(&request)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fold-cli"))
        .arg("protocol")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&request).unwrap();
    // Drain stdout concurrently: discovery/schema can exceed a pipe's capacity.
    let result = std::thread::spawn(move || child.wait_with_output().unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !result.is_finished() {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let result = result.join().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        expected
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(
        local::request(&path, &request).unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    drop(session);
    assert!(!path.exists());
}

#[test]
fn socket_rejects_old_versions() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let path = session.enable_agent_transport(directory.path()).unwrap();
    let worker = std::thread::spawn(move || {
        local::request(&path, br#"{"version":1,"command":{"method":"discover"}}"#).unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    let result: Value = serde_json::from_slice(&worker.join().unwrap()).unwrap();
    assert_eq!(result["error"]["code"], "version");
}

#[test]
fn transport_does_not_normalize_retry_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = Session::default();
    let path = session.enable_agent_transport(directory.path()).unwrap();
    let info: Value = serde_json::from_slice(
        &session.dispatch_agent(br#"{"version":2,"command":{"method":"discover"}}"#),
    )
    .unwrap();
    let request = serde_json::to_vec(&json!({"version":2,"command":{"method":"unknown","guard":{
        "epoch":info["epoch"],"request_id":fold_foundation::ObjectId::new()
    }}}))
    .unwrap();
    let worker = std::thread::spawn(move || {
        let first = local::request(&path, &request).unwrap();
        assert_eq!(local::request(&path, &request).unwrap(), first);
        let mut changed = request;
        changed.push(b' ');
        let conflict: Value =
            serde_json::from_slice(&local::request(&path, &changed).unwrap()).unwrap();
        assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        session.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap();
}
