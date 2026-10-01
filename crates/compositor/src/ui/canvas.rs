use super::{graph::Context, state::Shared};
use crate::package;
use fold_foundation::Time;
use fold_platform::{
    desktop::{DesktopCommand, ViewLocation},
    packages::CommandRequest,
};
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::GraphCanvas, imgui};

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
        if ui.button("New composite") {
            state.create(host);
        }
        ui.same_line();
        if ui.button("Create from selection")
            && let Some(snapshot) = host.snapshot()
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
        ui.same_line();
        if ui.button("View output") {
            state.cancel(host);
            state.view(host);
        }
        if let Some(snapshot) = host.snapshot()
            && let Some(_menu) = ui.begin_combo(
                "Document",
                state
                    .document
                    .map(|id| format!("Composite {id:?}"))
                    .unwrap_or_else(|| "Choose…".into()),
            )
        {
            for document in snapshot
                .state()
                .documents
                .values()
                .filter(|d| d.type_id == crate::COMPOSITE)
            {
                if ui.selectable(format!("Composite {:?}", document.id)) {
                    host.command(DesktopCommand::Navigate(ViewLocation {
                        document: document.id,
                        time: Time::ZERO,
                        label: "Composite".into(),
                    }));
                    state.sync(host);
                }
            }
        }
        if state.graph.is_none() {
            ui.text_wrapped("Create a composite, or select timeline footage and choose Create from selection. Linked audio stays in the sequence.");
            ui.text_wrapped(&host.state().status);
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
