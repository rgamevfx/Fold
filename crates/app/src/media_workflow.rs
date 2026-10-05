use fold_foundation::AssetId;
use fold_media::{Cancel, Decoder, Encoder, VideoInfo};
use fold_platform::desktop::PreviewKey;
use fold_project::{Asset, DocumentRef, EditBatch, Mutation, Snapshot};
use fold_timeline::VideoLayers;
use std::path::{Path, PathBuf};

pub fn import(
    snapshot: &Snapshot,
    paths: &[PathBuf],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    if crate::timeline_workflow::has_sequence(snapshot) {
        return Err(
            "An editable sequence is active. Use Import sequence, or open a layer-only project."
                .into(),
        );
    }
    if !(1..=2).contains(&paths.len()) {
        return Err("import requires one or two matching videos".into());
    }
    let mut mutations = Vec::new();
    let mut ids = Vec::new();
    let mut info = None;
    for path in paths {
        let video = fold_media::inspect(path, cancel)?;
        if info.as_ref().is_some_and(|i| i != &video.info) {
            return Err("two-layer import currently requires matching dimensions, frame rate, and frame count".into());
        }
        info = Some(video.info);
        let id = AssetId::new();
        ids.push(id);
        mutations.push(Mutation::PutAsset(Asset {
            id,
            location: video
                .path
                .to_str()
                .ok_or("media path must be UTF-8")?
                .into(),
            fingerprint: video.fingerprint,
            extensions: Default::default(),
        }));
    }
    // Replace the active media document, preserving unknown documents/assets.
    let previous = active_document(snapshot);
    let id = previous.map(|d| d.id).unwrap_or_default();
    let extensions = previous
        .map(VideoLayers::from_document)
        .transpose()?
        .map(|layers| layers.extensions)
        .unwrap_or_default();
    let mut document = VideoLayers {
        assets: ids,
        info: info.unwrap(),
        foreground_opacity: 0.5,
        extensions,
    }
    .document(id)?;
    if let Some(previous) = previous {
        document.extensions = previous.extensions.clone();
    }
    mutations.push(Mutation::PutDocument(document));
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations,
    })
}

fn active_document(snapshot: &Snapshot) -> Option<&fold_project::Document> {
    snapshot
        .state()
        .documents
        .values()
        .find(|d| {
            d.package_id == fold_timeline::PACKAGE
                && d.type_id == fold_timeline::VIDEO_LAYERS
                && d.schema_version == 1
        })
        .map(|d| d.as_ref())
}

pub fn active(snapshot: &Snapshot) -> Result<(DocumentRef, VideoLayers), String> {
    let document = active_document(snapshot).ok_or("no supported video layers imported")?;
    Ok((
        DocumentRef {
            document: document.id,
            output: "video".into(),
            extensions: Default::default(),
        },
        VideoLayers::from_document(document)?,
    ))
}

/// Hash authoring bytes, ordered asset identities, and implementation versions.
/// Unrelated project revisions and undo/redo do not invalidate viewer textures.
pub fn content(snapshot: &Snapshot) -> Result<String, String> {
    let (reference, _) = output(snapshot)?;
    content_for(snapshot, reference.document)
}

