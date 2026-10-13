//! Motion's immutable image-graph adapter. Compilation captures document state;
//! native compilation and rendering run only when an evaluation worker asks for pixels.
use crate::runtime::Config;
use fold_foundation::{Failure, Rounding};
use fold_motion_canvas::{PACKAGE, TYPE, model::Canvas};
use fold_platform::{VideoCompile, VideoInfo, VideoProvider};
use fold_project::Document;
use fold_render::RenderGraph;
#[cfg(all(feature = "gpu", target_os = "linux"))]
use {
    crate::runtime::gpu::{GpuRuntime, Snapshot},
    fold_foundation::ObjectId,
    fold_media::Cancel,
    fold_render::{
        ImageOp,
        gpu::{GpuFrame, Host},
        source::ImageSource,
    },
    std::sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicU64, Ordering},
    },
};

/// One retained worker per provider. Changing the receiving host retires the
/// worker; documents share the compiled-program cache without retaining an
/// unbounded collection of native processes. Fonts are immutable for its lifetime.
pub struct Provider {
    #[cfg(all(feature = "gpu", target_os = "linux"))]
    renderer: Arc<Renderer>,
    #[cfg(all(feature = "gpu", target_os = "linux"))]
    epoch: ObjectId,
    #[cfg(all(feature = "gpu", target_os = "linux"))]
    generation: AtomicU64,
}
#[cfg(all(feature = "gpu", target_os = "linux"))]
struct Renderer {
    config: Config,
    fonts: Vec<Vec<u8>>,
    runtime: Mutex<Option<(u64, GpuRuntime)>>,
}
impl Provider {
    pub fn new(config: Config, fonts: Vec<Vec<u8>>) -> Self {
        #[cfg(not(all(feature = "gpu", target_os = "linux")))]
        let _ = (config, fonts);
        Self {
            #[cfg(all(feature = "gpu", target_os = "linux"))]
            renderer: Arc::new(Renderer {
                config,
                fonts,
                runtime: Mutex::new(None),
            }),
            #[cfg(all(feature = "gpu", target_os = "linux"))]
            epoch: ObjectId::new(),
            #[cfg(all(feature = "gpu", target_os = "linux"))]
            generation: AtomicU64::new(0),
        }
    }
}
impl VideoProvider for Provider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        TYPE
    }
    fn video_info(&self, document: &Document) -> Result<VideoInfo, String> {
        let canvas = Canvas::from_document(document).map_err(|e| e.to_string())?;
        let frames = canvas
            .duration
            .to_ticks(canvas.rate[0], canvas.rate[1], Rounding::Ceil)
            .map_err(|e| e.to_string())?;
        let info = VideoInfo {
            width: canvas.width,
            height: canvas.height,
            rate: canvas.rate,
            frames: frames
                .try_into()
                .map_err(|_| "Motion duration exceeds frame range")?,
        };
        info.validate().map_err(|e| e.to_string())?;
        Ok(info)
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, Failure> {
        request.cancel.check()?;
        if request.reference.document != request.document.id || request.reference.output != "video"
        {
            return Err("Unknown Motion output".into());
        }
        #[cfg(all(feature = "gpu", target_os = "linux"))]
        {
            let generation = self
                .generation
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| "Motion generation exhausted")?;
            let snapshot = Snapshot::from_document(
                self.epoch,
                generation,
                request.document,
                request.time,
                request.dimensions,
            )?
            .bind_assets(request.snapshot)?;
            let graph = RenderGraph {
                width: request.dimensions[0],
                height: request.dimensions[1],
                nodes: vec![ImageOp::Source(Arc::new(Source {
                    snapshot,
                    renderer: self.renderer.clone(),
                }))],
                output: 0,
            };
            request.cancel.check()?;
            Ok(graph)
        }
        #[cfg(not(all(feature = "gpu", target_os = "linux")))]
        {
            Err("Motion rendering requires the qualified Linux GPU build".into())
        }
    }
}
#[cfg(all(feature = "gpu", target_os = "linux"))]
struct Source {
    snapshot: Snapshot,
    renderer: Arc<Renderer>,
}
#[cfg(all(feature = "gpu", target_os = "linux"))]
impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotionSource")
            .field("intent", &self.snapshot.intent())
            .finish()
    }
}
#[cfg(all(feature = "gpu", target_os = "linux"))]
impl ImageSource for Source {
    fn dimensions(&self) -> [u32; 2] {
        self.snapshot.intent().dimensions
    }
    fn render(&self, host: &Host, cancel: &Cancel) -> Result<GpuFrame, Failure> {
        // Waiting for another frame must remain cancellable. The lock is never
        // acquired on the UI thread, and poisoned worker state is not reused.
        let mut slot = loop {
            cancel.check()?;
            match self.renderer.runtime.try_lock() {
                Ok(slot) => break slot,
                Err(TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(TryLockError::Poisoned(_)) => return Err("Motion renderer state failed".into()),
            }
        };
        if slot.as_ref().is_some_and(|(id, _)| *id != host.id()) {
            slot.take();
        }
        if slot.is_none() {
            *slot = Some((
                host.id(),
                GpuRuntime::new(self.renderer.config.clone(), host.clone(), cancel)?,
            ));
        }
        let runtime = &mut slot
            .as_mut()
            .ok_or("Motion renderer was not initialized")?
            .1;
        let frame = runtime
            .render(&self.snapshot, &self.renderer.fonts, cancel)?
            .into_current(self.snapshot.intent())?;
        cancel.check()?;
        // This check establishes graph consistency. The application's outer
        // preview/export demand still owns publication and supersession guards.
        Ok(frame)
    }
}
