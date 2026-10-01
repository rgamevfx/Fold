//! One shared inspector window, with package-owned property implementations.
use crate::sdk::{ExtensionUi, Panel, imgui};
pub(crate) const ID: &str = "fold.ui.node-inspector";
#[derive(Default)]
pub(crate) struct NodeInspector {
    pub providers: Vec<Box<dyn Panel>>,
}
impl NodeInspector {
    pub fn contains(&self, id: &str) -> bool {
        self.providers.iter().any(|p| p.id() == id)
    }
}
impl Panel for NodeInspector {
    fn id(&self) -> &'static str {
        ID
    }
    fn supports_document_type(&self, kind: &str) -> bool {
        self.providers
            .iter()
            .any(|p| p.document_type() == Some(kind))
    }
    fn initialize(&mut self, context: &imgui::Context) {
        for provider in &mut self.providers {
            provider.initialize(context);
        }
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let snapshot = context.host.snapshot();
        let selected = context
            .host
            .state()
            .selection
            .document
            .or_else(|| context.host.state().navigation.last().map(|v| v.document));
        let kind = snapshot
            .as_ref()
            .and_then(|s| selected.and_then(|id| s.state().documents.get(&id)))
            .map(|d| d.type_id.as_str());
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.document_type() == kind && kind.is_some())
        {
            provider.draw(context);
        } else {
            context
                .ui
                .text_wrapped("Select a node to inspect its properties.");
        }
    }
}
