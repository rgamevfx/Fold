use crate::{Composite, Parameters as P, Source};
use fold_foundation::Time;
use fold_platform::{VideoCompile, VideoProvider};
use fold_render::channels::{Channel, ChannelGraph, Channels, Interpretation, RGBA, Value};
use fold_render::{ImageOp as Op, RenderGraph};
use std::collections::BTreeMap;

pub struct Provider;
impl VideoProvider for Provider {
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
    let composite = Composite::from_document(document)?;
    if output != "video"
        || time < Time::ZERO
        || time >= composite.info.time(composite.info.frames)?
    {
        return Err("unsupported composite output or time outside duration".into());
    }
    let mut graph = RenderGraph {
        width,
        height,
        nodes: vec![],
        output: 0,
    };
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
        let parameters = node.evaluated_parameters(time)?;
        let effect = node.evaluated_effect(time)?;
        if let P::Read {
            source:
                Source::Exr {
                    asset,
                    image,
                    color_layers,
                    ..
                },
            start,
            duration,
            ..
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
            let channels = crate::source_channels::exr(
                &mut graph,
                fold_media::exr::Source {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: image.clone(),
                },
                space,
                color_layers,
                time >= *start && time < start.checked_add(*duration).map_err(|e| e.to_string())?,
            )?;
            ids.insert(id, channels);
            continue;
        }
        if let P::Shuffle { mappings } = &parameters {
            let mut channels = ids[&node.inputs[0].unwrap()].clone();
            // All mappings read the original inputs, so swaps are simultaneous.
            for mapping in mappings {
                let channel = match &mapping.source {
                    crate::ChannelSource::A(name) => {
                        ids[&node.inputs[0].unwrap()].get(name.as_str())?
                    }
                    crate::ChannelSource::B(name) => ids
                        [&node.inputs[1].ok_or("Shuffle: connect B for this mapping")?]
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
            let mut result = ids[&node.inputs[0].unwrap()].clone();
            result.retain(channels, *keep);
            ids.insert(id, result);
            continue;
        }
        if matches!(parameters, P::Output) {
            ids.insert(id, ids[&node.inputs[0].unwrap()].clone());
            continue;
        }
        if parameters.operator().supports_effect() {
            let inputs: Vec<_> = node.inputs.iter().flatten().map(|id| &ids[id]).collect();
            let mask = effect
                .mask
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
        for input in node.inputs.iter().flatten() {
            packed.push(ids[input].pack(&mut graph, RGBA)?);
        }
        let input = |i: usize| packed[i];
        let op = match &parameters {
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
                            let channels = resolve_image(source, local)?.append_to(&mut graph)?;
                            ids.insert(node.id, channels);
                            continue;
                        }
                        Source::Asset { asset, info } => {
                            let asset = snapshot
                                .state()
                                .assets
                                .get(asset)
                                .ok_or("missing Read asset")?;
                            Op::video(
                                fold_media::VideoSource {
                                    path: asset.location.clone().into(),
                                    fingerprint: asset.fingerprint.clone(),
                                    info: info.clone(),
                                },
                                local,
                                match fold_platform::color::input(&node.extensions)? {
                                    Some(space) => Some(space),
                                    None => fold_platform::color::input(&asset.extensions)?,
                                },
                            )
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
            | P::Transform { .. }
            | P::Crop { .. }
            | P::Blur { .. }
            | P::Grade { .. }
            | P::ColorGrade { .. }
            | P::Composite { .. }
            | P::Merge
            | P::Output
            | P::Shuffle { .. }
            | P::RemoveChannels { .. } => unreachable!("handled channel routing/effects"),
        };
        let result = graph.nodes.len();
        graph.nodes.push(op);
        let mut channels = node
            .inputs
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
