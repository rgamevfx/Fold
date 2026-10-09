//! Motion output registration over the shared immutable video compilation service.
use crate::{model::Construction, rendering, types::Type};
use fold_foundation::{Rounding, Time};
use fold_platform::{VideoCompile, VideoProvider, workspace::OutputDescriptor};
use fold_project::{Document, DocumentRef};
use fold_render::{ImageOp, RenderGraph};

pub(crate) struct Provider;
fn info(construction: &Construction) -> Result<fold_media::VideoInfo, String> {
    let canvas = &construction.canvas;
    let duration = canvas
        .end
        .checked_sub(canvas.start)
        .map_err(|e| e.to_string())?;
    let frames = duration
        .to_ticks(canvas.rate[0], canvas.rate[1], Rounding::Ceil)
        .map_err(|e| e.to_string())?;
    let info = fold_media::VideoInfo {
        width: canvas.width,
        height: canvas.height,
        rate: canvas.rate,
        frames: u32::try_from(frames).map_err(|_| "Motion duration exceeds video frame range")?,
    };
    info.validate()?;
    Ok(info)
}
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::CONSTRUCTION
    }
    fn playback_mode(&self) -> fold_platform::desktop::PlaybackMode {
        fold_platform::desktop::PlaybackMode::EveryFrame
    }
    fn video_info(&self, document: &Document) -> Result<fold_media::VideoInfo, String> {
        info(&Construction::from_document(document)?)
    }
    fn outputs(&self, document: &Document) -> Vec<OutputDescriptor> {
        let Ok(construction) = Construction::from_document(document) else {
            return vec![];
        };
        let Some(root) = construction
            .definitions
            .iter()
            .find(|d| d.id == construction.root.definition)
        else {
            return vec![];
        };
        root.outputs
            .iter()
            .filter(|output| output.ty == Type::Scene)
            .map(|output| OutputDescriptor {
                reference: DocumentRef {
                    document: document.id,
                    output: output.id.to_string(),
                    extensions: Default::default(),
                },
                label: output.name.clone(),
                info: info(&construction),
                playback_mode: self.playback_mode(),
            })
            .collect()
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, fold_foundation::Failure> {
        request.cancel.check()?;
        if request.invocation.is_some() {
            return Err("Motion image-effect invocation is not qualified".into());
        }
        let construction = Construction::from_document(request.document)?;
        let root = construction
            .definitions
            .iter()
            .find(|d| d.id == construction.root.definition)
            .ok_or("Missing Motion root definition")?;
        let output = root
            .outputs
            .iter()
            .find(|o| o.ty == Type::Scene && o.id.to_string() == request.reference.output)
            .ok_or("Missing Motion Scene output")?;
        let duration = construction
            .canvas
            .end
            .checked_sub(construction.canvas.start)
            .map_err(|e| e.to_string())?;
        if request.time < Time::ZERO || request.time >= duration {
            return Err("Motion output time outside duration".into());
        }
        let scene_time = construction
            .canvas
            .start
            .checked_add(request.time)
            .map_err(|e| e.to_string())?;
        let mut graph = rendering::image(
            request.snapshot,
            request.document.id,
            output.id,
            scene_time,
            request.dimensions,
            Default::default(),
            &|| Ok(request.cancel.check()?),
        )
        .map_err(|e| format!("Motion image preparation failed: {e:?}"))?;
        let aces = fold_platform::color::project(request.snapshot)?.is_some();
        for op in &mut graph.nodes {
            request.cancel.check()?;
            if let ImageOp::Vector(drawings) = op {
                if aces {
                    for drawing in std::sync::Arc::make_mut(drawings) {
                        request.cancel.check()?;
                        drawing.fill = drawing
                            .fill
                            .map(fold_platform::color::linear_rec709_to_aces);
                        if let Some(stroke) = &mut drawing.stroke {
                            stroke.color =
                                fold_platform::color::linear_rec709_to_aces(stroke.color);
                        }
                    }
                }
                fold_render::vector::validate_working(drawings, aces)?;
            }
        }
        request.cancel.check()?;
        Ok(graph)
    }
}
