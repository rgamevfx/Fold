//! Static capability contracts. Only the application assembles implementations.
pub mod desktop;
pub mod packages;
use fold_foundation::Time;
use fold_project::{Document, DocumentRef, Snapshot};
pub use fold_project::{Revision as ProjectRevision, Snapshot as ProjectSnapshot};
use fold_render::RenderGraph;

// Presentation contract, not access to engine resources or GPU implementation.
pub use fold_render::DisplayFrame;

/// Nested outputs compile into the caller's IR, never into rendered frames.
pub type VideoResolver<'a> = dyn Fn(&DocumentRef, Time) -> Result<RenderGraph, String> + 'a;

pub trait VideoProvider: Send + Sync {
    /// Reference-local provider settings (such as exposed controls) do not mutate
    /// the source document. Providers explicitly opt into interpreting them.
    fn compile_reference(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        reference: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        resolve: &VideoResolver<'_>,
    ) -> Result<RenderGraph, String> {
        if reference.extensions.contains_key("fold.controls") {
            return Err("referenced provider does not support control overrides".into());
        }
        self.compile_resolved(
            snapshot,
            document,
            &reference.output,
            time,
            dimensions,
            resolve,
        )
    }
    fn compile_resolved(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        output: &str,
        time: Time,
        dimensions: [u32; 2],
        _resolve: &VideoResolver<'_>,
    ) -> Result<RenderGraph, String> {
        self.compile_snapshot(
            snapshot,
            document,
            output,
            time,
            dimensions[0],
            dimensions[1],
        )
    }
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    fn compile_snapshot(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        let _ = snapshot;
        self.compile(document, output, time, width, height)
    }
    fn compile(
        &self,
        document: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String>;
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

    pub fn compile(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        self.compile_nested(snapshot, source, time, width, height, &[])
    }

    fn compile_nested(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
        ancestors: &[fold_foundation::DocumentId],
    ) -> Result<RenderGraph, String> {
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
        let mut path = ancestors.to_vec();
        path.push(source.document);
        let resolve = |nested: &DocumentRef, local_time: Time| {
            if !document.dependencies.contains(nested) {
                return Err(format!(
                    "undeclared document dependency: {:?}",
                    nested.document
                ));
            }
            self.compile_nested(snapshot, nested, local_time, width, height, &path)
        };
        provider.compile_reference(snapshot, document, source, time, [width, height], &resolve)
    }
}
