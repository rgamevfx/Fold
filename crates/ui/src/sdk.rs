//! Small trusted, in-process panel SDK. Panels own presentation state only.
//! Raw ImGui is an explicitly unstable escape hatch for custom editors,
//! not a promised native plugin ABI.
#[cfg(test)]
#[path = "sdk_tests.rs"]
mod tests;
pub use dear_imgui_rs as imgui;
pub use dear_implot as implot;
// Unstable backend access for native UI capture tests in feature packages.
#[doc(hidden)]
pub use dear_imgui_wgpu as render_backend;
#[path = "appearance.rs"]
pub mod appearance;
#[path = "canvas_pan.rs"]
mod canvas_pan;
#[path = "color_controls.rs"]
pub mod color_controls;
#[path = "typography.rs"]
pub mod typography;
pub use canvas_pan::CanvasPan;
#[path = "animated_property.rs"]
pub mod animated_property;
#[path = "animation_editor/mod.rs"]
pub mod animation_editor;
#[path = "cache_indicator.rs"]
pub mod cache_indicator;
#[path = "composition.rs"]
pub(crate) mod composition;
#[path = "media_thumbnails.rs"]
pub mod media_thumbnails;
#[path = "network.rs"]
pub mod network;
#[path = "outliner.rs"]
pub mod outliner;
#[path = "project_drop.rs"]
pub mod project_drop;
#[path = "property_edit.rs"]
mod property_edit;
#[path = "time_view.rs"]
pub mod time_view;
#[path = "toolbar.rs"]
pub mod toolbar;
pub use property_edit::{EditResponse, NumericProperty, UiId};
#[path = "property_rows.rs"]
pub mod property_rows;
#[path = "text_draft.rs"]
mod text_draft;
pub use text_draft::TextDraft;
#[path = "graph_canvas/mod.rs"]
pub mod graph_canvas;
#[path = "inspector.rs"]
mod inspector;
use fold_platform::{
    desktop::DesktopClient,
    packages::{PackageRegistry, PanelDescriptor},
};
use std::collections::BTreeMap;

pub struct ExtensionUi<'a> {
    pub ui: &'a imgui::Ui,
    pub host: &'a mut dyn DesktopClient,
}
impl ExtensionUi<'_> {
    pub fn instance(&self) -> Option<fold_platform::workspace::PanelInstanceId> {
        self.host.panel_instance()
    }
}
#[derive(Clone, Copy)]
pub struct ViewerRect {
    /// Editing is suspended during playback; tool chrome remains visible.
    pub editable: bool,
    /// Artwork corresponds to the current source/time/revision.
    pub image_current: bool,
    /// Fixed viewer canvas, independent of image pan/zoom.
    pub canvas_origin: [f32; 2],
    pub canvas_size: [f32; 2],
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub dimensions: [u32; 2],
}
/// Provider-created UI bundle: independent gesture state with a matching inspector.
pub struct EditorPanels {
    pub editor: Box<dyn Panel>,
    pub inspector: Box<dyn Panel>,
}
/// A provider resolves a selection to editable content. The shell owns linking;
/// observing this target must never issue navigation or selection commands.
pub struct SelectionTarget {
    pub location: fold_platform::desktop::ViewLocation,
    pub objects: Vec<fold_foundation::ObjectId>,
    /// Scene-object rows inspect their content; image layers inspect placement.
    pub inspect_content: bool,
}
/// A source document inspected in its selected placement's time and lock context.
pub struct SourceControls {
    pub document: fold_foundation::DocumentId,
    pub time: fold_foundation::Time,
    pub editable: bool,
}
pub trait Panel {
    /// Shared inspector invokes this after the container's properties. None
    /// releases any previous source gesture when selection or navigation changes.
    fn draw_source_controls(&mut self, _context: ExtensionUi<'_>, _source: Option<SourceControls>) {
    }

