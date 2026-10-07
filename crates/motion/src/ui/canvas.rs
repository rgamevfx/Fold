use super::{graph::Context, state::Shared};
use crate::package;
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::GraphCanvas, imgui};

pub struct Canvas {
    pub state: Shared,
    pub(super) files: fold_ui::sdk::project_drop::FileDrop,
    pub canvas: GraphCanvas,
    pub overlay: super::overlay::Overlay,
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
        let Some(request) = self.files.placement(position) else {
            return false;
        };
        if request
            .graph
            .as_ref()
            .is_some_and(|g| g.group.is_none() && g.object.is_none())
        {
            self.state.borrow_mut().document_network = true;
        }
        fold_ui::sdk::project_drop::import_and_place(host, paths, request);
        true
    }

    fn take_network_request(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().network_requested)
    }

    fn supports_network(&self) -> bool {
        true
    }
    fn is_layer_graph(&self) -> bool {
        true
    }

    fn selected_layer_object(&self) -> Option<fold_foundation::ObjectId> {
        let state = self.state.borrow();
        state
            .motion
            .as_ref()
            .and_then(|m| m.scene.as_ref())
            .and_then(|s| {
                s.objects
                    .iter()
                    .find(|o| state.selected.first().is_some_and(|id| o.owns(*id)))
                    .map(|o| o.id)
            })
            .or(state.network_object)
    }
    fn new_instance(&self) -> Option<fold_ui::sdk::EditorPanels> {
        let state = std::rc::Rc::new(std::cell::RefCell::new(super::state::State::default()));
        Some(fold_ui::sdk::EditorPanels {
            editor: Box::new(Self {
                state: state.clone(),
                files: Default::default(),
                canvas: Default::default(),
                overlay: Default::default(),
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
    fn viewer_tool_insets(&self, ui: &imgui::Ui) -> [f32; 2] {
        super::tools::insets(ui)
    }
    fn draw_viewer_overlay(&mut self, context: ExtensionUi<'_>, rect: fold_ui::sdk::ViewerRect) {
        let ExtensionUi { ui, host } = context;
        self.overlay
            .draw(&self.state, ExtensionUi { ui, host }, rect);
        if rect.editable
            && let Some(document) = self.state.borrow().document
        {
            let time = host
                .state()
                .navigation
                .last()
                .map(|n| n.time)
                .unwrap_or(fold_foundation::Time::ZERO);
            let request = fold_ui::sdk::project_drop::placement(document, time);
            self.files.region(
                ui,
                rect.canvas_origin,
                rect.canvas_size,
                request.clone(),
                None,
            );
            if let Some(entry) =
                fold_ui::sdk::project_drop::canvas_target(ui, rect.canvas_origin, rect.canvas_size)
            {
                fold_ui::sdk::project_drop::place_request(host, entry, request);
            }
        }
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
        self.draw_network(context);
    }

    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        self.canvas.clear_hit_map();
        let mut state = self.state.borrow_mut();
        state.sync(host);
        if state.motion.is_none() {
            ui.text_disabled("Add a MoGraph layer in Composition.");
            return;
        }
        if state.group.is_none() {
            let owner = state
                .selected
                .first()
                .and_then(|id| {
                    state
                        .motion
                        .as_ref()?
                        .scene
                        .as_ref()?
                        .objects
                        .iter()
                        .find(|o| o.owns(*id))
                })
                .map(|o| o.id);
            if owner.is_some() && owner != state.network_object {
                state.network_object = owner;
            }
        }
        if state.group.is_none() {
            if let Some(_menu) = fold_ui::sdk::toolbar::menu_button(
                ui,
                "Tools",
                "Create, arrange, animate and style layers",
            ) {
                super::tool_actions::draw(ui, &mut state, host);
            }
            ui.same_line();
        }
        if let Some(_menu) =
            fold_ui::sdk::toolbar::menu_button(ui, "Network", "Procedural graph actions")
        {
            ui.checkbox("Show parameter sockets", &mut state.parameter_sockets);
            if ui.menu_item(if state.document_network {
                "Selected graph"
            } else {
                "Full document network"
            }) {
                state.document_network = !state.document_network;
                state.generation += 1;
            }
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
        super::group_interface::draw(ui, &mut state);
        if state.group.is_some() {
            ui.same_line();
        }
        let label = state
            .group
            .and_then(|id| state.motion.as_ref()?.groups.get(&id))
            .map(|g| g.name.as_str())
            .or_else(|| {
                if state.document_network {
                    return Some("Document network");
                }
                state
                    .motion
                    .as_ref()?
                    .scene
                    .as_ref()?
                    .objects
                    .iter()
                    .find(|o| Some(o.id) == state.network_object)
                    .map(|o| o.name.as_str())
            })
            .unwrap_or(if state.document_network {
                "Document network"
            } else {
                "Select a graph"
            });
        let label = format!("MoGraph / {label}");
        if ui.content_region_avail()[0] >= ui.calc_text_size(&label)[0] {
            ui.text_disabled(&label);
        } else {
            fold_ui::sdk::toolbar::tooltip(ui, &label);
            ui.new_line();
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
                group: state.group,
                object: if state.document_network {
                    None
                } else {
                    state.network_object
                },
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
                if state.group.is_none() && state.network_object.is_none() {
                    state.document_network = true;
                }
                fold_ui::sdk::project_drop::place_request(host, entry, request);
            }
        }
    }
}
