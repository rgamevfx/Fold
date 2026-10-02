//! Isolated codec-stage baseline; not sustained playback or end-to-end export.
//! stage_probe <verified-source.mp4> <new-output.mp4>
use fold_media::{Cancel, Decoder, Encoder, inspect};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 || args[1].exists() {
        return Err("expected source.mp4 and a new output.mp4".into());
    }
    let cancel = Cancel::default();
    let begin = Instant::now();
    let source = inspect(&args[0], &cancel)?;
    let inspect_ms = begin.elapsed().as_secs_f64() * 1000.;
    let size = [source.info.width, source.info.height];
    let mut decoder = Decoder::default();
    let mut decode_ms = Vec::new();
    for _ in 0..2 {
        let begin = Instant::now();
        std::hint::black_box(decoder.decode(&source, source.info.time(0)?, size, &cancel)?);
        decode_ms.push(begin.elapsed().as_secs_f64() * 1000.);
    }
    let rgb = vec![128; size[0] as usize * size[1] as usize * 3];
    let count = source.info.frames.min(30);
    let begin = Instant::now();
    let mut encoder = Encoder::new(&args[1], &source.info, count, cancel)?;
    for _ in 0..count {
        encoder.write_rgb(&rgb)?;
    }
    encoder.finish()?;
    let encode_ms = begin.elapsed().as_secs_f64() * 1000.;
    // Fresh decoder: includes source pin/probe cancellation, not GPU preemption.
    // Report whether work was still active instead of calling an early finish
    // a successful cancellation measurement.
    let cancel = Cancel::default();
    let worker_cancel = cancel.clone();
    let worker = std::thread::spawn(move || {
        Decoder::default().decode(&source, source.info.time(0).unwrap(), size, &worker_cancel)
    });
    std::thread::sleep(std::time::Duration::from_millis(5));
    let active_at_cancel = !worker.is_finished();
    let begin = Instant::now();
    cancel.cancel();
    let result = worker.join().map_err(|_| "decode worker panicked")?;
    let cancel_join_ms = begin.elapsed().as_secs_f64() * 1000.;
    let error = result.err();
    if active_at_cancel && !error.as_ref().is_some_and(|e| e.contains("cancelled")) {
        return Err(format!("active decode did not report cancellation: {error:?}").into());
    }
    println!(
        "{}",
        serde_json::json!({
            "inspect_ms": inspect_ms, "cold_decode_ms": decode_ms[0],
            "warm_same_frame_decode_ms": decode_ms[1], "encode_frames": count,
            "encode_wall_ms": encode_ms, "active_at_cancel": active_at_cancel,
            "cancel_to_join_ms": cancel_join_ms, "cancel_error": error,
            "note": "repeat synthetic RGB8 midgray frame; encode includes conversion/process/finalization, no audio"
        })
    );
    Ok(())
}