    fn selection_target(
        &mut self,
        _host: &mut dyn DesktopClient,
    ) -> Result<Option<SelectionTarget>, String> {
        Ok(None)
    }
    /// Consume an explicit artist request to reveal the shared Network tab.
    fn take_network_request(&mut self) -> bool {
        false
    }
    fn supports_network(&self) -> bool {
        false
    }
    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        context
            .ui
            .text_disabled("Select a procedural graph to edit its network.");
    }

    /// Layer/timing authoring is hosted by the shared Composition surface.
    /// `draw` receives this instance's selection and explicit edit-time context.
    fn supports_composition(&self) -> bool {
        false
    }

    /// Provider-owned graph edited inside a composition layer. No standalone window.
    /// Native graph selection expressed as its owning layer object, if any.
    fn selection_owner(
        &mut self,
        _host: &mut dyn DesktopClient,
    ) -> Option<fold_foundation::ObjectId> {
        None
    }
    /// Resolve a graph selection back to the common hierarchy and inherited lock.
    fn layer_for_graph(
        &self,
        _document: fold_foundation::DocumentId,
        _object: Option<fold_foundation::ObjectId>,
        _occurrence: Option<fold_foundation::ObjectId>,
    ) -> Option<(fold_foundation::ObjectId, bool)> {
        None
    }
    fn is_layer_graph(&self) -> bool {
        false
    }

    /// Reuse this contribution's implementation with fresh instance-local state.
    fn new_instance(&self) -> Option<EditorPanels> {
        None
    }
    /// Release this instance's transient authoring gesture before closure.
    fn cancel_interaction(&mut self, _host: &mut dyn DesktopClient) {}
    /// Optional direct-authoring overlay, restricted to the active document's editor.
    /// Coordinates describe the displayed image; no render/device ownership is exposed.
    fn draw_viewer_overlay(&mut self, _context: ExtensionUi<'_>, _rect: ViewerRect) {}
    /// Space reserved for a provider's left shelf and top tool options.
    fn viewer_tool_insets(&self, _ui: &imgui::Ui) -> [f32; 2] {
        [0.; 2]
    }
    /// Called once after ImGui initialization. Panels are dropped before ImGui.
    /// The desktop supplies shared fonts; embedded/test hosts may retain their own.
    fn initialize(
        &mut self,
        _context: &imgui::Context,
        _typography: Option<&typography::Typography>,
    ) {
    }
    /// CPU thumbnail publication before NewFrame; renderer/device remain private.
    fn prepare_frame(&mut self, _context: &mut imgui::Context) {}
    fn external_drag(&mut self, _position: Option<[f32; 2]>) {}
    fn files_dropped(
        &mut self,
        _position: [f32; 2],
        _paths: &[std::path::PathBuf],
        _host: &mut dyn DesktopClient,
    ) -> bool {
        false
    }
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
    inspector: inspector::Inspector,
}
impl PanelRegistry {
    /// Instantiate the registered editor for native integration probes.
    #[cfg(feature = "native-probe")]
    pub fn probe_editor(&self, kind: &str) -> Option<EditorPanels> {
        self.entries
            .iter()
            .find(|p| p.panel.document_type() == Some(kind))?
            .panel
            .new_instance()
    }
    pub fn new(packages: &PackageRegistry) -> Self {
        Self {
            declared: packages
                .manifests()
                .flat_map(|m| m.panels.iter().map(move |p| (p.id, (m.id, *p))))
                .collect(),
            entries: vec![],
            inspector: Default::default(),
        }
    }
    pub fn register(&mut self, package: &str, panel: impl Panel + 'static) -> Result<(), String> {
        let (owner, descriptor) = self
            .declared
            .get(panel.id())
            .ok_or("panel not declared in package manifest")?;
        if *owner != package
            || self.entries.iter().any(|p| p.descriptor.id == panel.id())
            || self.inspector.contains(panel.id())
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
    /// Register properties inside the shared Inspector, not another window.
    /// The contribution retains its package identity and shares the editor's state.
    pub fn register_inspector(
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
            .ok_or("inspector requires a document type")?;
        if *owner != package
            || descriptor.placement != fold_platform::packages::PanelPlacement::Inspector
            || self
                .entries
                .iter()
                .any(|p| p.descriptor.id == inspector.id())
            || self.inspector.contains(inspector.id())
            || self.inspector.supports_document_type(kind)
        {
            return Err("duplicate, foreign, or invalid inspector contribution".into());
        }
        self.inspector.providers.push(Box::new(inspector));
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<Vec<RegisteredPanel>, String> {
        if self.entries.len() + self.inspector.providers.len() != self.declared.len() {
            return Err("declared desktop contributions are missing implementations".into());
        }
        if !self.inspector.providers.is_empty() {
            let descriptor = PanelDescriptor {
                id: inspector::ID,
                title: "Inspector",
                placement: fold_platform::packages::PanelPlacement::Inspector,
            };
            self.entries.push(RegisteredPanel {
                descriptor,
                key: imgui::WindowKey::new(descriptor.id, descriptor.title)
                    .map_err(|e| e.to_string())?,
                panel: Box::new(self.inspector),
            });
        }
        if self.entries.iter().any(|p| p.panel.supports_network()) {
            let descriptor = PanelDescriptor {
                id: outliner::PANEL_ID,
                title: "Outliner",
                placement: fold_platform::packages::PanelPlacement::Browser,
            };
            self.entries.push(RegisteredPanel {
                descriptor,
                key: imgui::WindowKey::new(descriptor.id, descriptor.title)
                    .map_err(|e| e.to_string())?,
                panel: Box::new(outliner::Surface::default()),
            });
        }
        if self.entries.iter().any(|p| p.panel.supports_network()) {
            let descriptor = PanelDescriptor {
                id: network::PANEL_ID,
                title: "Network",
                placement: fold_platform::packages::PanelPlacement::Editor,
            };
            self.entries.push(RegisteredPanel {
                descriptor,
                key: imgui::WindowKey::new(descriptor.id, descriptor.title)
                    .map_err(|e| e.to_string())?,
                panel: Box::new(network::Surface),
            });
        }
        if self.entries.iter().any(|p| p.panel.supports_composition()) {
            let descriptor = PanelDescriptor {
                id: composition::PANEL_ID,
                title: "Composition",
                placement: fold_platform::packages::PanelPlacement::Editor,
            };
            self.entries.push(RegisteredPanel {
                descriptor,
                key: imgui::WindowKey::new(descriptor.id, descriptor.title)
                    .map_err(|e| e.to_string())?,
                panel: Box::new(composition::Surface),
            });
        }
        Ok(self.entries)
    }
}

/// Shared graph accents. Surfaces and text follow the active UI theme.
/// Socket labels and selection outlines supplement color.
pub struct GraphColors {
    pub image: [f32; 4],
    pub mask: [f32; 4],
    pub source: [f32; 4],
    pub merge: [f32; 4],
    pub selected: [f32; 4],
    pub invalid: [f32; 4],
}
pub const GRAPH_COLORS: GraphColors = GraphColors {
    image: [0.30, 0.74, 0.83, 1.0],
    mask: [0.76, 0.57, 0.94, 1.0],
    source: [0.40, 0.78, 0.55, 1.0],
    merge: [0.88, 0.66, 0.35, 1.0],
    selected: [0.98, 0.78, 0.32, 1.0],
    invalid: [0.95, 0.35, 0.25, 1.0],
};

/// Subdued channel accents; component letters remain visible alongside color.
pub const CHANNEL_COLORS: [[f32; 4]; 4] = [
    [0.78, 0.51, 0.51, 1.],
    [0.53, 0.70, 0.58, 1.],
    [0.54, 0.62, 0.78, 1.],
    [0.70, 0.70, 0.73, 1.],
];

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

#[path = "effects.rs"]
pub mod effects;

#[path = "normalized_curve.rs"]
pub mod normalized_curve;

#[path = "parameter_menu.rs"]
pub mod parameter_menu;
