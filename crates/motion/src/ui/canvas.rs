use super::{graph::Context, state::Shared};
use crate::{authoring as a, geometry::Domain, package};
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{
    ExtensionUi, Panel,
    graph_canvas::GraphCanvas,
    imgui,
    toolbar::{ToolbarIcon, icon_button, menu_button},
};

pub struct Canvas {
    pub state: Shared,
    pub canvas: GraphCanvas,
    pub overlay: super::overlay::Overlay,
}
impl Panel for Canvas {
    fn new_instance(&self) -> Option<fold_ui::sdk::EditorPanels> {
        let state = std::rc::Rc::new(std::cell::RefCell::new(super::state::State::default()));
        Some(fold_ui::sdk::EditorPanels {
            editor: Box::new(Self {
                state: state.clone(),
                canvas: Default::default(),
                overlay: Default::default(),
            }),
            inspector: Box::new(super::inspector::Inspector {
                state,
                curves: Default::default(),
            }),
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
    fn initialize(&mut self, context: &imgui::Context) {
        self.canvas.initialize(context);
    }
    fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.canvas.accepts_background_pan(position)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        self.canvas.clear_hit_map();
        let mut state = self.state.borrow_mut();
        state.sync(host);
        let mut licenses = false;
        if let Some(_menu) = menu_button(ui, "Motion", "Motion document actions") {
            if ui.menu_item("New motion") {
                state.create(host);
            }
            if let Some(source) = state.document {
                if ui.menu_item("Place in sequence")
                    && let Some(snapshot) = host.snapshot()
                {
                    state.cancel(host);
                    host.command(DesktopCommand::Extension(fold_platform::packages::CommandRequest {
                        id: "fold.app.insert-document".into(), base: snapshot.revision(),
                        arguments: serde_json::to_vec(&serde_json::json!({"source":source,"sequence":null,"at":fold_foundation::Time::ZERO})).unwrap(),
                    }));
                    if let Some(snapshot) = host.snapshot()
                        && let Some(sequence) = snapshot
                            .state()
                            .documents
                            .values()
                            .find(|d| d.type_id == "fold.timeline.sequence")
                    {
                        host.command(DesktopCommand::Navigate(
                            fold_platform::desktop::ViewLocation {
                                document: sequence.id,
                                time: fold_foundation::Time::ZERO,
                                label: "Sequence".into(),
                            },
                        ));
                    }
                }
            }
            ui.separator();
            licenses = ui.menu_item("Font licenses...");
        }
        if licenses {
            ui.open_popup("Font licenses");
        }
        if let Some(_modal) = ui
            .begin_modal_popup_config("Font licenses")
            .flags(imgui::WindowFlags::ALWAYS_AUTO_RESIZE)
            .begin()
        {
            let display = ui.io().display_size();
            ui.child_window("license-text")
                .size([(display[0] * 0.75).min(650.), (display[1] * 0.6).min(500.)])
                .build(ui, || ui.text_wrapped(crate::document::FONT_LICENSE));
            if ui.button("Close") {
                ui.close_current_popup();
            }
        }
        if state.motion.is_none() {
            ui.text_wrapped("Choose Motion > New motion to begin.");
            if !state.error.is_empty() {
                ui.text_wrapped(&state.error);
            }
            return;
        }
        // Direct authoring edits the same construction as the graph below.
        ui.same_line();
        if let Some(_menu) = menu_button(ui, "Add", "Add text or a shape") {
            for (name, kind) in [
                ("Text", "fold.motion.text"),
                ("Rectangle", "fold.motion.rectangle"),
                ("Ellipse", "fold.motion.ellipse"),
                ("Path", "fold.motion.path"),
            ] {
                if ui.menu_item(name) {
                    state.change(host, |m| a::add_content(m, kind).map(Some));
                    self.canvas.frame_all();
                }
            }
        }
        let selected = state
            .selected
            .first()
            .copied()
            .filter(|_| state.selected.len() == 1);
        ui.same_line();
        {
            let _disabled = ui.begin_disabled_with_cond(selected.is_none());
            if let Some(_menu) = menu_button(
                ui,
                "Modify",
                "Repeat, animate, or group the selected content",
            ) && let Some(selected) = selected
            {
                if let Some(_menu) = ui.begin_menu("Repeat") {
                    for (name, kind) in [
                        ("Line", "fold.motion.line_points"),
                        ("Grid", "fold.motion.grid_points"),
                        ("Radial", "fold.motion.radial_points"),
                    ] {
                        if ui.menu_item(name) {
                            state.change(host, |m| a::repeat(m, selected, kind).map(Some));
                            self.canvas.frame_all();
                        }
                    }
                }
                if let Some(_menu) = ui.begin_menu("Position wave") {
                    for (name, domain) in [
                        ("Each glyph", Domain::Glyphs),
                        ("Each copy", Domain::Instances),
                        ("Each child", Domain::Children),
                    ] {
                        if ui.menu_item(name) {
                            state.change(host, |m| a::wave_position(m, selected, domain).map(Some));
                            self.canvas.frame_all();
                        }
                    }
                }
                if ui.menu_item("Add radial falloff") {
                    state.change(host, |m| a::radial_influence(m, selected).map(Some));
                    self.canvas.frame_all();
                }
                ui.separator();
                if ui.menu_item("Create reusable group") {
                    state.change(host, |m| a::group_node(m, selected).map(Some));
                    self.canvas.frame_all();
                }
            }
        }
        ui.same_line();
        if icon_button(
            ui,
            "view-motion",
            ToolbarIcon::View,
            "Show motion output in viewer",
        ) {
            state.cancel(host);
            state.view(host);
        }
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