pub fn content_for(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
) -> Result<String, String> {
    let mut data =
        b"fold-video-evaluator-v7;analytic-vectors-all-spaces;gpu-operators-v1;rgba32f;ocio-2.4.2;aces-srgb-view-v1;native709-float;nearest;".to_vec();
    if let Some(color) = fold_platform::color::project(snapshot)? {
        data.extend(serde_json::to_vec(&color).map_err(|e| e.to_string())?);
    }
    let mut pending = vec![document];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let document = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("missing nested document")?;
        let packages = crate::packages::builtins();
        let identity = if packages.supports(document) {
            packages.evaluation_identity(document)?
        } else {
            document.payload.clone()
        };
        for bytes in [
            document.package_id.as_bytes(),
            document.type_id.as_bytes(),
            &identity,
        ] {
            data.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        data.extend_from_slice(&document.schema_version.to_le_bytes());
        let references = serde_json::to_vec(&document.dependencies).map_err(|e| e.to_string())?;
        data.extend_from_slice(&(references.len() as u64).to_le_bytes());
        data.extend_from_slice(&references);
        data.extend_from_slice(&(document.assets.len() as u64).to_le_bytes());
        for id in &document.assets {
            data.extend_from_slice(&serde_json::to_vec(id).map_err(|e| e.to_string())?);
            let asset = snapshot
                .state()
                .assets
                .get(id)
                .ok_or("missing video asset")?;
            if let Some(space) = fold_platform::color::input(&asset.extensions)? {
                data.extend_from_slice(&(space.len() as u64).to_le_bytes());
                data.extend_from_slice(space.as_bytes());
            }
            for bytes in [asset.location.as_bytes(), asset.fingerprint.as_bytes()] {
                data.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                data.extend_from_slice(bytes);
            }
        }
        let mut dependencies: Vec<_> = document.dependencies.iter().map(|r| r.document).collect();
        dependencies.sort();
        dependencies.dedup();
        pending.extend(dependencies);
    }
    Ok(fold_media::content_hash(&data))
}

pub fn evaluate(
    snapshot: &Snapshot,
    key: &PreviewKey,
    decoder: &mut Decoder,
    cancel: &Cancel,
) -> Result<fold_render::Frame, String> {
    cancel.check()?;
    evaluate_scene(snapshot, &preview_request(snapshot, key)?, decoder, cancel)
        .map(fold_render::Frame::over_black)
}

pub(crate) fn preview_request(
    snapshot: &Snapshot,
    key: &PreviewKey,
) -> Result<SceneRequest, String> {
    let (source, time) = if let Some((document, time)) = key.target {
        (
            DocumentRef {
                document,
                output: key.output.clone(),
                extensions: Default::default(),
            },
            time,
        )
    } else {
        let (source, info) = output(snapshot)?;
        let time = info.time(key.frame)?;
        (source, time)
    };
    if key.content != content_for(snapshot, source.document)? || key.view != 1 {
        return Err("preview identity/settings mismatch".into());
    }
    Ok(SceneRequest {
        region: key.region,
        preview: Some(key.channels.clone()),
        source,
        time,
        dimensions: key.dimensions,
    })
}

/// Logical scene demand. Viewer generation/cache identity is consumer routing,
/// not render authority. Delivery supplies a committed snapshot separately.
#[derive(Clone, Debug)]
pub struct SceneRequest {
    /// Optional spatial demand, independent of viewer routing and delivery.
    pub region: Option<fold_render::region::Region>,
    pub preview: Option<fold_render::view::View>,
    pub source: DocumentRef,
    pub time: fold_foundation::Time,
    pub dimensions: [u32; 2],
}

/// Return scene-linear pixels with alpha intact. Presentation/output transforms
/// and an optional delivery matte are separate operations owned by the caller.
pub fn evaluate_scene(
    snapshot: &Snapshot,
    request: &SceneRequest,
    decoder: &mut Decoder,
    cancel: &Cancel,
) -> Result<fold_render::Frame, String> {
    let (plan, duration) = compile_scene(snapshot, request, cancel)?;
    crate::color::with_config(snapshot, |config| {
        let (plan, crop) = if let Some(region) = request.region {
            let plan = fold_render::region::Plan::new(plan, region, config.is_some())?;
            (plan.graph, Some(plan.output))
        } else {
            (plan, None)
        };
        let frame = match config {
            Some(config) => fold_render::render_aces_with(plan, config, decoder, cancel),
            None => fold_render::render_with(plan, decoder, cancel),
        }?;
        match crop {
            Some(region) => fold_render::region::crop(frame, region),
            None => Ok(frame),
        }
    })?
    .with_timing(request.time, duration)
}

#[cfg(feature = "gpu")]
pub fn evaluate_scene_gpu(
    snapshot: &Snapshot,
    request: &SceneRequest,
    renderer: &mut fold_render::gpu::Renderer,
    decoder: &mut Decoder,
    cancel: &Cancel,
) -> Result<fold_render::gpu::GpuFrame, String> {
    evaluate_scene_gpu_admitted(snapshot, request, renderer, decoder, cancel, None)
}

