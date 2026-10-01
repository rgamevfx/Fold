use super::{graph::Context, state::Shared};
use crate::{authoring as a, geometry::Domain, package};
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::GraphCanvas, imgui};

pub struct Canvas {
    pub state: Shared,
    pub canvas: GraphCanvas,
    pub overlay: super::overlay::Overlay,
}
impl Panel for Canvas {
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
        if ui.button("New motion") {
            state.create(host);
        }
        ui.same_line();
        if ui.button("View output") {
            state.cancel(host);
            state.view(host);
        }
        if let Some(source) = state.document {
            if ui.button("Place in sequence")
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
        if state.motion.is_none() {
            ui.text_wrapped("Create a motion document, add content, then add distribution and drivers. All tools create editable graph nodes.");
            return;
        }
        // Direct authoring is motion-specific. Node search, grouping navigation,
        // connections, selection and gestures below use the same editor as compositing.
        if let Some(_menu) = ui.begin_combo("Add content", "Text / shapes") {
            for (name, kind) in [
                ("Text", "fold.motion.text"),
                ("Rectangle", "fold.motion.rectangle"),
                ("Ellipse", "fold.motion.ellipse"),
                ("Path", "fold.motion.path"),
            ] {
                if ui.selectable(name) {
                    state.change(host, |m| a::add_content(m, kind).map(Some));
                    self.canvas.frame_all();
                }
            }
        }
        if let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) {
            if ui.button("Make reusable group") {
                state.change(host, |m| a::group_node(m, selected).map(Some));
                self.canvas.frame_all();
            }
            if let Some(_menu) = ui.begin_combo("Repeat selected", "Distribution") {
                for (name, kind) in [
                    ("Line", "fold.motion.line_points"),
                    ("Grid", "fold.motion.grid_points"),
                    ("Radial", "fold.motion.radial_points"),
                ] {
                    if ui.selectable(name) {
                        state.change(host, |m| a::repeat(m, selected, kind).map(Some));
                        self.canvas.frame_all();
                    }
                }
            }
            ui.same_line();
            if let Some(_menu) = ui.begin_combo("Add position wave", "Scope") {
                for (name, domain) in [
                    ("Each glyph", Domain::Glyphs),
                    ("Each copy", Domain::Instances),
                    ("Children", Domain::Children),
                ] {
                    if ui.selectable(name) {
                        state.change(host, |m| a::wave_position(m, selected, domain).map(Some));
                        self.canvas.frame_all();
                    }
                }
            }
            if ui.button("Add radial influence to selected response") {
                state.change(host, |m| a::radial_influence(m, selected).map(Some));
                self.canvas.frame_all();
            }
        }
        self.canvas.draw(
            ui,
            &mut Context {
                state: &mut state,
                host,
            },
        );
    }
}
