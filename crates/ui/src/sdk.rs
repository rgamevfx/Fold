//! Small trusted, in-process panel SDK. Panels own presentation state only.
//! Backend/device access stays private; raw ImGui is an explicitly unstable
//! escape hatch for custom editors, not a promised native plugin ABI.
#[cfg(test)]
#[path = "sdk_tests.rs"]
mod tests;
pub use dear_imgui_rs as imgui;
#[path = "canvas_pan.rs"]
mod canvas_pan;
pub use canvas_pan::CanvasPan;
#[path = "property_edit.rs"]
mod property_edit;
pub use property_edit::{EditResponse, NumericProperty, UiId};
#[path = "graph_canvas/mod.rs"]
pub mod graph_canvas;
#[path = "node_inspector.rs"]
mod node_inspector;
use fold_platform::{
    desktop::DesktopClient,
    packages::{PackageRegistry, PanelDescriptor},
};
use std::collections::BTreeMap;

pub struct ExtensionUi<'a> {
    pub ui: &'a imgui::Ui,
    pub host: &'a mut dyn DesktopClient,
}
#[derive(Clone, Copy)]
pub struct ViewerRect {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub dimensions: [u32; 2],
}
pub trait Panel {
    /// Optional direct-authoring overlay, restricted to the active document's editor.
    /// Coordinates describe the displayed image; no render/device ownership is exposed.
    fn draw_viewer_overlay(&mut self, _context: ExtensionUi<'_>, _rect: ViewerRect) {}
    /// Called once after ImGui initialization. Panels are dropped before ImGui.
    fn initialize(&mut self, _context: &imgui::Context) {}
    /// Used by workspace navigation to reveal matching editor/inspector tabs.
    fn document_type(&self) -> Option<&'static str> {
        None
    }
    /// Shared surfaces can serve multiple document types without becoming
    /// separate dock tabs for each provider.
    fn supports_document_type(&self, kind: &str) -> bool {
        self.document_type() == Some(kind)
    }
    /// Hit-test empty canvas at a window-space pointer position. The shell
    /// queries only visible panels; objects/pins/wires retain ordinary input.
    fn accepts_background_pan(&self, _position: [f32; 2]) -> bool {
        false
    }
    fn id(&self) -> &'static str;
    fn draw(&mut self, context: ExtensionUi<'_>);
}
pub(crate) struct RegisteredPanel {
    pub descriptor: PanelDescriptor,
    pub panel: Box<dyn Panel>,
    pub key: imgui::WindowKey,
}
#[derive(Default)]
pub struct PanelRegistry {
    declared: BTreeMap<&'static str, (&'static str, PanelDescriptor)>,
    entries: Vec<RegisteredPanel>,
    node_inspector: node_inspector::NodeInspector,
}
impl PanelRegistry {
    pub fn new(packages: &PackageRegistry) -> Self {
        Self {
            declared: packages
                .manifests()
                .flat_map(|m| m.panels.iter().map(move |p| (p.id, (m.id, *p))))
                .collect(),
            entries: vec![],
            node_inspector: Default::default(),
        }
    }
    pub fn register(&mut self, package: &str, panel: impl Panel + 'static) -> Result<(), String> {
        let (owner, descriptor) = self
            .declared
            .get(panel.id())
            .ok_or("panel not declared in package manifest")?;
        if *owner != package
            || self.entries.iter().any(|p| p.descriptor.id == panel.id())
            || self.node_inspector.contains(panel.id())
        {
            return Err("duplicate/foreign panel registration".into());
        }
        let key =
            imgui::WindowKey::new(descriptor.id, descriptor.title).map_err(|e| e.to_string())?;
        self.entries.push(RegisteredPanel {
            descriptor: *descriptor,
            panel: Box::new(panel),
            key,
        });
        Ok(())
    }
    /// Register properties inside the normal Node Inspector, not another window.
    /// The contribution retains its package identity and shares the editor's state.
    pub fn register_node_inspector(
        &mut self,
        package: &str,
        inspector: impl Panel + 'static,
    ) -> Result<(), String> {
        let (owner, descriptor) = self
            .declared
            .get(inspector.id())
            .ok_or("inspector not declared in package manifest")?;
        let kind = inspector
            .document_type()
            .ok_or("node inspector requires a document type")?;
        if *owner != package
            || descriptor.placement != fold_platform::packages::PanelPlacement::Inspector
            || self
                .entries
                .iter()
                .any(|p| p.descriptor.id == inspector.id())
            || self.node_inspector.contains(inspector.id())
            || self.node_inspector.supports_document_type(kind)
        {
            return Err("duplicate, foreign, or invalid node inspector contribution".into());
        }
        self.node_inspector.providers.push(Box::new(inspector));
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<Vec<RegisteredPanel>, String> {
        if self.entries.len() + self.node_inspector.providers.len() != self.declared.len() {
            return Err("declared desktop contributions are missing implementations".into());
        }
        if !self.node_inspector.providers.is_empty() {
            let descriptor = PanelDescriptor {
                id: node_inspector::ID,
                title: "Node Inspector",
                placement: fold_platform::packages::PanelPlacement::Inspector,
            };
            self.entries.push(RegisteredPanel {
                descriptor,
                key: imgui::WindowKey::new(descriptor.id, descriptor.title)
                    .map_err(|e| e.to_string())?,
                panel: Box::new(self.node_inspector),
            });
        }
        Ok(self.entries)
    }
}

/// Shared graph palette. Types and selection also have labels/outlines, so
/// color is never the only indication of socket compatibility or focus.
pub struct GraphColors {
    pub background: [f32; 4],
    pub grid: [f32; 4],
    pub node: [f32; 4],
    pub image: [f32; 4],
    pub mask: [f32; 4],
    pub source: [f32; 4],
    pub merge: [f32; 4],
    pub selected: [f32; 4],
    pub invalid: [f32; 4],
}
pub const GRAPH_COLORS: GraphColors = GraphColors {
    background: [0.075, 0.083, 0.10, 1.0],
    grid: [0.13, 0.145, 0.17, 0.65],
    node: [0.13, 0.145, 0.175, 1.0],
    image: [0.30, 0.74, 0.83, 1.0],
    mask: [0.76, 0.57, 0.94, 1.0],
    source: [0.40, 0.78, 0.55, 1.0],
    merge: [0.88, 0.66, 0.35, 1.0],
    selected: [0.98, 0.78, 0.32, 1.0],
    invalid: [0.95, 0.35, 0.25, 1.0],
};

/// Semantic colors shared by custom editors. Base colors follow the active
/// ImGui style; media-kind and error colors have a single application definition.
pub struct EditorColors {
    pub background: [f32; 4],
    pub lane: [f32; 4],
    pub header: [f32; 4],
    pub grid: [f32; 4],
    pub text: [f32; 4],
    pub muted: [f32; 4],
    pub selected: [f32; 4],
    pub playhead: [f32; 4],
    pub video: [f32; 4],
    pub audio: [f32; 4],
    pub invalid: [f32; 4],
}
impl EditorColors {
    pub fn from_ui(ui: &imgui::Ui) -> Self {
        use imgui::StyleColor as C;
        Self {
            background: ui.style_color(C::WindowBg),
            lane: ui.style_color(C::FrameBg),
            header: ui.style_color(C::TitleBg),
            grid: ui.style_color(C::Border),
            text: ui.style_color(C::Text),
            muted: ui.style_color(C::TextDisabled),
            selected: ui.style_color(C::SliderGrabActive),
            playhead: [0.98, 0.72, 0.24, 1.0],
            video: [0.20, 0.37, 0.56, 1.0],
            audio: [0.20, 0.43, 0.35, 1.0],
            invalid: [0.9, 0.3, 0.25, 1.0],
        }
    }
}
