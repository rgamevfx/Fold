//! Explicit delivery evaluation shared by still-image CLI and video export.
//! One evaluator owns backend selection and retained resources for a pinned job.
use crate::media_workflow::{SceneRequest, evaluate_scene};
use fold_media::{Cancel, Decoder};
use fold_project::CommittedSnapshot;
use fold_render::DisplayFrame;

pub struct OutputRenderer {
    decoder: Decoder,
    #[cfg(feature = "gpu")]
    gpu: Option<fold_render::gpu::Renderer>,
}
impl OutputRenderer {
    pub fn from_environment() -> Result<Self, String> {
        let decoder = Decoder::from_environment()?;
        #[cfg(feature = "gpu")]
        let gpu = match std::env::var("FOLD_RENDER_BACKEND").as_deref() {
            Ok("gpu") => {
                let (host, _) =
                    pollster::block_on(fold_render::gpu::Host::headless(512 * 1024 * 1024))?;
                Some(fold_render::gpu::Renderer::new(host)?)
            }
            Ok("cpu") | Err(std::env::VarError::NotPresent) => None,
            _ => return Err("FOLD_RENDER_BACKEND must be cpu or gpu".into()),
        };
        #[cfg(not(feature = "gpu"))]
        if std::env::var("FOLD_RENDER_BACKEND").is_ok_and(|v| v != "cpu") {
            return Err("GPU delivery requires a build with the gpu feature".into());
        }
        Ok(Self {
            decoder,
            #[cfg(feature = "gpu")]
            gpu,
        })
    }
    #[cfg(feature = "gpu")]
    pub(crate) fn with_shared_host(host: Option<fold_render::gpu::Host>) -> Result<Self, String> {
        match host {
            Some(host) => Ok(Self {
                decoder: Decoder::from_environment()?,
                gpu: Some(fold_render::gpu::Renderer::new(host)?),
            }),
            None => Self::from_environment(),
        }
    }
    pub fn evaluate(
        &mut self,
        snapshot: &CommittedSnapshot,
        request: &SceneRequest,
        cancel: &Cancel,
    ) -> Result<DisplayFrame, String> {
        cancel.check()?;
        let _admission = fold_render::scheduling::Scheduler::shared()
            .enter(fold_render::scheduling::Class::Export, cancel)?;
        #[cfg(feature = "gpu")]
        if let Some(renderer) = self.gpu.as_mut() {
            let scene = crate::media_workflow::evaluate_scene_gpu(
                snapshot,
                request,
                renderer,
                &mut self.decoder,
                cancel,
            )?;
            let mut output = crate::color::with_config(snapshot, |config| {
                let processor = config
                    .map(|c| {
                        c.display(
                            fold_color::WORKING_SPACE,
                            &fold_platform::color::output(snapshot, request.source.document)?
                                .display_transform(),
                        )
                    })
                    .transpose()?;
                renderer.output(&scene, processor.as_ref(), cancel)
            })?;
            return output.readback(cancel);
        }
        let scene = evaluate_scene(snapshot, request, &mut self.decoder, cancel)?.over_black();
        crate::color::delivery(snapshot, request.source.document, &scene)
    }
}
