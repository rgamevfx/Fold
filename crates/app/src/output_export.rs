//! Pinned committed-snapshot delivery; cancellation cannot retract finalized files.
use super::{SceneRequest, output};
use fold_media::{Cancel, Encoder};
use std::path::Path;
/// Export only a coordinator-published revision, never a live gesture overlay.
/// ```compile_fail
/// use fold_app::output::export;
/// use fold_project::Snapshot;
/// fn export_preview(preview: &Snapshot) {
///     export(preview, std::path::Path::new("preview.mp4"), 0, 1,
///         &fold_media::Cancel::default()).unwrap();
/// }
/// ```
pub fn export(
    snapshot: &fold_project::CommittedSnapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
) -> Result<(), String> {
    export_inner(
        snapshot,
        path,
        start,
        end,
        cancel,
        #[cfg(feature = "gpu")]
        None,
    )
}

#[cfg(feature = "gpu")]
pub(crate) fn export_with_host(
    snapshot: &fold_project::CommittedSnapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
    host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    export_inner(snapshot, path, start, end, cancel, host)
}

fn export_inner(
    snapshot: &fold_project::CommittedSnapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
    #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    use fold_foundation::Rounding;
    use fold_media::audio::{AUDIO_RATE, AudioDecoder};
    use std::io::Write;
    let (reference, info) = output(snapshot)?;
    let registry = crate::packages::builtins();
    if !registry.supports_audio(snapshot, reference.document) {
        return export_video(
            snapshot,
            path,
            start,
            end,
            cancel,
            #[cfg(feature = "gpu")]
            host,
        );
    }
    if path.exists() || start >= end || end > info.frames {
        return Err("invalid export range or destination exists".into());
    }
    let plan = registry.audio(snapshot, reference.document)?;
    let mut decoder = AudioDecoder::default();
    plan.preflight(&mut decoder, cancel)?;
    let _video_scratch = fold_media::budget::reserve_delivery(2 * 1024 * 1024 * 1024)?;
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let video = temporary.path().join(if prores_destination(path)? {
        "video.mov"
    } else {
        "video.mp4"
    });
    let pcm = temporary.path().join("audio.f32");
    let mut file = std::fs::File::create(&pcm).map_err(|e| e.to_string())?;
    let mut sample = info
        .time(start)?
        .to_ticks(AUDIO_RATE, 1, Rounding::Ceil)
        .map_err(|e| e.to_string())? as u64;
    let end_sample = info
        .time(end)?
        .to_ticks(AUDIO_RATE, 1, Rounding::Ceil)
        .map_err(|e| e.to_string())? as u64;
    let _pcm_scratch = fold_media::budget::reserve_delivery_pcm((end_sample - sample) * 8)?;
    while sample < end_sample {
        let count = (end_sample - sample).min(4096) as usize;
        let block = plan.evaluate(sample, count, &mut decoder, cancel)?;
        let bytes: Vec<u8> = block
            .into_iter()
            .flatten()
            .flat_map(f32::to_le_bytes)
            .collect();
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        sample += count as u64;
    }
    drop(file);
    // Delivery PCM is complete. Release prepared PCM files while the decoder
    // still retains immutable video-source pins for the pinned export.
    decoder.retain_sources([]);
    export_video(
        snapshot,
        &video,
        start,
        end,
        cancel,
        #[cfg(feature = "gpu")]
        host,
    )?;
    Ok(fold_media::audio::mux_audio(&video, &pcm, path, cancel)?)
}

fn prores_destination(path: &Path) -> Result<bool, String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mov") => Ok(true),
        Some("mp4") => Ok(false),
        _ => Err("Choose .mp4 (H.264) or .mov (ProRes 4444 with alpha)".into()),
    }
}

fn export_video(
    snapshot: &fold_project::CommittedSnapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
    #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    let (source, info) = output(snapshot)?;
    if start >= end || end > info.frames {
        return Err("export requires nonempty half-open [start,end) within the sequence".into());
    }
    #[cfg(feature = "gpu")]
    let mut evaluator = crate::output::OutputRenderer::with_shared_host(host)?;
    #[cfg(not(feature = "gpu"))]
    let mut evaluator = crate::output::OutputRenderer::from_environment()?;
    let mut request = SceneRequest {
        region: None,
        preview: None,
        source,
        time: info.time(start)?,
        dimensions: [info.width, info.height],
    };
    let prores = prores_destination(path)?;
    if prores {
        let mut first = Some(evaluator.evaluate_rgba16(snapshot, &request, cancel)?);
        let output_color = fold_platform::color::project(snapshot)?
            .map(|_| fold_platform::color::output(snapshot, request.source.document))
            .transpose()?;
        let rec709 = match output_color.as_ref().map(|c| c.display.as_str()) {
            Some("Rec.1886 Rec.709 - Display") => true,
            Some("sRGB - Display") | None => false,
            _ => {
                return Err(
                    "ProRes 4444 requires an sRGB or Rec.1886 Rec.709 output transform".into(),
                );
            }
        };
        let mut encoder =
            Encoder::new_prores4444(path, &info, end - start, cancel.clone(), rec709)?;
        for frame in start..end {
            cancel.check()?;
            request.time = info.time(frame)?;
            let output = match first.take() {
                Some(frame) => frame,
                None => evaluator.evaluate_rgba16(snapshot, &request, cancel)?,
            };
            encoder.write_rgba16(output.bytes())?;
        }
        return Ok(encoder.finish()?);
    }
    let mut evaluate_output =
        |request: &SceneRequest| evaluator.evaluate(snapshot, request, cancel);
    // Preflight decode AND the explicit output transform before any destination.
    let mut first = Some(evaluate_output(&request)?);
    let output_color = fold_platform::color::project(snapshot)?
        .map(|_| fold_platform::color::output(snapshot, request.source.document))
        .transpose()?;
    let mut encoder = match output_color.as_ref().map(|c| c.display.as_str()) {
        Some("Rec.1886 Rec.709 - Display") => {
            Encoder::new_rec709(path, &info, end - start, cancel.clone())?
        }
        Some("sRGB - Display") | None => Encoder::new(path, &info, end - start, cancel.clone())?,
        _ => return Err("MP4 requires an sRGB or Rec.1886 Rec.709 output transform".into()),
    };
    for frame in start..end {
        cancel.check()?;
        request.time = info.time(frame)?;
        let display = match first.take() {
            Some(display) => display,
            None => evaluate_output(&request)?,
        };
        let rgb: Vec<u8> = display
            .rgba()
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect();
        encoder.write_rgb(&rgb)?;
    }
    Ok(encoder.finish()?)
}
