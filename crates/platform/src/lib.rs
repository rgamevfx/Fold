//! Static capability contracts. Only the application assembles implementations.
use fold_foundation::Time;
use fold_project::{Document, DocumentRef, Snapshot};
use fold_render::SolidPlan;

// Presentation contract, not access to engine resources or GPU implementation.
pub use fold_render::DisplayFrame;

pub trait VideoProvider: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    fn compile(
        &self,
        document: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<SolidPlan, String>;
}

#[derive(Default)]
pub struct VideoRegistry {
    providers: Vec<Box<dyn VideoProvider>>,
}
impl VideoRegistry {
    pub fn register(&mut self, provider: impl VideoProvider + 'static) -> Result<(), String> {
        if self
            .providers
            .iter()
            .any(|p| p.package_id() == provider.package_id() && p.type_id() == provider.type_id())
        {
            return Err("duplicate video provider".into());
        }
        self.providers.push(Box::new(provider));
        Ok(())
    }

    pub fn compile(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<SolidPlan, String> {
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
        provider.compile(document, &source.output, time, width, height)
    }
}
