use fold_foundation::AssetId;
use fold_media::{Cancel, Decoder, Encoder, VideoInfo};
use fold_platform::{VideoRegistry, desktop::PreviewKey};
use fold_project::{Asset, DocumentRef, EditBatch, Mutation, Snapshot};
use fold_timeline::{VideoLayers, VideoLayersProvider};
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
    let (reference, assets) = if crate::timeline_workflow::has_sequence(snapshot) {
        let (reference, sequence) = crate::timeline_workflow::active(snapshot)?;
        (
            reference,
            sequence.clips.iter().map(|c| c.asset).collect::<Vec<_>>(),
        )
    } else {
        let (reference, layers) = active(snapshot)?;
        (reference, layers.assets)
    };
    let mut data = b"fold-video-evaluator-v1;ffmpeg-zscale-bt709-srgb-v1;nearest;".to_vec();
    data.extend_from_slice(&snapshot.state().documents[&reference.document].payload);
    for id in assets {
        let asset = snapshot
            .state()
            .assets
            .get(&id)
            .ok_or("missing video asset")?;
        data.extend_from_slice(&(asset.fingerprint.len() as u64).to_le_bytes());
        data.extend_from_slice(asset.fingerprint.as_bytes());
    }
    Ok(fold_media::content_hash(&data))
}

pub fn evaluate(
    snapshot: &Snapshot,
    key: &PreviewKey,
    decoder: &mut Decoder,
    cancel: &Cancel,
) -> Result<fold_render::Frame, String> {
    if key.content != content(snapshot)? || key.view != 1 {
        return Err("preview identity/settings mismatch".into());
    }
    let (source, info) = output(snapshot)?;
    if key.frame >= info.frames {
        return Err("frame outside sequence".into());
    }
    let mut registry = VideoRegistry::default();
    registry.register(VideoLayersProvider)?;
    let packages = crate::packages::builtins();
    let plan = if packages.supports(&snapshot.state().documents[&source.document]) {
        packages.video(
            snapshot,
            &source,
            info.time(key.frame)?,
            key.dimensions[0],
            key.dimensions[1],
        )?
    } else {
        registry.compile(
            snapshot,
            &source,
            info.time(key.frame)?,
            key.dimensions[0],
            key.dimensions[1],
        )?
    };
    fold_render::render_with(plan, decoder, cancel)
}

pub fn output(snapshot: &Snapshot) -> Result<(DocumentRef, VideoInfo), String> {
    if crate::timeline_workflow::has_sequence(snapshot) {
        let (reference, sequence) = crate::timeline_workflow::active(snapshot)?;
        let _ = sequence;
        let info = crate::packages::builtins().output(snapshot, reference.document)?;
        Ok((reference, info))
    } else {
        let (reference, layers) = active(snapshot)?;
        Ok((reference, layers.info))
    }
}

pub fn export(
    snapshot: &Snapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
) -> Result<(), String> {
    use fold_foundation::Rounding;
    use fold_media::audio::{AUDIO_RATE, AudioDecoder};
    use std::io::Write;
    if !crate::timeline_workflow::has_sequence(snapshot) {
        return export_video(snapshot, path, start, end, cancel);
    }
    let (reference, sequence) = crate::timeline_workflow::active(snapshot)?;
    let info = sequence.info()?;
    if path.exists() || start >= end || end > info.frames {
        return Err("invalid export range or destination exists".into());
    }
    let plan = crate::packages::builtins().audio(snapshot, reference.document)?;
    let mut decoder = AudioDecoder::default();
    plan.preflight(&mut decoder, cancel)?;
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
    export_video(snapshot, &video, start, end, cancel)?;
    fold_media::audio::mux_audio(&video, &pcm, path, cancel)
}

fn export_video(
    snapshot: &Snapshot,
    path: &Path,
    start: u32,
    end: u32,
    cancel: &Cancel,
) -> Result<(), String> {
    let (_, info) = output(snapshot)?;
    if start >= end || end > info.frames {
        return Err("export requires nonempty half-open [start,end) within the sequence".into());
    }
    let mut decoder = Decoder::default();
    let mut key = PreviewKey {
        content: content(snapshot)?,
        frame: start,
        dimensions: [info.width, info.height],
        view: 1,
    };
    // Preflight decode of both dependencies before starting the encoder.
    let mut first = Some(evaluate(snapshot, &key, &mut decoder, cancel)?);
    let mut encoder = Encoder::new(path, &info, end - start, cancel.clone())?;
    for frame in start..end {
        cancel.check()?;
        key.frame = frame;
        let scene = match first.take() {
            Some(scene) => scene,
            None => evaluate(snapshot, &key, &mut decoder, cancel)?,
        };
        let display = scene.to_display().map_err(str::to_owned)?;
        drop(scene); // Do not retain a linear frame across encode or later frames.
        let rgb: Vec<u8> = display
            .rgba()
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect();
        encoder.write_rgb(&rgb)?;
    }
    encoder.finish()
}
