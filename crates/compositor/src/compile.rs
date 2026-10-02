use crate::{Composite, Parameters as P, Source};
use fold_foundation::Time;
use fold_platform::{VideoCompile, VideoProvider};
use fold_render::{Affine, ImageOp as Op, RenderGraph};
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
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        let VideoCompile {
            snapshot,
            document,
            reference,
            time,
            dimensions: [width, height],
            cancel,
            resolve,
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
        let mut ids = BTreeMap::new();
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
            let input = |i: usize| ids[&node.inputs[i].expect("validated reachable socket")];
            let op = match &node.parameters {
                P::Read {
                    source,
                    start,
                    source_start,
                    duration,
                } => {
                    let end = start.checked_add(*duration).map_err(|e| e.to_string())?;
                    if time < *start || time >= end {
                        Op::Solid { rgba: [0.0; 4] }
                    } else {
                        let local = time
                            .checked_sub(*start)
                            .and_then(|t| t.checked_add(*source_start))
                            .map_err(|e| e.to_string())?;
                        match source {
                            Source::Document { source, .. } => {
                                let id = graph.append(resolve(source, local)?)?;
                                ids.insert(node.id, id);
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
                P::Transform {
                    translate,
                    scale,
                    opacity,
                } => {
                    graph.nodes.push(Op::Transform {
                        input: input(0),
                        transform: Affine {
                            a: scale[0],
                            d: scale[1],
                            tx: translate[0] * sx,
                            ty: translate[1] * sy,
                            ..Affine::IDENTITY
                        },
                    });
                    Op::Opacity {
                        input: graph.nodes.len() - 1,
                        opacity: *opacity,
                    }
                }
                P::Crop { rect: r } => Op::Crop {
                    input: input(0),
                    rect: rect(*r),
                },
                P::Blur { radius } => {
                    let radius = (f64::from(*radius) * sx.max(sy)).round() as u32;
                    if radius > 64 {
                        return Err(
                            "scaled blur radius exceeds 64 pixels at the requested resolution"
                                .into(),
                        );
                    }
                    Op::Blur {
                        input: input(0),
                        radius,
                    }
                }
                P::Grade { gain } => Op::Grade {
                    input: input(0),
                    gain: *gain,
                },
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
                P::Merge => Op::Over {
                    foreground: input(0),
                    background: input(1),
                },
                P::Output => {
                    ids.insert(node.id, input(0));
                    continue;
                }
            };
            ids.insert(node.id, graph.nodes.len());
            graph.nodes.push(op);
            if graph.nodes.len() > 4096 {
                return Err("composite compilation exceeds 4096 IR nodes".into());
            }
        }
        graph.output = ids[&composite.output];
        Ok(graph)
    }
}
