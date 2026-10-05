//! Contextual graph surface; providers retain all graph semantics and state.
pub const PANEL_ID: &str = "fold.ui.network";
pub(crate) struct Surface;
impl crate::sdk::Panel for Surface {
    fn id(&self) -> &'static str {
        PANEL_ID
    }
    fn supports_document_type(&self, _: &str) -> bool {
        true
    }
    fn draw(&mut self, context: crate::sdk::ExtensionUi<'_>) {
        context.ui.text_disabled("Choose a network context.");
    }
}