#[cfg(feature = "gpu")]
pub(crate) fn evaluate_scene_gpu_admitted(
    snapshot: &Snapshot,
    request: &SceneRequest,
    renderer: &mut fold_render::gpu::Renderer,
    decoder: &mut Decoder,
    cancel: &Cancel,
    class: Option<fold_render::scheduling::Class>,
) -> Result<fold_render::gpu::GpuFrame, String> {
    let (plan, duration) = compile_scene(snapshot, request, cancel)?;
    crate::color::with_config(snapshot, |config| {
        if let Some(region) = request.region {
            renderer.evaluate_region(plan, region, config, decoder, cancel, class)
        } else {
            match class {
                Some(class) => renderer.evaluate_scheduled(plan, config, decoder, cancel, class),
                None => renderer.evaluate(plan, config, decoder, cancel),
            }
        }
    })?
    .with_timing(request.time, duration)
}

fn compile_scene(
    snapshot: &Snapshot,
    request: &SceneRequest,
    cancel: &Cancel,
) -> Result<(fold_render::RenderGraph, fold_foundation::Time), String> {
    cancel.check()?;
    let registry = crate::packages::builtins();
    let info = registry.output_ref(snapshot, &request.source)?;
    if request.time < fold_foundation::Time::ZERO || request.time >= info.time(info.frames)? {
        return Err("frame outside sequence".into());
    }
    let plan = if let Some(view) = &request.preview {
        registry.video_channels(
            snapshot,
            &request.source,
            request.time,
            request.dimensions,
            cancel,
            &view.selection()?,
        )?
    } else {
        registry.video_with(
            snapshot,
            &request.source,
            request.time,
            request.dimensions,
            cancel,
        )?
    };
    Ok((plan, info.time(1)?))
}

const OUTPUT_SETTING: &str = "fold.output";

/// Persist an explicit output choice as one undoable project setting change.
pub fn select_output(
    snapshot: &Snapshot,
    document: fold_foundation::DocumentId,
) -> Result<EditBatch, String> {
    let reference = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    output_info(snapshot, &reference)?;
    let mut settings = snapshot.state().settings.clone();
    settings.insert(
        OUTPUT_SETTING.into(),
        serde_json::to_value(reference).map_err(|e| e.to_string())?,
    );
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::SetSettings(settings)],
    })
}

fn output_info(snapshot: &Snapshot, reference: &DocumentRef) -> Result<VideoInfo, String> {
    if reference.output != "video" {
        return Err("unsupported project output port".into());
    }
    crate::packages::builtins().output(snapshot, reference.document)
}

pub fn output(snapshot: &Snapshot) -> Result<(DocumentRef, VideoInfo), String> {
    if let Some(value) = snapshot.state().settings.get(OUTPUT_SETTING) {
        let reference: DocumentRef = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid project output: {e}"))?;
        let info = output_info(snapshot, &reference)?;
        return Ok((reference, info));
    }
    // Compatibility for projects written before explicit output selection.
    // An explicit but unavailable selection above never falls back silently.
    if crate::timeline_workflow::has_sequence(snapshot) {
        let (reference, sequence) = crate::timeline_workflow::active(snapshot)?;
        let _ = sequence;
        let info = crate::packages::builtins().output(snapshot, reference.document)?;
        Ok((reference, info))
    } else if let Ok((reference, layers)) = active(snapshot) {
        Ok((reference, layers.info))
    } else {
        let document = snapshot
            .state()
            .documents
            .values()
            .find(|d| {
                matches!(
                    d.type_id.as_str(),
                    fold_compositor::COMPOSITE | fold_motion::MOTION
                )
            })
            .ok_or("no supported video document")?;
        let info = crate::packages::builtins().output(snapshot, document.id)?;
        Ok((
            DocumentRef {
                document: document.id,
                output: "video".into(),
                extensions: Default::default(),
            },
            info,
        ))
    }
}

/// Export only a coordinator-published revision, never a live gesture overlay.
/// ```compile_fail
/// use fold_app::media_workflow::export;
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
    let video = temporary.path().join("video.mp4");
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
    fold_media::audio::mux_audio(&video, &pcm, path, cancel)
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
    encoder.finish()
}
