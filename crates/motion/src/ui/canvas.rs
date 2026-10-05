use super::{graph::Context, state::Shared};
use crate::package;
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::GraphCanvas, imgui};

pub struct Canvas {
    pub state: Shared,
    pub canvas: GraphCanvas,
    pub overlay: super::overlay::Overlay,
    pub(super) animation: super::animation::Animation,
    pub(super) network_animation: super::animation::Animation,
}
impl Panel for Canvas {
    fn take_network_request(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().network_requested)
    }

    fn supports_network(&self) -> bool {
        true
    }
    fn supports_animation(&self) -> bool {
        true
    }
    fn draw_animation(&mut self, context: ExtensionUi<'_>) {
        let scoped = self.state.borrow().group.is_some();
        if scoped {
            self.network_animation.draw_scoped(&self.state, context);
        } else {
            context.ui.text_disabled("Motion animation is integrated into MoGraph. Open a modifier graph to edit its internal animation here.");
        }
    }

    fn new_instance(&self) -> Option<fold_ui::sdk::EditorPanels> {
        let state = std::rc::Rc::new(std::cell::RefCell::new(super::state::State::default()));
        Some(fold_ui::sdk::EditorPanels {
            editor: Box::new(Self {
                state: state.clone(),
                canvas: Default::default(),
                overlay: Default::default(),
                animation: Default::default(),
                network_animation: Default::default(),
            }),
            inspector: Box::new(super::inspector::Inspector { state }),
        })
    }
    fn cancel_interaction(&mut self, host: &mut dyn fold_platform::desktop::DesktopClient) {
        self.overlay.cancel();
        let mut state = self.state.borrow_mut();
        if state.editing {
            state.cancel(host);
        }
    }
    fn draw_viewer_overlay(&mut self, context: ExtensionUi<'_>, rect: fold_ui::sdk::ViewerRect) {
        self.overlay.draw(&self.state, context, rect);
    }
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::MOTION)
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
    fn draw(&mut self, context: ExtensionUi<'_>) {
        super::scene::draw(&self.state, &mut self.animation, context);
    }
    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        self.canvas.clear_hit_map();
        let mut state = self.state.borrow_mut();
        state.sync(host);
        if state.motion.is_none() {
            ui.text_disabled("Create a Motion document in MoGraph or Project.");
            return;
        }
        if let Some(_menu) =
            fold_ui::sdk::toolbar::menu_button(ui, "Network", "Procedural graph actions")
        {
            if let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) {
                if ui.menu_item("Create reusable group from node") {
                    state.change(host, |m| {
                        crate::authoring::group_node(m, selected).map(Some)
                    });
                }
            } else {
                ui.text_disabled("Select a node to create a reusable function.");
            }
        }
        ui.same_line();
        super::group_interface::draw(ui, host, &mut state);
        if state.group.is_some() {
            ui.same_line();
        }
        let label = state
            .group
            .and_then(|id| state.motion.as_ref()?.groups.get(&id))
            .map(|g| g.name.as_str())
            .unwrap_or("Document network");
        ui.text(format!("MoGraph / {label}"));
        let drop_origin = ui.cursor_screen_pos();
        let drop_size = ui.content_region_avail();
        self.canvas.draw_inline(
            ui,
            &mut Context {
                state: &mut state,
                host,
            },
        );
        if let Some(entry) = fold_ui::sdk::project_drop::canvas_target(ui, drop_origin, drop_size)
            && let Some(document) = state.document
        {
            fold_ui::sdk::project_drop::place(
                host,
                entry,
                document,
                None,
                fold_foundation::Time::ZERO,
            );
        }
    }
}
