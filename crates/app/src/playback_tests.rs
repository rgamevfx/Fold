use super::*;

#[test]
fn device_timestamp_corrections_cannot_reverse_transport() {
    let playback = Playback::default();
    let shared = &playback.shared;
    shared.end.store(48_000, Ordering::Release);
    shared.sample.store(1_000, Ordering::Release);
    shared
        .stamp
        .store(shared.epoch.elapsed().as_nanos() as u64, Ordering::Release);
    shared.mode.store(2, Ordering::Release);
    let first = playback.sample().unwrap();
    // Device latency estimates can correct backwards. Presentation must hold
    // until the new estimate catches up, not seek backwards within a generation.
    shared.sample.store(900, Ordering::Release);
    shared
        .stamp
        .store(shared.epoch.elapsed().as_nanos() as u64, Ordering::Release);
    assert!(playback.sample().unwrap() >= first);
}
