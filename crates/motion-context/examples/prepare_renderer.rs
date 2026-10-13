//! Compile the prepared script scene with the actual runtime cache and bridge identity.
use fold_media::process::Cancel;
use fold_motion_canvas::protocol::Request;
use fold_motion_context::runtime::{Config, Profile, Runtime};
use std::{io::BufRead, path::PathBuf, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Expected scene directory, SDK directory and Rust compiler path".into());
    }
    let directory = PathBuf::from(&args[1]);
    let mut input = std::io::BufReader::new(std::fs::File::open(directory.join("requests.jsonl"))?);
    let mut line = String::new();
    input.read_line(&mut line)?;
    let request: Request = serde_json::from_str(&line)?;
    let cancel = Cancel::default();
    let start = std::time::Instant::now();
    let runtime = Runtime::new(
        Config {
            bridge: PathBuf::from(&args[2]).join("crates/motion-fframes"),
            cache: directory.join("cache"),
            rustc: (&args[3]).into(),
            profile: Profile::Release,
            compile_timeout: Duration::from_secs(600),
            frame_timeout: Duration::from_secs(30),
        },
        &cancel,
    )?;
    let sdk_verification_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = std::time::Instant::now();
    let executable = runtime.prepare(&request.canvas, &cancel)?;
    let program_preparation_ms = start.elapsed().as_secs_f64() * 1000.;
    std::fs::write(
        directory.join("prepared-program.json"),
        serde_json::to_vec(
            &serde_json::json!({"executable":executable,"sdk_verification_ms":sdk_verification_ms,"program_preparation_ms":program_preparation_ms}),
        )?,
    )?;
    println!("{}", executable.display());
    Ok(())
}
