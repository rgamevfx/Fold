/// Shell-owned surface, dispatched to the linked editor's composition capability.
pub(crate) const PANEL_ID: &str = "fold.ui.composition";
pub(crate) struct Surface;
impl crate::sdk::Panel for Surface {
    fn id(&self) -> &'static str {
        PANEL_ID
    }
    fn supports_document_type(&self, _: &str) -> bool {
        true
    }
    fn draw(&mut self, context: crate::sdk::ExtensionUi<'_>) {
        context
            .ui
            .text_disabled("Choose a source editor for this group.");
    }
}
