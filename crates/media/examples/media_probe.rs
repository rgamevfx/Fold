//! Native-plane timing and import probe; no RGB conversion or hardware required.
use fold_media::{
    Cancel, Decoder,
    ingest::{SourceProfile, inspect_linked},
};
use std::{path::Path, time::Instant};
fn main() -> Result<(), String> {
    let cancel = Cancel::default();
    for path in std::env::args().skip(1) {
        let begin = Instant::now();
        let linked = inspect_linked(Path::new(&path), &cancel)?;
        println!(
            "{path}: import_ms={:.3}",
            begin.elapsed().as_secs_f64() * 1000.
        );
        match linked.metadata.profile {
            SourceProfile::Video(info) => {
                let source = fold_media::VideoSource {
                    path: linked.path,
                    fingerprint: linked.fingerprint,
                    info,
                };
                let mut decoder = Decoder::default();
                let mut elapsed = vec![];
                for i in 0..120.min(source.info.frames) {
                    let begin = Instant::now();
                    let f = decoder.decode_native(&source, source.info.time(i)?, &cancel)?;
                    elapsed.push(begin.elapsed().as_secs_f64() * 1000.);
                    if i == 0 {
                        println!("format={:?} bytes={}", f.format(), f.storage_bytes());
                    }
                }
                println!(
                    "first_ms={:.3} warm_mean_ms={:.3} stats={:?}",
                    elapsed[0],
                    elapsed[1..].iter().sum::<f64>()
                        / elapsed.len().saturating_sub(1).max(1) as f64,
                    decoder.statistics()
                );
                for i in [source.info.frames - 1, 0, source.info.frames / 2] {
                    let begin = Instant::now();
                    decoder.decode_native(&source, source.info.time(i)?, &cancel)?;
                    println!("seek={i} ms={:.3}", begin.elapsed().as_secs_f64() * 1000.);
                }
            }
            SourceProfile::Wave(info) => {
                let source = fold_media::audio::AudioSource::Wave(fold_media::WaveSource {
                    path: linked.path,
                    fingerprint: linked.fingerprint,
                    info,
                });
                let mut decoder = fold_media::audio::AudioDecoder::default();
                let begin = Instant::now();
                decoder.read(&source, 0, 4096, &cancel)?;
                println!(
                    "first_block_ms={:.3}",
                    begin.elapsed().as_secs_f64() * 1000.
                );
            }
            _ => return Err("probe expects video or audio".into()),
        }
    }
    Ok(())
}
