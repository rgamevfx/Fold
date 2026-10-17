//! Pinned committed-snapshot delivery; cancellation cannot retract finalized files.
use super::{SceneRequest, output};
use fold_media::{Cancel, Encoder, encoding::Movie};
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
    export_configured(
        snapshot,
        path,
        start,
        end,
        Movie::from_path(path)?,
        cancel,
        #[cfg(feature = "gpu")]
        None,
    )
}

/// Delivery-panel movie export, including audio when the selected output supplies it.
pub fn export_configured(
    snapshot: &fold_project::CommittedSnapshot,
    path: &Path,
    start: u32,
    end: u32,
    encoding: Movie,
    cancel: &Cancel,
    #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    encoding.validate_path(path)?;
    use fold_foundation::Rounding;
    use fold_media::audio::{AUDIO_RATE, AudioDecoder};
    use std::io::Write;
    let (reference, info) = output(snapshot)?;
    let transform = fold_platform::color::output(snapshot, reference.document)?;
    let registry = crate::packages::builtins();
    if !registry.supports_audio(snapshot, reference.document) {
        return export_movie(
            snapshot,
            &reference,
            path,
            start..end,
            encoding,
            &transform,
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
    let video = temporary
        .path()
        .join(if matches!(encoding, Movie::ProRes(_)) {
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
    export_movie(
        snapshot,
        &reference,
        &video,
        start..end,
        encoding,
        &transform,
        cancel,
        #[cfg(feature = "gpu")]
        host,
    )?;
    Ok(fold_media::audio::mux_audio(&video, &pcm, path, cancel)?)
}

/// The same branch-local movie renderer serves Write and Delivery. Source identity
/// and color are explicit; this never changes the project's delivery selection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn export_movie(
    snapshot: &fold_project::CommittedSnapshot,
    source: &fold_project::DocumentRef,
    path: &Path,
    frames: std::ops::Range<u32>,
    encoding: Movie,
    transform: &fold_platform::color::OutputTransform,
    cancel: &Cancel,
    #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    encoding.validate_path(path)?;
    cancel.check()?;
    let info = crate::packages::builtins().output_ref(snapshot, source)?;
    if frames.start >= frames.end || frames.end > info.frames || path.exists() {
        return Err("Invalid export range or destination already exists".into());
    }
    let rec709 = if fold_platform::color::project(snapshot)?.is_some() {
        match transform.display.as_str() {
            "Rec.1886 Rec.709 - Display" => true,
            "sRGB - Display" => false,
            _ => return Err("Movies require an sRGB or Rec.1886 Rec.709 output transform".into()),
        }
    } else {
        false
    };
    #[cfg(feature = "gpu")]
    let mut evaluator = crate::output::OutputRenderer::with_shared_host(host)?;
    #[cfg(not(feature = "gpu"))]
    let mut evaluator = crate::output::OutputRenderer::from_environment()?;
    let mut request = SceneRequest {
        region: None,
        preview: None,
        source: source.clone(),
        time: info.time(frames.start)?,
        dimensions: [info.width, info.height],
    };
    // Validate decode and output conversion before starting the atomic encoder.
    let mut first = Some(evaluator.evaluate_movie_frame(
        snapshot,
        &request,
        transform,
        encoding.preserves_alpha(),
        cancel,
    )?);
    let encoder = Encoder::new_with_signal(
        path,
        &info,
        frames.end - frames.start,
        cancel.clone(),
        rec709,
        encoding,
    )?;
    write_pipeline(
        encoder,
        frames,
        cancel,
        |frame| {
            request.time = info.time(frame)?;
            match first.take() {
                Some(frame) => Ok(frame),
                None => Ok(evaluator.evaluate_movie_frame(
                    snapshot,
                    &request,
                    transform,
                    encoding.preserves_alpha(),
                    cancel,
                )?),
            }
        },
        |encoder, frame| match encoding {
            Movie::ProRes(_) => encoder.write_rgba16(frame.bytes()).map_err(Into::into),
            Movie::H264(_) => {
                let _lease = fold_media::budget::reserve_output(
                    u64::from(info.width) * u64::from(info.height) * 3,
                )?;
                let rgb: Vec<u8> = frame
                    .bytes()
                    .chunks_exact(8)
                    .flat_map(|p| {
                        p[..6].chunks_exact(2).map(|c| {
                            ((u32::from(u16::from_le_bytes([c[0], c[1]])) + 128) / 257) as u8
                        })
                    })
                    .collect();
                encoder.write_rgb(&rgb).map_err(Into::into)
            }
        },
    )
}

/// The writer owns the encoder. Only a complete sequence may finalize it; a
/// failed producer closes the channel and dropping Encoder removes its temporary.
/// One queued frame overlaps bounded CPU conversion/pipe IO with rendering.
fn write_pipeline<T: Send>(
    mut encoder: Encoder,
    frames: std::ops::Range<u32>,
    cancel: &Cancel,
    mut render: impl FnMut(u32) -> Result<T, String>,
    mut write: impl FnMut(&mut Encoder, T) -> Result<(), String> + Send,
) -> Result<(), String> {
    let count = frames.len();
    std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let writer = scope.spawn(move || -> Result<(), String> {
            for _ in 0..count {
                cancel.check()?;
                let frame = receive.recv().map_err(|_| "export rendering stopped")?;
                write(&mut encoder, frame)?;
            }
            cancel.check()?;
            Ok(encoder.finish()?)
        });
        let produced = (|| -> Result<(), String> {
            for frame in frames {
                cancel.check()?;
                let output = render(frame)?;
                send.send(output).map_err(|_| "export writer stopped")?;
            }
            Ok(())
        })();
        drop(send);
        let written = writer.join().map_err(|_| "export writer panicked")?;
        // Preserve the producer's concrete decode/render failure; otherwise
        // return the writer's codec/IO error instead of a channel symptom.
        match produced {
            Err(error) if error != "export writer stopped" => Err(error),
            _ => written,
        }
    })
}
