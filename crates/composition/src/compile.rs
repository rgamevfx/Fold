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
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        let composition = Composition::from_document(request.document)?;
        if request.reference.output != "video"
            || request.time < Time::ZERO
            || request.time >= composition.info.time(composition.info.frames)?
        {
            return Err("Unsupported composition output or time outside duration".into());
        }
        let [width, height] = request.dimensions;
        let mut graph = RenderGraph {
            width,
            height,
            nodes: vec![ImageOp::Solid { rgba: [0.; 4] }],
            output: 0,
        };
        fn stack(
            composition: &Composition,
            parent: Option<ObjectId>,
            request: &VideoCompile<'_>,
            graph: &mut RenderGraph,
        ) -> Result<usize, String> {
            let mut output = 0;
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
                let input = if let Some(source) = &layer.source {
                    let info = match source {
                        Source::Output(reference) => (request.describe_video)(reference)?,
                        Source::Asset(_) => layer.info.clone(),
                    };
                    let local_time = layer.local_time(request.time)?;
                    if let Source::Output(reference) = source
                        && let Some(object) = layer.graph_object()
                    {
                        if local_time < Time::ZERO || local_time >= info.time(info.frames)? {
                            0
                        } else {
                            let context = fold_platform::layer_graph::EvaluationContext {
                                kind: crate::graph::CONTEXT,
                                data: serde_json::to_value(crate::graph::Context::new(
                                    composition,
                                    reference.document,
                                    object,
                                ))
                                .map_err(|e| e.to_string())?,
                            };
                            graph
                                .append((request.resolve_graph)(reference, local_time, &context)?)?
                        }
                    } else {
                        let source_request = Request {
                            bake: None,
                            source: source.clone(),
                            info,
                            time: local_time,
                            outside: Outside::Black,
                            color_space: None,
                        };
                        graph.append(image_source::compile(&source_request, request)?)?
                    }
                } else {
                    stack(composition, Some(layer.id), request, graph)?
                };
                let matrix: [f64; 6] = std::array::from_fn(|i| {
                    layer.sample(&format!("transform.{i}"), layer.offset[i], request.time)
                });
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
        graph.output = stack(&composition, None, &request, &mut graph)?;
        Ok(graph)
    }
}

fn push(graph: &mut RenderGraph, op: ImageOp) -> usize {
    let id = graph.nodes.len();
    graph.nodes.push(op);
    id
}
