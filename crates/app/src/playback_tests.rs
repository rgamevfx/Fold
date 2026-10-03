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

#[test]
fn loop_mapping_preserves_absolute_ring_positions_and_marked_boundaries() {
    let range = LoopRange {
        start: 2,
        end: 6,
        numerator: 2,
        duration: 4,
        denominator: 1,
    };
    assert_eq!(source_window(4, 6, Some(range), 4), (4, 2));
    assert_eq!(source_window(6, 6, Some(range), 4), (2, 4));
    assert_eq!(source_window(10, 6, Some(range), 4), (2, 4));
    // A late producer skips absolute positions but resumes the matching source.
    assert_eq!(source_window(15, 6, Some(range), 4), (3, 3));
    assert_eq!(source_window(8, 6, None, 4), (6, 0));
    let mut cursor = 4;
    let mut samples = Vec::new();
    while samples.len() < 12 {
        let (source, count) = source_window(cursor, 6, Some(range), 12 - samples.len());
        samples.extend((source..source + count as u64).map(|value| {
            let sample = (cursor, value);
            cursor += 1;
            sample
        }));
    }
    assert_eq!(
        samples,
        [
            (4, 4),
            (5, 5),
            (6, 2),
            (7, 3),
            (8, 4),
            (9, 5),
            (10, 2),
            (11, 3),
            (12, 4),
            (13, 5),
            (14, 2),
            (15, 3)
        ]
    );
}

#[test]
fn fractional_rate_loop_boundaries_do_not_accumulate_sample_rounding() {
    let range = LoopRange {
        start: 1602,
        end: 3204,
        numerator: 48_000 * 1001,
        duration: 48_000 * 1001,
        denominator: 30_000,
    };
    let mut cursor = range.start;
    for cycle in 0..10_000u128 {
        let boundary =
            (range.numerator + (cycle + 1) * range.duration).div_ceil(range.denominator) as u64;
        while cursor < boundary {
            let (source, count) = source_window(cursor, range.end, Some(range), 4096);
            assert!(count > 0 && source >= range.start && source + count as u64 <= range.end);
            cursor += count as u64;
        }
        assert_eq!(cursor, boundary);
    }
    assert_eq!(
        cursor,
        (range.numerator + 10_000 * range.duration).div_ceil(range.denominator) as u64
    );
    assert_ne!(cursor, range.start + 10_000 * (range.end - range.start));
}
