//! Release-mode retained decoder timing/copy probe. No UI or color runtime needed.
use fold_media::{Cancel, DecodeBackend, Decoder};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: decode_probe SOURCE [frames=90] [width=480] [height=270]")?;
    let frames: u32 = args
        .get(2)
        .map_or(Ok(90), |v| v.parse())
        .map_err(|e| format!("frames: {e}"))?;
    let dimensions = [
        args.get(3)
            .map_or(Ok(480), |v| v.parse())
            .map_err(|e| format!("width: {e}"))?,
        args.get(4)
            .map_or(Ok(270), |v| v.parse())
            .map_err(|e| format!("height: {e}"))?,
    ];
    let cancel = Cancel::default();
    let start = Instant::now();
    let source = fold_media::inspect(Path::new(path), &cancel)?;
    println!(
        "inspect_ms={:.3} source={:?}",
        start.elapsed().as_secs_f64() * 1000.,
        source.info
    );
    if frames.min(source.info.frames) < 2 {
        return Err("timing probe requires at least two frames".into());
    }
    let mut expected = Vec::new();
    for backend in [DecodeBackend::Software, DecodeBackend::Cuda] {
        let mut decoder = Decoder::with_backend(backend);
        let mut times = Vec::new();
        for frame in 0..frames.min(source.info.frames) {
            let start = Instant::now();
            let image =
                decoder.decode_signal(&source, source.info.time(frame)?, dimensions, &cancel)?;
            times.push(start.elapsed().as_secs_f64() * 1000.);
            let mut hash = Sha256::new();
            for sample in image.native_gbr_planes().unwrap() {
                hash.update(sample.to_le_bytes());
            }
            let hash = hash.finalize();
            if backend == DecodeBackend::Software {
                expected.push(hash);
            } else if hash != expected[frame as usize] {
                return Err(format!("hardware mismatch at frame {frame}"));
            }
        }
        let cold = times[0];
        let mut warm = times[1..].to_vec();
        warm.sort_by(f64::total_cmp);
        println!(
            "{backend:?} {dimensions:?}: first_ms={cold:.3} warm_mean_ms={:.3} warm_p95_ms={:.3} statistics={:?} retained_bytes={} shared_source_bytes={}",
            warm.iter().sum::<f64>() / warm.len() as f64,
            warm[warm.len() * 95 / 100],
            decoder.statistics(),
            decoder.retained_bytes(),
            fold_media::pinned_source_bytes()
        );
        for frame in [source.info.frames - 1, 0, source.info.frames / 2, 1] {
            let start = Instant::now();
            decoder.decode_signal(&source, source.info.time(frame)?, dimensions, &cancel)?;
            println!(
                "{backend:?} seek={frame} ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
    println!("Software/CUDA GBR float planes matched exactly for all measured sequential frames.");
    Ok(())
}
