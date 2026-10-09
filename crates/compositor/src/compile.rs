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
    fn channels(
        &self,
        document: &fold_project::Document,
        resolve: &fold_platform::ChannelResolver<'_>,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        let composite = Composite::from_document(document)?;
        composite.channel_names_with(composite.output, resolve)
    }
    fn compile_image(&self, request: VideoCompile<'_>) -> Result<ChannelGraph, String> {
        compile(request)
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        compile(request)?.select(None)
    }
}
fn compile(request: VideoCompile<'_>) -> Result<ChannelGraph, String> {
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
    let mut composite = Composite::from_document(document)?;
    if let Some(invocation) = request.invocation {
        composite.bind_values(invocation)?;
    }
    if output != "video"
        || (request.context.is_none()
            && (time < Time::ZERO || time >= composite.info.time(composite.info.frames)?))
    {
        return Err("unsupported composite output or time outside duration".into());
    }
    let mut graph = request
        .invocation
        .map(|i| i.graph.clone())
        .unwrap_or(RenderGraph {
            width,
            height,
            nodes: vec![],
            output: 0,
        });
    let mut ids: BTreeMap<_, Channels> = BTreeMap::new();
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
    for id in composite.render_order()? {
        cancel.check()?;
        let node = composite.node(id)?;
        let connections = node.input_ids();
        let parameters = node.evaluated_parameters(time)?;
        match &parameters {
            P::Input { id } => {
                let invocation = request
                    .invocation
                    .ok_or("Input requires an effect invocation")?;
                let image = *invocation
                    .images
                    .get(id)
                    .ok_or("Unbound effect image input")?;
                ids.insert(node.id, Channels::rgba(image));
                continue;
            }
            P::Instance { instance, images } => {
                if !instance.enabled {
                    let source = connections
                        .first()
                        .and_then(|id| *id)
                        .ok_or("Bypass requires a source input")?;
                    ids.insert(node.id, ids[&source].clone());
                    continue;
                }
                let mut bindings = BTreeMap::new();
                for (port, input) in images.iter().zip(&connections) {
                    let image =
                        ids[&input.ok_or("Unconnected effect image")?].pack(&mut graph, RGBA)?;
                    bindings.insert(port.clone(), image);
                }
                let timing =
                    request
                        .context
                        .map(|i| i.timing)
                        .unwrap_or(fold_animation::timing::Range {
                            start: Time::ZERO,
                            duration: composite.info.time(composite.info.frames)?,
                        });
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
                continue;
            }
            _ => {}
        }

        let effect = node.evaluated_effect(time)?;
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
        } = &parameters
        {
            let asset = snapshot
                .state()
                .assets
                .get(asset)
                .ok_or("Missing EXR asset")?;
            let space = if fold_platform::color::project(snapshot)?.is_some() {
                Some(
                    fold_platform::color::input(&node.extensions)?
                        .or(fold_platform::color::input(&asset.extensions)?)
                        .unwrap_or_else(|| "ACEScg".into()),
                )
            } else {
                None
            };
            let local = time
                .checked_sub(*start)
                .and_then(|t| t.checked_add(*source_start))
                .map_err(|e| e.to_string())?;
            let outside =
                time < *start || time >= start.checked_add(*duration).map_err(|e| e.to_string())?;
            let mut frame = local
                .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
                .map_err(|e| e.to_string())?;
            if outside && options.outside == fold_media::exr_sequence::Policy::Hold {
                let first = source_start
                    .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
                    .map_err(|e| e.to_string())?;
                let end = source_start
                    .checked_add(*duration)
                    .map_err(|e| e.to_string())?
                    .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Ceil)
                    .map_err(|e| e.to_string())?;
                frame = frame.clamp(first, end - 1);
            }
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
            let selected = fold_media::exr_sequence::resolve(&original, frame, options, outside)?;
            let active = selected.is_some();
            let channels = crate::source_channels::exr(
                &mut graph,
                selected.unwrap_or(original),
                space,
                color_layers,
                active,
            )?;
            ids.insert(id, channels);
            continue;
        }
        if let P::Shuffle { mappings } = &parameters {
            let mut channels = ids[&connections[0].unwrap()].clone();
            // All mappings read the original inputs, so swaps are simultaneous.
            for mapping in mappings {
                let channel = match &mapping.source {
                    crate::ChannelSource::A(name) => {
                        ids[&connections[0].unwrap()].get(name.as_str())?
                    }
                    crate::ChannelSource::B(name) => ids
                        [&connections[1].ok_or("Shuffle: connect B for this mapping")?]
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
            continue;
        }
        if let P::RemoveChannels { channels, keep } = &parameters {
            let mut result = ids[&connections[0].unwrap()].clone();
            result.retain(channels, *keep);
            ids.insert(id, result);
            continue;
        }
        if matches!(parameters, P::Output) {
            ids.insert(id, ids[&connections[0].unwrap()].clone());
            continue;
        }
        if node.background_only_merge() {
            let background = connections[1].ok_or("Merge: connect 'background' input")?;
            ids.insert(id, ids[&background].clone());
            continue;
        }
        if parameters.operator().supports_effect() {
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
            let result = crate::compile_image::effect(
                &mut graph,
                &parameters,
                &effect,
                &inputs,
                mask,
                [sx, sy],
            )?;
            ids.insert(id, result);
            continue;
        }
        let mut packed = Vec::new();
        for input in connections.iter().flatten() {
            packed.push(ids[input].pack(&mut graph, RGBA)?);
        }
        let input = |i: usize| packed[i];
        let op =
            match &parameters {
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
                } => {
                    let end = start.checked_add(*duration).map_err(|e| e.to_string())?;
                    if time < *start || time >= end {
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
                            continue;
                        }
                        Op::Solid { rgba: [0.0; 4] }
                    } else {
                        let local = time
                            .checked_sub(*start)
                            .and_then(|t| t.checked_add(*source_start))
                            .map_err(|e| e.to_string())?;
                        match source {
                            Source::Unassigned { .. } => {
                                return Err("Read: choose a source file".into());
                            }
                            Source::Exr { .. } => unreachable!("handled channel source"),
                            Source::Document { source, .. } => {
                                let channels =
                                    resolve_image(source, local)?.append_to(&mut graph)?;
                                ids.insert(node.id, channels);
                                continue;
                            }
                            Source::Asset { asset, info } => {
                                let fragment = fold_platform::image_source::compile(
                                    &fold_platform::image_source::Request {
                                        bake: None,
                                        source: fold_platform::image_source::Source::Asset(*asset),
                                        info: info.clone(),
                                        time: local,
                                        outside: fold_platform::image_source::Outside::Black,
                                        color_space: fold_platform::color::input(&node.extensions)?,
                                    },
                                    &request,
                                )?;
                                let image = graph.append(fragment)?;
                                ids.insert(node.id, Channels::rgba(image));
                                continue;
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
                P::ApplyMask => Op::Mask {
                    input: input(0),
                    mask: input(1),
                },
                P::Unary { .. }
                | P::GaussianBlur { .. }
                | P::TransformImage { .. }
                | P::Crop { .. }
                | P::ColorGrade { .. }
                | P::Composite { .. }
                | P::Output
                | P::Shuffle { .. }
                | P::RemoveChannels { .. } => unreachable!("handled channel routing/effects"),
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
    Ok(ChannelGraph {
        graph,
        channels: ids.remove(&composite.output).unwrap(),
    })
}
