//! Video-track lowering shared by preview and export. Higher video tracks
//! composite over lower tracks; gaps/disabled tracks contribute nothing.
use crate::{SEQUENCE, Sequence, SourceMedia, TrackKind};
use fold_foundation::Time;
use fold_media::VideoSource;
use fold_platform::VideoProvider;
use fold_project::{Document, Snapshot};
use fold_render::{ImageOp, RenderGraph};

pub struct SequenceProvider;
impl VideoProvider for SequenceProvider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        SEQUENCE
    }
    fn compile(
        &self,
        _: &Document,
        _: &str,
        _: Time,
        _: u32,
        _: u32,
    ) -> Result<RenderGraph, String> {
        Err("sequence requires snapshot asset resolution".into())
    }
    fn compile_snapshot(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        if output != "video" || time < Time::ZERO {
            return Err("unsupported sequence output or negative time".into());
        }
        let sequence = Sequence::from_document(document)?;
        let mut nodes = vec![ImageOp::Solid {
            rgba: [0.0, 0.0, 0.0, 1.0],
        }];
        let mut background = 0;
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
                if let Some(time) = clip.source_time(time)? {
                    let asset = snapshot
                        .state()
                        .assets
                        .get(&clip.asset)
                        .ok_or("missing sequence asset")?;
                    let SourceMedia::Video(info) = &clip.info else {
                        return Err("non-video clip on video track".into());
                    };
                    let source = nodes.len();
                    nodes.push(ImageOp::Video {
                        source: VideoSource {
                            path: asset.location.clone().into(),
                            fingerprint: asset.fingerprint.clone(),
                            info: info.clone(),
                        },
                        time,
                    });
                    if clip.level == 1.0 {
                        // Supported video is opaque: dead lower layers need not decode.
                        background = source;
                    } else {
                        let foreground = nodes.len();
                        nodes.push(ImageOp::Opacity {
                            input: source,
                            opacity: clip.level,
                        });
                        nodes.push(ImageOp::Over {
                            foreground,
                            background,
                        });
                        background = nodes.len() - 1;
                    }
                }
            }
        }
        Ok(RenderGraph {
            width,
            height,
            nodes,
            output: background,
        })
    }
}
