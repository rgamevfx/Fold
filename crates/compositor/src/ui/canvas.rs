use super::{graph::Context, state::Shared};
use crate::package;
use fold_foundation::Time;
use fold_platform::{
    desktop::{DesktopCommand, ViewLocation},
    packages::CommandRequest,
};
use fold_ui::sdk::{
    ExtensionUi, Panel,
    graph_canvas::GraphCanvas,
    imgui,
    toolbar::{ToolbarIcon, icon_button, menu_button},
};

pub(super) struct Canvas {
    state: Shared,
    canvas: GraphCanvas,
}
impl Canvas {
    pub fn new(state: Shared) -> Self {
        Self {
            state,
            canvas: GraphCanvas::default(),
        }
    }
}
impl Panel for Canvas {
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITE)
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
        if let Some(_menu) = menu_button(ui, "Composite", "Create or open a composite") {
            if ui.menu_item("New composite") {
                state.create(host);
            }
            if ui.menu_item_enabled_selected_no_shortcut(
                "Create from selected clips",
                false,
                host.state().selection.document.is_some()
                    && !host.state().selection.objects.is_empty(),
            ) && let Some(snapshot) = host.snapshot()
            {
                let selection = &host.state().selection;
                if let Some(document) = selection.document {
                    host.command(DesktopCommand::Extension(CommandRequest {
                        id: "fold.app.create-composition".into(),
                        base: snapshot.revision(),
                        arguments: serde_json::to_vec(
                            &serde_json::json!({"document": document, "clips": selection.objects}),
                        )
                        .unwrap(),
                    }));
                    // Keep the parent playhead in the host's navigation breadcrumb.
                    if let Some(now) = host.snapshot()
                        && let Some(created) = now.state().documents.values().find(|d| {
                            d.type_id == crate::COMPOSITE
                                && !snapshot.state().documents.contains_key(&d.id)
                        })
                    {
                        host.command(DesktopCommand::Navigate(ViewLocation {
                            document: created.id,
                            time: Time::ZERO,
                            label: "Composite".into(),
                        }));
                    }
                    state.sync(host);
                }
            }
            if let Some(snapshot) = host.snapshot() {
                ui.separator();
                for (index, document) in snapshot
                    .state()
                    .documents
                    .values()
                    .filter(|d| d.type_id == crate::COMPOSITE)
                    .enumerate()
                {
                    let _id = ui.push_id(&format!("{:?}", document.id));
                    let label = document
                        .extensions
                        .get("name")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Composite {}", index + 1));
                    if ui.menu_item_enabled_selected_no_shortcut(
                        &label,
                        state.document == Some(document.id),
                        true,
                    ) {
                        host.command(DesktopCommand::Navigate(ViewLocation {
                            document: document.id,
                            time: Time::ZERO,
                            label,
                        }));
                        state.sync(host);
                    }
                }
            }
        }
        ui.same_line();
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
            ui.text_wrapped(
                "Choose Composite > New composite, or create one from selected timeline clips.",
            );
            if !state.error.is_empty() {
                ui.text_wrapped(&state.error);
            }
            return;
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
