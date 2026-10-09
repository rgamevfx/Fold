//! Shared scene preparation and evaluation for preview and delivery.
use super::{content_for, output};
use fold_media::{Cancel, Decoder};
use fold_platform::desktop::PreviewKey;
use fold_project::{DocumentRef, Snapshot};
pub fn evaluate(
    snapshot: &Snapshot,
    key: &PreviewKey,
    decoder: &mut Decoder,
    cancel: &Cancel,
) -> Result<fold_render::Frame, fold_foundation::Failure> {
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
) -> Result<fold_render::Frame, fold_foundation::Failure> {
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
) -> Result<fold_render::gpu::GpuFrame, fold_foundation::Failure> {
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
) -> Result<fold_render::gpu::GpuFrame, fold_foundation::Failure> {
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

pub(crate) fn compile_scene(
    snapshot: &Snapshot,
    request: &SceneRequest,
    cancel: &Cancel,
) -> Result<(fold_render::RenderGraph, fold_foundation::Time), fold_foundation::Failure> {
    cancel.check()?;
    let registry = crate::packages::builtins();
    let document = snapshot
        .state()
        .documents
        .get(&request.source.document)
        .ok_or_else(|| {
            fold_foundation::Failure::new(
                fold_foundation::FailureKind::MissingResource,
                "missing output document",
            )
            .context(request.source.document)
        })?;
    if !registry.supports(document) || !registry.has_video(document) {
        return Err(fold_foundation::Failure::new(
            fold_foundation::FailureKind::UnsupportedCapability,
            "missing video provider or unsupported schema",
        )
        .context(document.id));
    }
    let info = registry
        .output_ref(snapshot, &request.source)
        .map_err(|error| {
            fold_foundation::Failure::new(fold_foundation::FailureKind::InvalidInput, error)
                .context(document.id)
                .context(&request.source.output)
        })?;
    if request.time < fold_foundation::Time::ZERO || request.time >= info.time(info.frames)? {
        return Err(fold_foundation::Failure::new(
            fold_foundation::FailureKind::InvalidInput,
            "frame outside sequence",
        )
        .context(request.source.document));
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
