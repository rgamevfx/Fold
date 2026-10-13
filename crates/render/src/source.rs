//! Feature-independent immutable image producers. Implementations retain their
//! own document snapshot and runtime; no feature document models enter the IR.
pub trait ImageSource: std::fmt::Debug + Send + Sync {
    fn dimensions(&self) -> [u32; 2];
    /// Produce premultiplied linear Rec.709 pixels on the supplied host.
    /// The frame must retain its allocation and completion ownership, use this
    /// exact host and match `dimensions`. Runs on an evaluation worker.
    #[cfg(feature = "gpu")]
    fn render(
        &self,
        host: &crate::gpu::Host,
        cancel: &fold_media::Cancel,
    ) -> Result<crate::gpu::GpuFrame, fold_foundation::Failure>;
}
