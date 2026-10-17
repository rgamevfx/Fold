//! Native canvas tools and artist controls over the same guarded document edits
//! used by agent authoring. Panels retain only selection, drafts and gestures.
mod controls;
mod design_input;
mod gradient_input;
mod graph;
mod image_control;
mod inspector;
mod layers;
mod overlay;
mod paint;
mod path_input;
mod picking;
mod state;
#[cfg(test)]
mod tests;
mod viewport;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{self, ExtensionUi, Panel, PanelRegistry};
use std::{cell::RefCell, rc::Rc};

type Shared = Rc<RefCell<state::State>>;

pub fn panels() -> sdk::EditorPanels {
    let state = Shared::default();
    sdk::EditorPanels {
        editor: Box::new(Editor {
            state: state.clone(),
            viewport: Default::default(),
            animation: Default::default(),
            graph: Default::default(),
            files: Default::default(),
        }),
        inspector: Box::new(inspector::Inspector {
            state,
            sources: Default::default(),
            names: Default::default(),
            motions: Default::default(),
            embedded: Default::default(),
        }),
    }
}
pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = Shared::default();
    registry.register(
        fold_motion_canvas::PACKAGE,
        Editor {
            state: state.clone(),
            viewport: Default::default(),
            animation: Default::default(),
            graph: Default::default(),
            files: Default::default(),
        },
    )?;
    registry.register_inspector(
        fold_motion_canvas::PACKAGE,
        inspector::Inspector {
            state,
            sources: Default::default(),
            names: Default::default(),
            motions: Default::default(),
            embedded: Default::default(),
        },
    )
}

struct Editor {
    state: Shared,
    viewport: viewport::Viewport,
    animation: sdk::animation_editor::Editor,
    graph: sdk::graph_canvas::GraphCanvas,
    files: sdk::project_drop::FileDrop,
}
impl Panel for Editor {
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.files.begin_frame(position);
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) -> bool {
        self.files.dropped(position, paths, host)
    }

    fn supports_network(&self) -> bool {
        true
    }
    fn initialize(
        &mut self,
        context: &sdk::imgui::Context,
        typography: Option<&sdk::typography::Typography>,
    ) {
        self.graph.initialize(context, typography);
        self.viewport.initialize(context);
    }
    fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.graph.accepts_background_pan(position)
    }
    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.state.borrow_mut();
        state.sync(host);
        if state.canvas().is_none() {
            ui.text_disabled("Select a Motion document.");
            return;
        }
        self.graph.draw(
            ui,
            &mut graph::Context {
                state: &mut state,
                host,
            },
        );
        if !state.busy()
            && !state.editing
            && let Some(document) = state.document
            && let Some([origin, end]) = self.graph.drop_bounds()
        {
            let size = [end[0] - origin[0], end[1] - origin[1]];
            let mut request = sdk::project_drop::placement(document, state.time);
            request.graph = Some(fold_platform::browser::GraphPlacement {
                position: self.graph.drop_position(ui.io().mouse_pos()),
                group: state.graph_group,
                object: None,
            });
            self.files.region(
                ui,
                origin,
                size,
                request.clone(),
                Some(self.graph.drop_transform()),
            );
            if let Some(entry) = sdk::project_drop::canvas_target(ui, origin, size) {
                sdk::project_drop::place_request(host, entry, request);
            }
        }
    }
    fn id(&self) -> &'static str {
        crate::package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(fold_motion_canvas::TYPE)
    }
    fn new_instance(&self) -> Option<sdk::EditorPanels> {
        Some(panels())
    }
    fn cancel_interaction(&mut self, host: &mut dyn DesktopClient) {
        self.state.borrow_mut().cancel(host);
        self.viewport.cancel();
    }
    fn viewer_tool_insets(&self, ui: &sdk::imgui::Ui) -> [f32; 2] {
        [0., ui.frame_height() + ui.clone_style().item_spacing()[1]]
    }
    fn draw_viewer_overlay(&mut self, context: ExtensionUi<'_>, rect: sdk::ViewerRect) {
        let mut state = self.state.borrow_mut();
        state.sync(context.host);
        let ExtensionUi { ui, host } = context;
        self.viewport
            .draw(ExtensionUi { ui, host }, rect, &mut state);
        if rect.editable
            && !state.busy()
            && !state.editing
            && let Some(document) = state.document
        {
            let mut request = sdk::project_drop::placement(document, state.time);
            let dimensions = state
                .canvas()
                .map(|canvas| [canvas.width, canvas.height])
                .unwrap();
            let view = overlay::View { rect, dimensions };
            request.canvas_position = Some(view.image(ui.io().mouse_pos()));
            let scale = [0, 1].map(|axis| dimensions[axis] as f32 / rect.size[axis]);
            let origin = [0, 1].map(|axis| -rect.origin[axis] * scale[axis]);
            self.files.region(
                ui,
                rect.origin,
                rect.size,
                request.clone(),
                Some([origin, scale]),
            );
            if let Some(entry) = sdk::project_drop::canvas_target(ui, rect.origin, rect.size) {
                sdk::project_drop::place_request(host, entry, request);
            }
        }
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let mut state = self.state.borrow_mut();
        state.sync(context.host);
        layers::draw(context, &mut state, &mut self.animation);
    }
}
