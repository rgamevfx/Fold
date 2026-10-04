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
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String> {
        let VideoCompile {
            snapshot,
            document,
            reference,
            time,
            dimensions: [width, height],
            cancel,
            resolve,
            ..
        } = request;
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
                rgba: [0.0, 0.0, 0.0, 1.0],
            }],
            output: 0,
        };
        for track in sequence
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && t.enabled)
        {
            for clip in sequence
                .clips
                .iter()
                .filter(|c| c.track == track.id && c.level > 0.0)
            {
                cancel.check()?;
                if let Some(time) = clip.source_time(time)? {
                    let (source, opaque) = match &clip.info {
                        SourceMedia::Document { source, .. } => {
                            (graph.append(resolve(source, time)?)?, false)
                        }
                        SourceMedia::Video(info) => {
                            let asset = snapshot
                                .state()
                                .assets
                                .get(&clip.asset)
                                .ok_or("missing sequence asset")?;
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
                            (id, true)
                        }
                        SourceMedia::Audio(_) => return Err("non-video clip on video track".into()),
                    };
                    if opaque && clip.level == 1.0 {
                        graph.output = source;
                    } else {
                        let foreground = graph.nodes.len();
                        graph.nodes.push(ImageOp::Opacity {
                            input: source,
                            opacity: clip.level,
                        });
                        graph.nodes.push(ImageOp::Over {
                            foreground,
                            background: graph.output,
                        });
                        graph.output = graph.nodes.len() - 1;
                    }
                }
            }
        }
        Ok(graph)
    }
}
