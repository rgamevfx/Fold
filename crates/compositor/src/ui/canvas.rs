use super::{graph::Context, state::Shared};
use crate::package;
use fold_ui::sdk::{
    ExtensionUi, Panel,
    graph_canvas::GraphCanvas,
    imgui,
    toolbar::{ToolbarIcon, icon_button},
};

pub(super) struct Canvas {
    state: Shared,
    files: fold_ui::sdk::project_drop::FileDrop,
    canvas: GraphCanvas,
    animation: fold_ui::sdk::animation_editor::Editor,
}
impl Canvas {
    pub fn new(state: Shared) -> Self {
        Self {
            state,
            files: Default::default(),
            canvas: GraphCanvas::default(),
            animation: Default::default(),
        }
    }
}
impl Panel for Canvas {
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.files.begin_frame(position);
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn fold_platform::desktop::DesktopClient,
    ) -> bool {
        self.files.dropped(position, paths, host)
    }

    fn supports_network(&self) -> bool {
        true
    }
    fn is_layer_graph(&self) -> bool {
        true
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        super::animation::draw(&mut self.animation, &self.state, context);
    }

    fn new_instance(&self) -> Option<fold_ui::sdk::EditorPanels> {
        let state = std::rc::Rc::new(std::cell::RefCell::new(super::state::State::default()));
        Some(fold_ui::sdk::EditorPanels {
            editor: Box::new(Self::new(state.clone())),
            inspector: Box::new(super::inspector::Inspector(state, Default::default())),
        })
    }
    fn cancel_interaction(&mut self, host: &mut dyn fold_platform::desktop::DesktopClient) {
        let mut state = self.state.borrow_mut();
        if state.editing {
            state.cancel(host);
        }
    }
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITE)
    }
    fn initialize(
        &mut self,
        context: &imgui::Context,
        typography: Option<&fold_ui::sdk::typography::Typography>,
    ) {
        self.canvas.initialize(context, typography);
    }
    fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.canvas.accepts_background_pan(position)
    }
    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        let animation = {
            let mut state = self.state.borrow_mut();
            context.ui.checkbox("Animation", &mut state.graph_animation);
            state.graph_animation
        };
        if animation {
            self.draw(context);
            return;
        }
        let ExtensionUi { ui, host } = context;
        self.canvas.clear_hit_map();
        let mut state = self.state.borrow_mut();
        state.sync(host);
        {
            let _disabled = ui.begin_disabled_with_cond(state.graph.is_none());
            if icon_button(
                ui,
                "view-composite",
                ToolbarIcon::View,
                "Show composite output in viewer",
            ) {
                state.cancel(host);
                state.view(host);
            }
        }
        if state.graph.is_none() {
            ui.text_wrapped("Add a Compositing layer in Composition.");
            if !state.error.is_empty() {
                ui.text_wrapped(&state.error);
            }
            return;
        }
        self.canvas.draw_inline(
            ui,
            &mut Context {
                state: &mut state,
                host,
            },
        );
        let Some([drop_origin, drop_end]) = self.canvas.drop_bounds() else {
            return;
        };
        let drop_size = [drop_end[0] - drop_origin[0], drop_end[1] - drop_origin[1]];
        if let Some(document) = state.document {
            let mut request =
                fold_ui::sdk::project_drop::placement(document, fold_foundation::Time::ZERO);
            request.graph = Some(fold_platform::browser::GraphPlacement {
                position: self.canvas.drop_position(ui.io().mouse_pos()),
                group: None,
                object: None,
            });
            self.files.region(
                ui,
                drop_origin,
                drop_size,
                request.clone(),
                Some(self.canvas.drop_transform()),
            );
            if let Some(entry) =
                fold_ui::sdk::project_drop::canvas_target(ui, drop_origin, drop_size)
            {
                fold_ui::sdk::project_drop::place_request(host, entry, request);
            }
        }
    }
}
