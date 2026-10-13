//! Video-track lowering shared by preview and export. Nested outputs are
//! compiled through capabilities, without importing another feature's model.
use crate::{SEQUENCE, Sequence, SourceMedia, TrackKind};
use fold_foundation::Time;
use fold_media::VideoSource;
use fold_platform::{VideoCompile, VideoProvider};
use fold_render::{ImageOp, RenderGraph};

pub struct SequenceProvider;
impl VideoProvider for SequenceProvider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        SEQUENCE
    }
    fn video_info(
        &self,
        document: &fold_project::Document,
    ) -> Result<fold_media::VideoInfo, String> {
        Sequence::from_document(document)?.info()
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, fold_foundation::Failure> {
        let document = request.document;
        let reference = request.reference;
        let time = request.time;
        let [width, height] = request.dimensions;
        let cancel = request.cancel;
        let output = reference.output.as_str();
        cancel.check()?;
        if output != "video" || time < Time::ZERO {
            return Err("unsupported sequence output or negative time".into());
        }
        let sequence = Sequence::from_document(document)?;
        let mut graph = RenderGraph {
            width,
            height,
            nodes: vec![ImageOp::Solid {
                // Empty timeline regions have no coverage. A black matte must
                // be authored upstream rather than silently destroying alpha.
                rgba: [0.; 4],
            }],
            output: 0,
        };
        for track in sequence
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && t.enabled)
        {
            let background = graph.output;
            if let Some(clip) = sequence.clips.iter().find(|c| {
                c.track == track.id
                    && matches!(c.info, SourceMedia::Adjustment)
                    && c.start <= time
                    && c.end().is_ok_and(|end| time < end)
            }) {
                let processed = fold_platform::effects::apply_stack(
                    &clip.effects,
                    background,
                    &mut graph,
                    time,
                    fold_animation::timing::Range {
                        start: clip.start,
                        duration: clip.duration,
                    },
                    fold_platform::format::Format::square(sequence.dimensions),
                    request.resolve_effect,
                )?;
                graph.nodes.push(ImageOp::Mix {
                    original: background,
                    processed,
                    mask: None,
                    mask_channel: 3,
                    invert: false,
                    amount: clip.level,
                    channels: [true; 4],
                });
                graph.output = graph.nodes.len() - 1;
                continue;
            }
            let transition = sequence.transitions.iter().find(|t| {
                sequence
                    .clips
                    .iter()
                    .any(|c| c.id == t.outgoing && c.track == track.id)
                    && t.active(time).unwrap_or(false)
            });
            let image = if let Some(transition) = transition {
                let a = sequence
                    .clips
                    .iter()
                    .find(|c| c.id == transition.outgoing)
                    .ok_or("Missing outgoing clip")?;
                let b = sequence
                    .clips
                    .iter()
                    .find(|c| c.id == transition.incoming)
                    .ok_or("Missing incoming clip")?;
                let outgoing = clip_image(&sequence, a, &request, &mut graph)?;
                let incoming = clip_image(&sequence, b, &request, &mut graph)?;
                if transition.effect.enabled {
                    graph = (request.resolve_effect)(
                        &transition.effect.definition,
                        time,
                        &fold_platform::effects::Invocation {
                            graph: &graph,
                            images: std::collections::BTreeMap::from([
                                ("outgoing".into(), outgoing),
                                ("incoming".into(), incoming),
                            ]),
                            values: transition.effect.values_at(time)?,
                            version: transition.effect.version,
                            timing: fold_animation::timing::Range {
                                start: transition.start,
                                duration: transition.duration,
                            },
                            format: fold_platform::format::Format::square(sequence.dimensions),
                        },
                    )?;
                    Some(graph.output)
                } else {
                    Some(if time < b.start { outgoing } else { incoming })
                }
            } else if let Some(clip) = sequence.clips.iter().find(|c| {
                c.track == track.id && c.start <= time && c.end().is_ok_and(|end| time < end)
            }) {
                Some(clip_image(&sequence, clip, &request, &mut graph)?)
            } else {
                None
            };
            if let Some(foreground) = image {
                let opaque = transition.is_none()
                    && sequence.clips.iter().any(|c| {
                        c.track == track.id
                            && c.start <= time
                            && c.end().is_ok_and(|end| time < end)
                            && c.level == 1.
                            && c.effects.is_empty()
                            && c.transform()
                                .is_ok_and(|t| t == crate::Transform::default())
                            && matches!(c.info, SourceMedia::Video(_))
                    });
                if opaque {
                    graph.output = foreground;
                } else {
                    graph.nodes.push(ImageOp::Over {
                        foreground,
                        background,
                    });
                    graph.output = graph.nodes.len() - 1;
                }
            }
        }
        Ok(graph)
    }
}

fn clip_image(
    sequence: &Sequence,
    clip: &crate::Clip,
    request: &VideoCompile<'_>,
    graph: &mut RenderGraph,
) -> Result<usize, fold_foundation::Failure> {
    request.cancel.check()?;
    if clip.level == 0. {
        let id = graph.nodes.len();
        graph.nodes.push(ImageOp::Solid { rgba: [0.; 4] });
        return Ok(id);
    }
    let time = clip
        .source_start
        .checked_add(
            request
                .time
                .checked_sub(clip.start)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    let source = match &clip.info {
        SourceMedia::Document { source, .. } => graph.append((request.resolve)(source, time)?)?,
        SourceMedia::Video(info) => {
            let asset = request
                .snapshot
                .state()
                .assets
                .get(&clip.asset)
                .ok_or("Missing sequence asset")?;
            let id = graph.nodes.len();
            graph.nodes.push(ImageOp::video(
                VideoSource {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: info.clone(),
                },
                time,
                fold_platform::color::input(&asset.extensions)?,
            ));
            id
        }
        SourceMedia::Image(info) => graph.append(fold_platform::image_source::compile(
            &fold_platform::image_source::Request {
                source: fold_platform::image_source::Source::Asset(clip.asset),
                time,
                info: info.clone(),
                outside: Default::default(),
                color_space: None,
                bake: None,
            },
            request,
        )?)?,
        SourceMedia::Adjustment => {
            return Err("Adjustment images require lower-track context".into());
        }
        SourceMedia::Audio(_) => return Err("Non-video clip on video track".into()),
    };
    let source = fold_platform::effects::apply_stack(
        &clip.effects,
        source,
        graph,
        request.time,
        fold_animation::timing::Range {
            start: clip.start,
            duration: clip.duration,
        },
        fold_platform::format::Format::square(sequence.dimensions),
        request.resolve_effect,
    )?;
    let transform = clip.transform()?;
    let source = if transform != crate::Transform::default() {
        let id = graph.nodes.len();
        if transform.scale.contains(&0.) {
            graph.nodes.push(ImageOp::Solid { rgba: [0.; 4] });
        } else {
            graph.nodes.push(ImageOp::Transform {
                input: source,
                transform: transform.matrix(sequence.dimensions, [graph.width, graph.height]),
            });
        }
        id
    } else {
        source
    };
    if clip.level == 1. {
        Ok(source)
    } else {
        let id = graph.nodes.len();
        graph.nodes.push(ImageOp::Opacity {
            input: source,
            opacity: clip.level,
        });
        Ok(id)
    }
}
