//! Exercise the real retained runtime with the prepared native/script scene.
use fold_media::process::Cancel;
use fold_motion_canvas::protocol::{Alpha, Encoding, Request, Response};
use fold_motion_context::runtime::{Config, Profile, Runtime};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    time::Duration,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Expected prepared scene directory")?,
    );
    let sdk = PathBuf::from(std::env::args().nth(2).ok_or("Expected SDK directory")?);
    let rustc = PathBuf::from(
        std::env::args()
            .nth(3)
            .ok_or("Expected Rust compiler executable")?,
    );
    let profile = match std::env::args().nth(4).as_deref() {
        None | Some("debug") => Profile::Debug,
        Some("release") => Profile::Release,
        Some(_) => return Err("Expected debug or release profile".into()),
    };
    let cancel = Cancel::default();
    let mut runtime = Runtime::new(
        Config {
            bridge: sdk.join("crates/motion-fframes"),
            cache: directory.join("cache"),
            rustc,
            profile,
            compile_timeout: Duration::from_secs(600),
            frame_timeout: Duration::from_secs(30),
        },
        &cancel,
    )?;
    let mut output =
        std::io::BufWriter::new(std::fs::File::create(directory.join("runtime-frames.bin"))?);
    let input = std::io::BufReader::new(std::fs::File::open(directory.join("requests.jsonl"))?);
    let mut fonts = Vec::new();
    for line in input.lines() {
        let mut request: Request = serde_json::from_str(&line?)?;
        if let Some(bytes) = request.fonts {
            fonts = bytes;
        }
        if request.id == 2 {
            for object in request.canvas.objects.values_mut() {
                if let fold_motion_canvas::model::Content::Script { controls, .. } =
                    &mut object.content
                {
                    for control in controls.values_mut() {
                        if let fold_motion_canvas::model::ControlValue::Number(value) =
                            &mut control.value
                        {
                            value.value = 8000.;
                        }
                    }
                }
            }
        }
        let start = std::time::Instant::now();
        let frame = runtime.render(
            &request.canvas,
            request.time,
            request.dimensions,
            fold_motion_context::runtime::Inputs {
                fonts: &fonts,
                assets: &[],
            },
            &cancel,
        )?;
        eprintln!("request {}: {:?}", request.id, start.elapsed());
        serde_json::to_writer(
            &mut output,
            &Response {
                id: request.id,
                dimensions: frame.dimensions,
                bytes: frame.pixels().len() * 16,
                encoding: Encoding::LinearSrgb,
                alpha: Alpha::Premultiplied,
                error: None,
            },
        )?;
        output.write_all(b"\n")?;
        for pixel in frame.pixels() {
            for value in pixel {
                output.write_all(&value.to_le_bytes())?;
            }
        }
    }
    output.flush()?;
    Ok(())
}
