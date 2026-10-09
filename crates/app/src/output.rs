//! Explicit delivery evaluation shared by still-image CLI and video export.
//! One evaluator owns backend selection and retained resources for a pinned job.
#[path = "output_export.rs"]
mod export;
#[path = "output_identity.rs"]
mod identity;
#[path = "output_scene.rs"]
mod scene;
pub use export::export;
#[cfg(feature = "gpu")]
pub(crate) use export::export_with_host;
use fold_media::{Cancel, Decoder};
use fold_project::CommittedSnapshot;
use fold_render::DisplayFrame;
pub(crate) use identity::reference;
pub use identity::{content, content_for, output, select_output};
#[cfg(feature = "gpu")]
pub use scene::evaluate_scene_gpu;
#[cfg(feature = "gpu")]
pub(crate) use scene::evaluate_scene_gpu_admitted;
pub use scene::{SceneRequest, evaluate, evaluate_scene};
pub(crate) use scene::{compile_scene, preview_request};

pub struct OutputRenderer {
    decoder: Decoder,
    #[cfg(feature = "gpu")]
    gpu: Option<fold_render::gpu::Renderer>,
}
impl OutputRenderer {
    pub fn from_environment() -> Result<Self, fold_foundation::Failure> {
        let decoder = Decoder::from_environment()?;
        #[cfg(feature = "gpu")]
        let gpu = match std::env::var("FOLD_RENDER_BACKEND").as_deref() {
            Ok("gpu") => {
                let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(
                    fold_render::gpu::Host::DEFAULT_BUDGET,
                ))?;
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
    pub(crate) fn with_shared_host(
        host: Option<fold_render::gpu::Host>,
    ) -> Result<Self, fold_foundation::Failure> {
        match host {
            Some(host) => Ok(Self {
                decoder: Decoder::from_environment()?,
                gpu: Some(fold_render::gpu::Renderer::new(host)?),
            }),
            None => Self::from_environment(),
        }
    }
    pub fn backend(&self) -> &'static str {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() {
            return "gpu";
        }
        "cpu"
    }
    /// Alpha delivery reads scene precision, never opaque/quantized presentation.
    pub fn evaluate_rgba16(
        &mut self,
        snapshot: &CommittedSnapshot,
        request: &SceneRequest,
        cancel: &Cancel,
    ) -> Result<fold_render::Rgba16Output, fold_foundation::Failure> {
        self.with_budget_retry(cancel, |renderer| {
            renderer.evaluate_rgba16_once(snapshot, request, cancel)
        })
        .map_err(|error| {
            error
                .context(request.source.document)
                .context(&request.source.output)
        })
    }
    fn evaluate_rgba16_once(
        &mut self,
        snapshot: &CommittedSnapshot,
        request: &SceneRequest,
        cancel: &Cancel,
    ) -> Result<fold_render::Rgba16Output, fold_foundation::Failure> {
        cancel.check()?;
        #[cfg(feature = "gpu")]
        let scene = if let Some(renderer) = self.gpu.as_mut() {
            let mut scene = crate::output::evaluate_scene_gpu_admitted(
                snapshot,
                request,
                renderer,
                &mut self.decoder,
                cancel,
                Some(fold_render::scheduling::Class::Export),
            )?;
            scene.readback(cancel)?
        } else {
            let _permit = fold_render::scheduling::Scheduler::shared()
                .enter(fold_render::scheduling::Class::Export, cancel)?;
            evaluate_scene(snapshot, request, &mut self.decoder, cancel)?
        };
        #[cfg(not(feature = "gpu"))]
        let scene = {
            let _permit = fold_render::scheduling::Scheduler::shared()
                .enter(fold_render::scheduling::Class::Export, cancel)?;
            evaluate_scene(snapshot, request, &mut self.decoder, cancel)?
        };
        crate::color::with_config(snapshot, |config| {
            let processor = config
                .map(|config| {
                    config.display(
                        fold_color::WORKING_SPACE,
                        &fold_platform::color::output(snapshot, request.source.document)?
                            .display_transform(),
                    )
                })
                .transpose()?;
            scene.into_rgba16(processor.as_ref())
        })
    }
    pub fn evaluate(
        &mut self,
        snapshot: &CommittedSnapshot,
        request: &SceneRequest,
        cancel: &Cancel,
    ) -> Result<DisplayFrame, fold_foundation::Failure> {
        self.with_budget_retry(cancel, |renderer| {
            renderer.evaluate_once(snapshot, request, cancel)
        })
        .map_err(|error| {
            error
                .context(request.source.document)
                .context(&request.source.output)
        })
    }
    fn with_budget_retry<T>(
        &mut self,
        cancel: &Cancel,
        mut evaluate: impl FnMut(&mut Self) -> Result<T, fold_foundation::Failure>,
    ) -> Result<T, fold_foundation::Failure> {
        let began = std::time::Instant::now();
        loop {
            cancel.check()?;
            match evaluate(self) {
                Err(error) if error.retryable() && began.elapsed().as_secs() < 30 => {
                    // Submitted preview leases may still be in flight. Release
                    // admission and partial work before waiting; the pinned
                    // request remains immutable and no encoder output was sent.
                    #[cfg(feature = "gpu")]
                    if let Some(renderer) = &self.gpu {
                        renderer.host().poll()?;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                result => return result,
            }
        }
    }
    fn evaluate_once(
        &mut self,
        snapshot: &CommittedSnapshot,
        request: &SceneRequest,
        cancel: &Cancel,
    ) -> Result<DisplayFrame, fold_foundation::Failure> {
        cancel.check()?;
        #[cfg(feature = "gpu")]
        if let Some(renderer) = self.gpu.as_mut() {
            let scene = crate::output::evaluate_scene_gpu_admitted(
                snapshot,
                request,
                renderer,
                &mut self.decoder,
                cancel,
                Some(fold_render::scheduling::Class::Export),
            )?;
            let output_permit = fold_render::scheduling::Scheduler::shared()
                .enter(fold_render::scheduling::Class::Export, cancel)?;
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
            drop(scene);
            drop(output_permit);
            let result = output.readback(cancel);
            #[cfg(feature = "native-probe")]
            if std::env::var_os("FOLD_SHARED_PROBE").is_some() {
                eprintln!(
                    "GPU export: time={:?} unix_ms={} transfers={:?}",
                    request.time,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                    output.statistics
                );
            }
            return result;
        }
        let _admission = fold_render::scheduling::Scheduler::shared()
            .enter(fold_render::scheduling::Class::Export, cancel)?;
        let scene = evaluate_scene(snapshot, request, &mut self.decoder, cancel)?.over_black();
        crate::color::delivery(snapshot, request.source.document, &scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::{Failure, FailureKind};
    fn renderer() -> OutputRenderer {
        OutputRenderer {
            decoder: Decoder::default(),
            #[cfg(feature = "gpu")]
            gpu: None,
        }
    }
    #[test]
    fn pressure_retries_independently_of_wording_but_permanent_failure_does_not() {
        let mut attempts = 0;
        let result = renderer()
            .with_budget_retry(&Cancel::default(), |_| {
                attempts += 1;
                if attempts == 1 {
                    Err(Failure::new(FailureKind::ResourcePressure, "busy"))
                } else {
                    Ok(42)
                }
            })
            .unwrap();
        assert_eq!((result, attempts), (42, 2));
        for kind in [
            FailureKind::RequestTooLarge,
            FailureKind::Execution,
            FailureKind::InvalidInput,
        ] {
            let mut attempts = 0;
            let result: Result<(), _> = renderer().with_budget_retry(&Cancel::default(), |_| {
                attempts += 1;
                Err(Failure::new(kind, "GPU working budget exhausted"))
            });
            assert_eq!(result.unwrap_err().kind, kind);
            assert_eq!(attempts, 1);
        }
    }
    #[test]
    fn cancellation_stops_export_retry_before_another_evaluation() {
        let cancel = Cancel::default();
        let mut attempts = 0;
        let result: Result<(), _> = renderer().with_budget_retry(&cancel, |_| {
            attempts += 1;
            cancel.cancel();
            Err(Failure::new(FailureKind::ResourcePressure, "busy"))
        });
        assert_eq!(result.unwrap_err().kind, FailureKind::Cancelled);
        assert_eq!(attempts, 1);
    }
}
