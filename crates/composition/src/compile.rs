use crate::Composition;
use fold_foundation::{ObjectId, Time};
use fold_platform::{
    VideoCompile, VideoProvider,
    image_source::{self, Outside, Request, Source},
};
use fold_render::{Affine, ImageOp, RenderGraph};
pub(crate) struct Provider;
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITION
    }
    fn playback_mode(&self) -> fold_platform::desktop::PlaybackMode {
        fold_platform::desktop::PlaybackMode::EveryFrame
    }
    fn video_info(
        &self,
        document: &fold_project::Document,
    ) -> Result<fold_media::VideoInfo, String> {
        Ok(Composition::from_document(document)?.info)
    }
    fn effect_interface(
        &self,
        document: &fold_project::Document,
    ) -> Result<fold_platform::effects::Interface, String> {
        Ok(Composition::from_document(document)?.reusable()?.interface)
    }
    fn compile(
        &self,
        mut request: VideoCompile<'_>,
    ) -> Result<RenderGraph, fold_foundation::Failure> {
        let mut composition = Composition::from_document(request.document)?;
        if let Some(invocation) = request.invocation {
            composition.bind_values(invocation)?;
            let local = request
                .time
                .checked_sub(invocation.timing.start)
                .map_err(|e| e.to_string())?;
            request.context = Some(invocation.context().at_child_time(request.time, local)?);
            request.time = local;
            // Full-length template layers follow the host duration. Explicit
            // shorter ranges retain seconds; media clocks are never stretched.
            let end = composition.info.time(composition.info.frames)?;
            for layer in &mut composition.layers {
                if layer.end == end {
                    layer.end = invocation.timing.duration;
                }
            }
        }
        let definition = request
            .invocation
            .map(|_| composition.reusable())
            .transpose()?;
        if request.reference.output != "video"
            || (request.context.is_none()
                && (request.time < Time::ZERO
                    || request.time >= composition.info.time(composition.info.frames)?))
        {
            return Err("Unsupported composition output or time outside duration".into());
        }
        let [width, height] = request.dimensions;
        let mut graph = request
            .invocation
            .map(|i| i.graph.clone())
            .unwrap_or(RenderGraph {
                width,
                height,
                nodes: vec![],
                output: 0,
            });
        let transparent = push(&mut graph, ImageOp::Solid { rgba: [0.; 4] });
        fn stack(
            composition: &Composition,
            parent: Option<ObjectId>,
            request: &VideoCompile<'_>,
            graph: &mut RenderGraph,
            transparent: usize,
            definition: Option<&crate::reusable::Definition>,
        ) -> Result<usize, fold_foundation::Failure> {
            let mut output = transparent;
            for layer in composition
                .layers
                .iter()
                .rev()
                .filter(|l| l.parent == parent)
            {
                request.cancel.check()?;
                if !layer.active(request.time) {
                    continue;
                }
                let range =
                    request
                        .context
                        .map(|c| c.timing)
                        .unwrap_or(fold_animation::timing::Range {
                            start: layer.start,
                            duration: layer
                                .end
                                .checked_sub(layer.start)
                                .map_err(|e| e.to_string())?,
                        });
                let format = request.context.map(|c| c.format).unwrap_or(
                    fold_platform::format::Format::square([
                        composition.info.width,
                        composition.info.height,
                    ]),
                );
                if layer.adjustment {
                    let processed = fold_platform::effects::apply_stack(
                        &layer.effects,
                        output,
                        graph,
                        request.time,
                        range,
                        format,
                        request.resolve_effect,
                    )?;
                    let amount = layer
                        .sample("scene.opacity.0", layer.opacity, request.time)
                        .clamp(0., 1.) as f32;
                    output = push(
                        graph,
                        ImageOp::Mix {
                            original: output,
                            processed,
                            mask: None,
                            mask_channel: 3,
                            invert: false,
                            amount,
                            channels: [true; 4],
                        },
                    );
                    continue;
                }
                let input = if let Some(mapping) = definition.and_then(|d| d.graphs.get(&layer.id))
                {
                    let Some(Source::Output(source)) = &layer.source else {
                        return Err("Graph input layer requires a document output".into());
                    };
                    let parent = request
                        .invocation
                        .ok_or("Graph inputs require an invocation")?;
                    let parent_values = definition.unwrap().interface.values(&parent.values)?;
                    let images = mapping
                        .images
                        .iter()
                        .map(|(child, parent_name)| {
                            Ok((
                                child.clone(),
                                *parent
                                    .images
                                    .get(parent_name)
                                    .ok_or("Missing parent image input")?,
                            ))
                        })
                        .collect::<Result<_, String>>()?;
                    let values = mapping
                        .controls
                        .iter()
                        .map(|(child, parent_name)| {
                            Ok((
                                child.clone(),
                                parent_values
                                    .get(parent_name)
                                    .ok_or("Missing parent control")?
                                    .clone(),
                            ))
                        })
                        .collect::<Result<_, String>>()?;
                    *graph = (request.resolve_effect)(
                        source,
                        request.time,
                        &fold_platform::effects::Invocation {
                            graph,
                            images,
                            values,
                            version: mapping.version,
                            timing: range,
                            format,
                        },
                    )?;
                    graph.output
                } else if let Some(name) = definition.and_then(|d| d.images.get(&layer.id)) {
                    *request
                        .invocation
                        .ok_or("Published input requires an effect invocation")?
                        .images
                        .get(name)
                        .ok_or("Missing image input")?
                } else if let Some(source) = &layer.source {
                    let info = match source {
                        Source::Output(reference) => (request.describe_video)(reference)?,
                        Source::Asset(_) => layer.info.clone(),
                    };
                    let local_time = layer.local_time(request.time)?;
                    let source_request = Request {
                        bake: None,
                        source: source.clone(),
                        info,
                        time: local_time,
                        outside: Outside::Black,
                        color_space: None,
                    };
                    graph.append(image_source::compile(&source_request, request)?)?
                } else {
                    stack(
                        composition,
                        Some(layer.id),
                        request,
                        graph,
                        transparent,
                        definition,
                    )?
                };
                let input = fold_platform::effects::apply_stack(
                    &layer.effects,
                    input,
                    graph,
                    request.time,
                    range,
                    format,
                    request.resolve_effect,
                )?;
                let matrix = layer.matrix(request.time);
                let [a, b, c, d, tx, ty] = matrix;
                // A collapsed axis produces no coverage, rather than an invalid inverse.
                if (a * d - b * c).abs() < f64::EPSILON {
                    continue;
                }
                let sx = f64::from(graph.width) / f64::from(composition.info.width);
                let sy = f64::from(graph.height) / f64::from(composition.info.height);
                let transformed = if matrix == crate::layer::IDENTITY {
                    input
                } else {
                    push(
                        graph,
                        ImageOp::Transform {
                            input,
                            transform: Affine {
                                a,
                                b: b * sy / sx,
                                c: c * sx / sy,
                                d,
                                tx: tx * sx,
                                ty: ty * sy,
                            },
                        },
                    )
                };
                let opacity = layer
                    .sample("scene.opacity.0", layer.opacity, request.time)
                    .clamp(0., 1.) as f32;
                let foreground = if opacity == 1. {
                    transformed
                } else {
                    push(
                        graph,
                        ImageOp::Opacity {
                            input: transformed,
                            opacity,
                        },
                    )
                };
                output = push(
                    graph,
                    ImageOp::Merge {
                        first: foreground,
                        second: output,
                        mode: Default::default(),
                    },
                );
            }
            Ok(output)
        }
        graph.output = stack(
            &composition,
            None,
            &request,
            &mut graph,
            transparent,
            definition.as_ref(),
        )?;
        Ok(graph)
    }
}

fn push(graph: &mut RenderGraph, op: ImageOp) -> usize {
    let id = graph.nodes.len();
    graph.nodes.push(op);
    id
}
