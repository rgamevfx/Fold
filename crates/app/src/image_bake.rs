//! Worker orchestration. Creative graph interpretation stays in Motion; texture
//! storage/rendering stays in shared modules. Applying the result is one edit.
use fold_media::Cancel;
use fold_motion::nodes::image::bake::{Args, Settings};
use fold_platform::image_bake::{Manifest, Progress, Record, Reference};
use fold_project::{EditBatch, Mutation, Snapshot};
use std::{collections::BTreeMap, sync::Mutex};
static ACTIVE: Mutex<()> = Mutex::new(());
pub fn run(snapshot: &Snapshot, args: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let args: Args = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let _active = ACTIVE
        .try_lock()
        .map_err(|_| "Another texture bake is running")?;
    let key = args.progress_key();
    fold_platform::image_bake::start(&key, cancel.clone());
    let result = bake(snapshot, &args, cancel);
    let mut progress = fold_platform::image_bake::progress(&key).unwrap_or_default();
    progress.running = false;
    progress.error = result.as_ref().err().cloned();
    fold_platform::image_bake::report(&key, progress);
    fold_platform::image_bake::finish(&key);
    result
}
fn bake(snapshot: &Snapshot, args: &Args, cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let previous = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing Motion document")?;
    let mut motion = fold_motion::Motion::from_document(previous)?;
    let mut view = motion.clone();
    if let Some(group) = args.group {
        view.graph = motion
            .groups
            .get(&group)
            .ok_or("Missing group")?
            .graph
            .clone();
        view.scene = None;
    }
    let node = view
        .graph
        .nodes
        .iter()
        .find(|n| n.id == args.node && n.kind == "fold.motion.image_bake")
        .ok_or("Select an Image Bake node")?;
    let settings = node.settings::<Settings>()?;
    settings.validate()?;
    let end = if settings.end == 0 {
        view.info.frames
    } else {
        settings.end
    };
    if end > view.info.frames || end <= settings.start || end - settings.start > 600 {
        return Err("Bake range must contain 1..600 frames inside the document".into());
    }
    let mut requests = BTreeMap::new();
    for frame in settings.start..end {
        cancel.check()?;
        for request in fold_motion::nodes::image::bake::requests(
            &view,
            args.node,
            view.info.time(frame)?,
            cancel,
        )? {
            if request.bake.is_some() {
                return Err("Use one Image Bake per source chain".into());
            }
            let key = fold_platform::image_bake::key(snapshot, &request)?;
            requests.entry(key).or_insert(request);
            if requests.len() > 10000 {
                return Err("Bake exceeds 10,000 unique source frames; shorten its range".into());
            }
        }
    }
    if requests.is_empty() {
        return Err("Connect an image source before baking".into());
    }
    // Decode each source forward; interleaving more sources than the decoder's
    // retained-session budget would repeatedly reopen them during preparation.
    let mut requests = requests.into_iter().collect::<Vec<_>>();
    requests.sort_by(|a, b| {
        serde_json::to_string(&a.1.source)
            .unwrap()
            .cmp(&serde_json::to_string(&b.1.source).unwrap())
            .then(a.1.time.cmp(&b.1.time))
    });
    let root = fold_media::scratch::cache_root().join("image-bakes-v1");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let used = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|e| std::fs::read_dir(e.path()).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum::<u64>();
    if used > 8 * 1024 * 1024 * 1024 {
        return Err("Texture bake cache exceeds 8 GiB; remove unused image-bakes-v1 directories before rebuilding".into());
    }
    let directory = tempfile::Builder::new()
        .prefix("bake-")
        .tempdir_in(&root)
        .map_err(|e| e.to_string())?;
    let mut manifest = Manifest {
        version: 1,
        frames: end - settings.start,
        bytes: 0,
        entries: BTreeMap::new(),
    };
    let mut decoder = fold_media::Decoder::from_environment()?;
    #[cfg(feature = "gpu")]
    let mut renderer = {
        let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(
            fold_render::gpu::Host::DEFAULT_BUDGET,
        ))?;
        fold_render::gpu::Renderer::new(host)?
    };
    let packages = crate::packages::builtins();
    #[cfg(feature = "gpu")]
    let mut encoder = fold_render::gpu::BakeEncoder::new(renderer.host().clone())?;
    let progress_key = args.progress_key();
    let total = requests.len() as u32;
    for (i, (key, request)) in requests.into_iter().enumerate() {
        cancel.check()?;
        fold_platform::image_bake::report(
            &progress_key,
            Progress {
                completed: i as u32,
                total,
                running: true,
                error: None,
            },
        );
        let scale = (settings.resolution as f64
            / f64::from(request.info.width.max(request.info.height)))
        .min(1.);
        let dimensions = [request.info.width, request.info.height]
            .map(|v| ((v as f64 * scale).round() as u32).div_ceil(4).max(1) * 4);
        let reference = fold_project::DocumentRef {
            document: args.document,
            output: "video".into(),
            extensions: Default::default(),
        };
        let resolve = |r: &fold_project::DocumentRef, t| {
            packages.video_with(snapshot, r, t, dimensions, cancel)
        };
        let image = |r: &fold_project::DocumentRef, t| {
            packages
                .video_channels(snapshot, r, t, dimensions, cancel, &Default::default())
                .map(fold_render::channels::ChannelGraph::rgba)
        };
        let describe = |r: &fold_project::DocumentRef| packages.channels(snapshot, r);
        let context = fold_platform::VideoCompile {
            graph_context: None,
            resolve_graph: &|_, _, _| {
                Err("Layer graph evaluation is unavailable in this context".into())
            },
            snapshot,
            document: previous,
            reference: &reference,
            time: request.time,
            dimensions,
            cancel,
            resolve: &resolve,
            resolve_image: &image,
            describe_channels: &describe,
            describe_video: &|r| packages.output(snapshot, r.document),
        };
        let mut graph = fold_platform::image_source::compile(&request, &context)?;
        if settings.format == fold_render::baked::Format::Bc7 && settings.clamp_sdr {
            graph.nodes.push(fold_render::ImageOp::Unary {
                input: graph.output,
                operation: fold_render::operations::Unary::Clamp {
                    minimum: 0.,
                    maximum: 1.,
                },
            });
            graph.output = graph.nodes.len() - 1;
        }
        let frame = crate::color::with_config(snapshot, |config| {
            #[cfg(feature = "gpu")]
            {
                let mut frame = renderer.evaluate(graph, config, &mut decoder, cancel)?;
                frame.readback(cancel)
            }
            #[cfg(not(feature = "gpu"))]
            {
                match config {
                    Some(config) => {
                        fold_render::render_aces_with(graph, config, &mut decoder, cancel)
                    }
                    None => fold_render::render_with(graph, &mut decoder, cancel),
                }
            }
        })?;
        #[cfg(feature = "gpu")]
        let texture = encoder.encode(&frame, settings.format, cancel)?;
        #[cfg(not(feature = "gpu"))]
        let texture = fold_render::baked::Texture::encode(&frame, settings.format, cancel)?;
        let path = directory
            .path()
            .join(format!("{}.foldtex", texture.identity));
        if !path.exists() {
            manifest.bytes += texture.bytes();
            if manifest.bytes > 2 * 1024 * 1024 * 1024
                || used + manifest.bytes > 8 * 1024 * 1024 * 1024
            {
                return Err("Texture bake exceeds its 2 GiB / aggregate 8 GiB storage budget; lower resolution or shorten range".into());
            }
            texture.save(&path, cancel)?;
        }
        manifest.entries.insert(
            key,
            Record {
                texture: texture.identity,
            },
        );
    }
    let bytes = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(directory.path().join("manifest.json"), &bytes).map_err(|e| e.to_string())?;
    cancel.check()?;
    let reference = Reference {
        directory: directory.path().to_owned(),
        manifest: fold_media::content_hash(&bytes),
        options: settings.options(),
    };
    let graph = match args.group {
        Some(id) => &mut motion.groups.get_mut(&id).unwrap().graph,
        None => &mut motion.graph,
    };
    let node = graph.nodes.iter_mut().find(|n| n.id == args.node).unwrap();
    node.settings["bake"] = serde_json::to_value(reference).map_err(|e| e.to_string())?;
    let mut document = motion.document(args.document)?;
    document.extensions = previous.extensions.clone();
    let _path = directory.keep();
    fold_platform::image_bake::report(
        &progress_key,
        Progress {
            completed: total,
            total,
            running: false,
            error: None,
        },
    );
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
