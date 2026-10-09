use super::*;
use fold_platform::{VideoCompile, VideoProvider, workspace::OutputDescriptor};
use fold_project::DocumentRef;
pub struct Provider;
fn info(scene: &Scene) -> Result<fold_media::VideoInfo, String> {
    let duration = scene
        .canvas
        .end
        .checked_sub(scene.canvas.start)
        .map_err(|e| e.to_string())?;
    let frames = duration
        .to_ticks(
            scene.canvas.rate[0],
            scene.canvas.rate[1],
            fold_foundation::Rounding::Ceil,
        )
        .map_err(|e| e.to_string())?;
    let info = fold_media::VideoInfo {
        width: scene.canvas.width,
        height: scene.canvas.height,
        rate: scene.canvas.rate,
        frames: u32::try_from(frames).map_err(|_| "Scene duration exceeds video range")?,
    };
    info.validate()?;
    Ok(info)
}
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn video_info(&self, d: &Document) -> Result<fold_media::VideoInfo, String> {
        info(&Scene::from_document(d)?)
    }
    fn outputs(&self, d: &Document) -> Vec<OutputDescriptor> {
        let Ok(scene) = Scene::from_document(d) else {
            return vec![];
        };
        vec![OutputDescriptor {
            reference: DocumentRef {
                document: d.id,
                output: scene.output.to_string(),
                extensions: Default::default(),
            },
            label: "Scene".into(),
            info: info(&scene),
            playback_mode: fold_platform::desktop::PlaybackMode::EveryFrame,
        }]
    }
    fn compile(&self, r: VideoCompile<'_>) -> Result<fold_render::RenderGraph, String> {
        r.cancel.check()?;
        let scene = Scene::from_document(r.document)?;
        if r.invocation.is_some() || r.reference.output != scene.output.to_string() {
            return Err("Unsupported scene invocation or output".into());
        }
        let duration = scene
            .canvas
            .end
            .checked_sub(scene.canvas.start)
            .map_err(|e| e.to_string())?;
        if r.time < Time::ZERO || r.time >= duration {
            return Err("Scene time outside its half-open range".into());
        }
        crate::rendering::image(
            r.snapshot,
            r.document.id,
            scene.output,
            scene
                .canvas
                .start
                .checked_add(r.time)
                .map_err(|e| e.to_string())?,
            r.dimensions,
            Default::default(),
            &|| r.cancel.check(),
        )
        .map_err(|e| format!("Scene rendering failed: {e:?}"))
    }
}
