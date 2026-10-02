//! Static capability contracts. Only the application assembles implementations.
pub mod browser;
pub mod desktop;
pub mod packages;
use fold_foundation::Time;
use fold_media::Cancel;
use fold_project::{Document, DocumentRef, Snapshot};
pub use fold_project::{Revision as ProjectRevision, Snapshot as ProjectSnapshot};
use fold_render::RenderGraph;

// Presentation contract, not access to engine resources or GPU implementation.
pub use fold_render::DisplayFrame;

/// Nested outputs compile into the caller's IR, never into rendered frames.
pub type VideoResolver<'a> = dyn Fn(&DocumentRef, Time) -> Result<RenderGraph, String> + 'a;

/// One compilation interface for every source, including legacy adapters.
/// Nested resolution inherits the snapshot, dimensions and cancellation token.
pub struct VideoCompile<'a> {
    pub snapshot: &'a Snapshot,
    pub document: &'a Document,
    pub reference: &'a DocumentRef,
    pub time: Time,
    pub dimensions: [u32; 2],
    pub cancel: &'a Cancel,
    pub resolve: &'a VideoResolver<'a>,
}

pub trait VideoProvider: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    /// Duration and raster metadata belongs to the video capability, not every
    /// document schema. Legacy generated-image adapters may have no timed output.
    fn video_info(&self, _document: &Document) -> Result<fold_media::VideoInfo, String> {
        Err("video provider has no timed output metadata".into())
    }
    /// Reference-local controls must be explicitly supported, never silently ignored.
    fn supports_controls(&self) -> bool {
        false
    }
    fn compile(&self, request: VideoCompile<'_>) -> Result<RenderGraph, String>;
}

#[derive(Default)]
pub struct VideoRegistry {
    providers: Vec<Box<dyn VideoProvider>>,
}
impl VideoRegistry {
    pub fn register(&mut self, provider: impl VideoProvider + 'static) -> Result<(), String> {
        self.register_boxed(Box::new(provider))
    }
    pub fn register_boxed(&mut self, provider: Box<dyn VideoProvider>) -> Result<(), String> {
        if self
            .providers
            .iter()
            .any(|p| p.package_id() == provider.package_id() && p.type_id() == provider.type_id())
        {
            return Err("duplicate video provider".into());
        }
        self.providers.push(provider);
        Ok(())
    }

    /// Convenience entry for synchronous callers without a cancellable job.
    pub fn compile(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        self.compile_with(snapshot, source, time, [width, height], &Cancel::default())
    }

    pub fn compile_with(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RenderGraph, String> {
        self.compile_nested(snapshot, source, time, dimensions, cancel, &[])
    }

    fn compile_nested(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
        ancestors: &[fold_foundation::DocumentId],
    ) -> Result<RenderGraph, String> {
        cancel.check()?;
        if ancestors.contains(&source.document) || ancestors.len() >= 64 {
            return Err(format!(
                "recursive or excessively deep document reference: {:?}",
                source.document
            ));
        }
        let document = snapshot
            .state()
            .documents
            .get(&source.document)
            .ok_or_else(|| format!("missing document {:?}", source.document))?;
        let provider = self
            .providers
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or_else(|| {
                format!(
                    "missing video provider for {}/{} (document {:?})",
                    document.package_id, document.type_id, document.id
                )
            })?;
        if source.extensions.contains_key("fold.controls") && !provider.supports_controls() {
            return Err("referenced provider does not support control overrides".into());
        }
        let mut path = ancestors.to_vec();
        path.push(source.document);
        let resolve = |nested: &DocumentRef, local_time: Time| {
            cancel.check()?;
            if !document.dependencies.contains(nested) {
                return Err(format!(
                    "undeclared document dependency: {:?}",
                    nested.document
                ));
            }
            self.compile_nested(snapshot, nested, local_time, dimensions, cancel, &path)
        };
        let graph = provider.compile(VideoCompile {
            snapshot,
            document,
            reference: source,
            time,
            dimensions,
            cancel,
            resolve: &resolve,
        })?;
        cancel.check()?;
        Ok(graph)
    }
}
