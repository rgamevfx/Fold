use crate::{Composite, OperationConfig as P, Source};
use fold_foundation::Time;
use fold_platform::{VideoCompile, VideoProvider};
use fold_render::channels::{Channel, ChannelGraph, Channels, Interpretation, RGBA, Value};
use fold_render::{ImageOp as Op, RenderGraph};
use std::collections::BTreeMap;

pub struct Provider;
impl VideoProvider for Provider {
    fn effect_interface(
        &self,
        document: &fold_project::Document,
    ) -> Result<fold_platform::effects::Interface, String> {
        Ok(Composite::from_document(document)?.reusable()?.interface)
    }
    fn playback_mode(&self) -> fold_platform::desktop::PlaybackMode {
        fold_platform::desktop::PlaybackMode::EveryFrame
    }
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITE
    }
    fn video_info(
        &self,
        document: &fold_project::Document,
    ) -> Result<fold_media::VideoInfo, String> {
        Ok(Composite::from_document(document)?.info)
    }
    fn working_output(
        &self,
        document: &fold_project::Document,
    ) -> Option<fold_project::DocumentRef> {
        let graph = Composite::from_document(document).ok()?;
        Some(fold_project::DocumentRef {
            document: document.id,
            output: if graph
                .nodes
                .iter()
                .any(|n| n.operation.operation == "Viewer")
            {
                "viewer"
            } else {
                "video"
            }
            .into(),
            extensions: Default::default(),
        })
    }
    fn outputs(
        &self,
        document: &fold_project::Document,
    ) -> Vec<fold_platform::workspace::OutputDescriptor> {
        let mut outputs = vec![fold_platform::workspace::OutputDescriptor {
            reference: fold_project::DocumentRef {
                document: document.id,
                output: "video".into(),
                extensions: Default::default(),
            },
            label: "Video".into(),
            info: self.video_info(document),
            playback_mode: self.playback_mode(),
        }];
        if let Ok(composite) = Composite::from_document(document) {
            if composite
                .nodes
                .iter()
                .any(|n| n.operation.operation == "Viewer")
            {
                outputs.push(fold_platform::workspace::OutputDescriptor {
                    reference: fold_project::DocumentRef {
                        document: document.id,
                        output: "viewer".into(),
                        extensions: Default::default(),
                    },
                    label: "Viewer".into(),
                    info: Ok(composite.info.clone()),
                    playback_mode: self.playback_mode(),
                });
            }
            for node in &composite.nodes {
                if node.operation.operation == "Color Shift" {
                    for (kind, label) in
                        [("qualifier-input", "Input"), ("qualifier-matte", "Matte")]
                    {
                        outputs.push(fold_platform::workspace::OutputDescriptor {
                            reference: fold_project::DocumentRef {
                                document: document.id,
                                output: format!("{kind}/{}", node.id),
                                extensions: Default::default(),
                            },
                            label: format!("{} · {label}", node.name),
                            info: Ok(composite.info.clone()),
                            playback_mode: self.playback_mode(),
                        });
                    }
                }
                if node.operation.operation == "Write" {
                    outputs.push(fold_platform::workspace::OutputDescriptor {
                        reference: fold_project::DocumentRef {
                            document: document.id,
                            output: format!("write/{}", node.id),
                            extensions: Default::default(),
                        },
                        label: node.name.clone(),
                        info: Ok(composite.info.clone()),
                        playback_mode: self.playback_mode(),
                    });
                }
            }
        }
        outputs
    }
    fn channels(
        &self,
        document: &fold_project::Document,
        resolve: &fold_platform::ChannelResolver<'_>,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        let native = Composite::read_document(document)?;
        let composite = native.prepare_document(document)?;
        composite.channel_names_with(composite.output, resolve)
    }
    fn output_channels(
        &self,
        document: &fold_project::Document,
        output: &str,
        resolve: &fold_platform::ChannelResolver<'_>,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        if output != "viewer" {
            return self.channels(document, resolve);
        }
        let native = Composite::read_document(document)?;
        let node = native
            .nodes
            .iter()
            .find(|n| n.operation.operation == "Viewer")
            .ok_or("No working Viewer")?
            .id;
        native
            .prepare_document(document)?
            .channel_names_with(node, resolve)
    }
    fn compile_image(
        &self,
        request: VideoCompile<'_>,
    ) -> Result<ChannelGraph, fold_foundation::Failure> {
        compile(request)
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, fold_foundation::Failure> {
        Ok(compile(request)?.select(None)?)
    }
}
fn compile(request: VideoCompile<'_>) -> Result<ChannelGraph, fold_foundation::Failure> {
    let VideoCompile {
        snapshot,
        document,
        reference,
        time,
        dimensions: [width, height],
        cancel,
        resolve_image,
        describe_channels,
        ..
    } = request;
    let output = reference.output.as_str();
    cancel.check()?;
    let composite = Composite::read_document(document)?;
    let mut prepared = composite.prepare_document(document)?;
    if let Some(invocation) = request.invocation {
        prepared = prepared.with_invocation(invocation)?;
    }
    let qualifier = output
        .strip_prefix("qualifier-input/")
        .or_else(|| output.strip_prefix("qualifier-matte/"))
        .map(|id| {
            serde_json::from_value::<fold_foundation::ObjectId>(serde_json::Value::String(
                id.into(),
            ))
            .map_err(|_| "Invalid qualifier identity")
        })
        .transpose()?;
    let render_output = if let Some(id) = qualifier {
        let node = prepared.node(id)?;
        if !matches!(node.config(), P::ColorShift { .. }) {
            return Err("Not a Color Shift node".into());
        }
        node.input_ids()[0].ok_or("Connect Color Shift input")?
    } else if output == "viewer" {
        composite
            .nodes
            .iter()
            .find(|n| n.operation.operation == "Viewer")
            .ok_or("No working Viewer")?
            .id
    } else if output == "video" {
        composite.output
    } else {
        let id = serde_json::from_value(serde_json::Value::String(
            output
                .strip_prefix("write/")
                .ok_or("Unsupported composite output")?
                .into(),
        ))
        .map_err(|_| "Invalid Write output identity")?;
        if !matches!(prepared.node(id)?.config(), P::Write { .. }) {
            return Err("Output is not a Write node".into());
        }
        id
    };
    if request.context.is_none()
        && (time < Time::ZERO || time >= composite.info.time(composite.info.frames)?)
    {
        return Err("unsupported composite output or time outside duration".into());
    }
    let composite = prepared;
    let mut graph = request
        .invocation
        .map(|i| i.graph.clone())
        .unwrap_or(RenderGraph {
            width,
            height,
            nodes: vec![],
            output: 0,
        });
    let mut cache: BTreeMap<crate::retime::Key, Channels> = BTreeMap::new();
    let plan = crate::retime::plan(&composite, render_output, time, cancel)?;
    let aces = fold_platform::color::project(snapshot)?.is_some();
    let sx = f64::from(width) / f64::from(composite.info.width);
    let sy = f64::from(height) / f64::from(composite.info.height);
    let rect = |r: [u32; 4]| {
        [
            (f64::from(r[0]) * sx).round() as u32,
            (f64::from(r[1]) * sy).round() as u32,
            (f64::from(r[2]) * sx).round() as u32,
            (f64::from(r[3]) * sy).round() as u32,
        ]
    };
    for &(id, time) in &plan.order {
        let node = composite.node(id)?;
        if node.disabled {
            let channels = if let Some(input) = node
                .config()
                .primary_slot()
                .and_then(|slot| node.input_ids().get(slot).copied().flatten())
            {
                cache
                    .get(&(input, time))
                    .ok_or("Missing bypass input")?
                    .clone()
            } else {
                let image = graph.nodes.len();
                graph
                    .nodes
                    .push(fold_render::ImageOp::Solid { rgba: [0.; 4] });
                Channels::rgba(image)
            };
            cache.insert((id, time), channels);
            continue;
        }
        if let Some(samples) = plan.samples.get(&(id, time)) {
            let input = node.input_ids()[0].ok_or("Retime: connect image input")?;
            let names = composite.channel_names_with(input, describe_channels)?;
            let P::Retime { settings } = composite.evaluated_parameters(id, time)? else {
                unreachable!()
            };
            let channels =
                crate::retime::synthesize(&mut graph, samples, input, &cache, &names, &settings)?;
            cache.insert((id, time), channels);
            continue;
        }
        let mut ids: BTreeMap<_, Channels> = node
            .input_ids()
            .iter()
            .flatten()
            .chain(node.effect.mask_node().iter())
            .filter_map(|input| cache.get(&(*input, time)).map(|c| (*input, c.clone())))
            .collect();
        'node: {
            cancel.check()?;
            let node = composite.node(id)?;
            let connections = node.input_ids();
            let mut parameters = composite.evaluated_parameters(node.id, time)?;
            if let P::Read {
                source, options, ..
            } = &mut parameters
            {
                if options.use_proxy
                    && let Some(proxy) = &options.proxy
                {
                    *source = proxy.clone();
                }
            }
            match &parameters {
                P::Input { id } => {
                    if time != request.time {
                        return Err("Retime cannot request another frame of a reusable effect's bound image. Place Retime before the effect, or connect a Read node inside it.".into());
                    }
                    let invocation = request
                        .invocation
                        .ok_or("Input requires an effect invocation")?;
                    let image = *invocation
                        .images
                        .get(id)
                        .ok_or("Unbound effect image input")?;
                    ids.insert(node.id, Channels::rgba(image));
                    break 'node;
                }
                P::Instance { instance, images } => {
                    if !instance.enabled {
                        let source = connections
                            .first()
                            .and_then(|id| *id)
                            .ok_or("Bypass requires a source input")?;
                        ids.insert(node.id, ids[&source].clone());
                        break 'node;
                    }
                    let mut bindings = BTreeMap::new();
                    for (port, input) in images.iter().zip(&connections) {
                        let image = ids[&input.ok_or("Unconnected effect image")?]
                            .pack(&mut graph, RGBA)?;
                        bindings.insert(port.clone(), image);
                    }
                    let timing = request.context.map(|i| i.timing).unwrap_or(
                        fold_animation::timing::Range {
                            start: Time::ZERO,
                            duration: composite.info.time(composite.info.frames)?,
                        },
                    );
                    let format = request.context.map(|i| i.format).unwrap_or(
                        fold_platform::format::Format::square([
                            composite.info.width,
                            composite.info.height,
                        ]),
                    );
                    graph = (request.resolve_effect)(
                        &instance.definition,
                        time,
                        &fold_platform::effects::Invocation {
                            graph: &graph,
                            images: bindings,
                            values: instance.values_at(time)?,
                            version: instance.version,
                            timing,
                            format,
                        },
                    )?;
                    ids.insert(node.id, Channels::rgba(graph.output));
                    break 'node;
                }
                _ => {}
            }

            let effect = composite.evaluated_effect(node.id, time)?;
            if let P::Read {
                source:
                    Source::Exr {
                        asset,
                        image,
                        color_layers,
                        options,
                        sequence,
                        info,
                    },
                start,
                source_start,
                duration,
                options: read,
            } = &parameters
            {
                let asset = snapshot
                    .state()
                    .assets
                    .get(asset)
                    .ok_or("Missing EXR asset")?;
                let space = if !read.raw && fold_platform::color::project(snapshot)?.is_some() {
                    Some(
                        fold_platform::color::input(&node.extensions)?
                            .or(fold_platform::color::input(&asset.extensions)?)
                            .unwrap_or_else(|| "ACEScg".into()),
                    )
                } else {
                    None
                };
                let local = read.mapped_time(time, *start, *source_start, *duration, info.rate)?;
                let frame = local
                    .unwrap_or(*source_start)
                    .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
                    .map_err(|e| e.to_string())?;
                let mut current_image = image.clone();
                if let Some(value) = asset.extensions.get(fold_media::ingest::METADATA_KEY) {
                    let metadata: fold_media::ingest::SourceMetadata =
                        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                    if let fold_media::ingest::SourceProfile::Exr(new) = metadata.profile {
                        new.validate()?;
                        if !image.compatible_pixels(&new)
                            || match (sequence, &new.sequence) {
                                (None, None) => false,
                                (Some(expected), Some(actual)) => !expected.matches(actual),
                                _ => true,
                            }
                        {
                            return Err("EXR source channel/frame contract changed".into());
                        }
                        current_image = new;
                    } else {
                        return Err("Read requires EXR asset metadata".into());
                    }
                } else if sequence.is_some() {
                    return Err("Missing EXR sequence manifest".into());
                }
                let original = fold_media::exr::Source {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: current_image,
                };
                let selected = if local.is_some() {
                    fold_media::exr_sequence::resolve(
                        &original,
                        frame,
                        &fold_media::exr_sequence::Options {
                            still: options.still,
                            missing: options.missing,
                            outside: if sequence
                                .as_ref()
                                .is_some_and(|s| frame < i64::from(s.first))
                            {
                                read.before
                            } else {
                                read.after
                            },
                        },
                        false,
                    )?
                } else {
                    None
                };
                let active = selected.is_some();
                let channels = crate::source_channels::exr(
                    &mut graph,
                    selected.unwrap_or(original),
                    space,
                    if read.raw { &[] } else { color_layers },
                    active,
                    read.alpha,
                )?;
                ids.insert(id, channels);
                break 'node;
            }
            if let P::Shuffle { mappings } = &parameters {
                let mut channels = ids[&connections[0].unwrap()].clone();
                // All mappings read the original inputs, so swaps are simultaneous.
                for mapping in mappings {
                    let channel = match &mapping.source {
                        crate::ChannelSource::B(name) => {
                            ids[&connections[0].unwrap()].get(name.as_str())?
                        }
                        crate::ChannelSource::A(name) => ids
                            [&connections[1].ok_or("Shuffle: connect A for this mapping")?]
                            .get(name.as_str())?,
                        crate::ChannelSource::Zero => Channel {
                            value: Value::Zero,
                            interpretation: Interpretation::Data,
                        },
                        crate::ChannelSource::One => Channel {
                            value: Value::One,
                            interpretation: Interpretation::Data,
                        },
                    };
                    channels.insert(mapping.destination.clone(), channel);
                }
                ids.insert(id, channels);
                break 'node;
            }
            if let P::CullChannels { channels, keep } = &parameters {
                let mut result = ids[&connections[0].unwrap()].clone();
                result.retain(
                    &channels.iter().flatten().cloned().collect::<Vec<_>>(),
                    *keep,
                );
                ids.insert(id, result);
                break 'node;
            }
            if matches!(parameters, P::Output | P::Viewer | P::Write { .. }) {
                ids.insert(id, ids[&connections[0].unwrap()].clone());
                break 'node;
            }
            if node.background_only_merge() {
                let background = connections[1].ok_or("Merge: connect 'background' input")?;
                ids.insert(id, ids[&background].clone());
                break 'node;
            }
            if parameters.operator().supports_effect()
                || matches!(parameters, P::TransformImage { .. })
            {
                let inputs: Vec<_> = node
                    .input_ids()
                    .iter()
                    .flatten()
                    .map(|id| &ids[id])
                    .collect();
                let mask = effect
                    .mask_node()
                    .filter(|_| !effect.mask_disabled)
                    .map(|id| &ids[&id]);
                let samples = if let P::TransformImage { settings } = &parameters {
                    settings
                        .sample_times(time, composite.info.rate)?
                        .into_iter()
                        .map(|time| composite.evaluated_parameters(node.id, time))
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    vec![parameters.clone()]
                };
                let mut effect = effect.clone();
                if matches!(parameters, P::TransformImage { .. }) {
                    effect.channels = inputs[0].iter().map(|(name, _)| name.clone()).collect();
                }
                let result = crate::compile_image::effect(
                    &mut graph,
                    &samples,
                    &effect,
                    &inputs,
                    mask,
                    [sx, sy],
                    time,
                    aces,
                )?;
                ids.insert(id, result);
                break 'node;
            }
            if let P::Roto { settings } = &parameters {
                let samples = settings
                    .sample_times(time, composite.info.rate)?
                    .into_iter()
                    .map(|t| {
                        if t == time {
                            return Ok((t, settings.clone()));
                        }
                        let P::Roto { settings } = composite.evaluated_parameters(node.id, t)?
                        else {
                            unreachable!()
                        };
                        Ok((t, settings))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let input = connections.first().and_then(|id| *id).map(|id| &ids[&id]);
                let result = crate::roto::compile(&mut graph, &samples, input, [sx, sy], cancel)?;
                ids.insert(node.id, result);
                break 'node;
            }
            let mut packed = Vec::new();
            for input in connections.iter().flatten() {
                packed.push(ids[input].pack(&mut graph, RGBA)?);
            }
            let input = |i: usize| packed[i];
            let op = match &parameters {
                P::Shader { settings } => {
                    let format = request.context.map(|i| i.format).unwrap_or(
                        fold_platform::format::Format::square([
                            composite.info.width,
                            composite.info.height,
                        ]),
                    );
                    let timing = request.context.map(|i| i.timing).unwrap_or(
                        fold_animation::timing::Range {
                            start: Time::ZERO,
                            duration: composite.info.time(composite.info.frames)?,
                        },
                    );
                    let progress = timing
                        .sample(time, fold_animation::timing::Behavior::Clamp)?
                        .progress;
                    Op::Shader {
                        inputs: packed.clone(),
                        program: std::sync::Arc::new(settings.program(
                            format,
                            fold_animation::seconds(time),
                            progress,
                        )?),
                    }
                }

                P::RelativeTransform { settings } => Op::Resample {
                    edges: Default::default(),
                    clamp: false,
                    input: input(0),
                    filter: settings.filter,
                    transform: settings.matrix(
                        request.context.map(|i| i.format).unwrap_or(
                            fold_platform::format::Format::square([
                                composite.info.width,
                                composite.info.height,
                            ]),
                        ),
                        [width, height],
                    )?,
                },
                P::Input { .. } | P::Instance { .. } => unreachable!("handled invocation"),
                P::Dissolve { curve } => {
                    let range = request.context.map(|i| i.timing).unwrap_or(
                        fold_animation::timing::Range {
                            start: Time::ZERO,
                            duration: composite.info.time(composite.info.frames)?,
                        },
                    );
                    let progress = range
                        .sample(time, fold_animation::timing::Behavior::Clamp)?
                        .progress;
                    Op::Mix {
                        original: input(0),
                        processed: input(1),
                        mask: None,
                        mask_channel: 3,
                        invert: false,
                        amount: curve.sample(progress) as f32,
                        channels: [true; 4],
                    }
                }

                P::Shape { shape, bounds } => {
                    Op::Vector(std::sync::Arc::new(vec![crate::shapes::drawing(
                        *shape,
                        *bounds,
                        [sx, sy],
                    )?]))
                }
                P::Read {
                    source,
                    start,
                    source_start,
                    duration,
                    options,
                } => {
                    let local = options.mapped_time(
                        time,
                        *start,
                        *source_start,
                        *duration,
                        source.info().rate,
                    )?;
                    if local.is_none() {
                        if let Source::Document { source, .. } = source {
                            let mut channels = Channels::default();
                            for name in describe_channels(source)? {
                                // Inactive outputs are literal zero data, with no
                                // source processing or media access.
                                channels.insert(
                                    name,
                                    Channel {
                                        value: Value::Zero,
                                        interpretation: Interpretation::Data,
                                    },
                                );
                            }
                            ids.insert(node.id, channels);
                            break 'node;
                        }
                        Op::Solid { rgba: [0.0; 4] }
                    } else {
                        let local = local.unwrap();
                        match source {
                            Source::Unassigned { .. } => {
                                return Err("Read: choose a source file".into());
                            }
                            Source::Exr { .. } => unreachable!("handled channel source"),
                            Source::Document { source, .. } => {
                                let channels =
                                    resolve_image(source, local)?.append_to(&mut graph)?;
                                ids.insert(node.id, channels);
                                break 'node;
                            }
                            Source::Asset { asset, info } => {
                                let fragment = fold_platform::image_source::compile(
                                    &fold_platform::image_source::Request {
                                        bake: None,
                                        source: fold_platform::image_source::Source::Asset(*asset),
                                        info: info.clone(),
                                        time: local,
                                        outside: fold_platform::image_source::Outside::Black,
                                        color_space: if options.raw {
                                            Some("ACEScg".into())
                                        } else {
                                            fold_platform::color::input(&node.extensions)?
                                        },
                                    },
                                    &request,
                                )?;
                                let image = graph.append(fragment)?;
                                ids.insert(node.id, Channels::rgba(image));
                                break 'node;
                            }
                        }
                    }
                }
                P::Solid { rgba } => Op::Solid { rgba: *rgba },
                P::Mask { rect: r } => {
                    graph.nodes.push(Op::Solid { rgba: [1.0; 4] });
                    Op::Crop {
                        input: graph.nodes.len() - 1,
                        rect: rect(*r),
                    }
                }
                P::LensDistortion { .. }
                | P::FilmEmulation { .. }
                | P::ColorShift { .. }
                | P::ChromaticAberration { .. }
                | P::Grain { .. }
                | P::Retime { .. }
                | P::Roto { .. }
                | P::Unary { .. }
                | P::Blur { .. }
                | P::TransformImage { .. }
                | P::Crop { .. }
                | P::ColorGrade { .. }
                | P::Composite { .. }
                | P::Output
                | P::Viewer
                | P::Write { .. }
                | P::Shuffle { .. }
                | P::CullChannels { .. } => unreachable!("handled channel routing/effects"),
            };
            let result = graph.nodes.len();
            graph.nodes.push(op);
            let mut channels = connections
                .first()
                .and_then(|id| *id)
                .map(|id| ids[&id].clone())
                .unwrap_or_default();
            for (name, channel) in Channels::rgba(result).iter() {
                channels.insert(name.clone(), *channel);
            }
            ids.insert(node.id, channels);
            if graph.nodes.len() > 4096 {
                return Err("composite compilation exceeds 4096 IR nodes".into());
            }
        }
        cache.insert((id, time), ids.remove(&id).ok_or("Missing evaluated node")?);
        if graph.nodes.len() > 4096 {
            return Err("Temporal render exceeds 4096 operations; reduce retime samples".into());
        }
    }
    if output.starts_with("qualifier-matte/") {
        let P::ColorShift { settings } =
            composite.evaluated_parameters(qualifier.unwrap(), time)?
        else {
            unreachable!()
        };
        let input = cache[&(render_output, time)].pack(&mut graph, RGBA)?;
        let matte = crate::filters::color_shift(
            &mut graph,
            &settings,
            input,
            [composite.info.width as f32, composite.info.height as f32],
            aces,
            true,
        )?;
        return Ok(ChannelGraph {
            graph,
            channels: Channels::rgba(matte),
        });
    }
    Ok(ChannelGraph {
        graph,
        channels: cache
            .remove(&(render_output, time))
            .ok_or("Missing output")?,
    })
}
