//! Artist adapters over the native 3D document and its guarded transactions.
mod inspector;
mod state;
#[cfg(test)]
mod tests;
mod viewport;
use crate::{
    model::{Content, Scene},
    package,
};
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_ui::sdk::{self, ExtensionUi, Panel, PanelRegistry};
use std::{cell::RefCell, rc::Rc};
type Shared = Rc<RefCell<state::State>>;

pub fn panels() -> sdk::EditorPanels {
    let state = Shared::default();
    sdk::EditorPanels {
        editor: Box::new(Editor::new(state.clone())),
        inspector: Box::new(inspector::Inspector::new(state)),
    }
}
pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = Shared::default();
    registry.register(crate::PACKAGE, Editor::new(state.clone()))?;
    registry.register_inspector(crate::PACKAGE, inspector::Inspector::new(state))
}
struct Editor {
    state: Shared,
    viewport: viewport::Viewport,
    rows: Vec<(ObjectId, usize)>,
    identity: Option<(fold_foundation::DocumentId, fold_project::Revision)>,
}
impl Editor {
    fn new(state: Shared) -> Self {
        Self {
            state,
            viewport: Default::default(),
            rows: vec![],
            identity: None,
        }
    }
}
impl Panel for Editor {
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::TYPE)
    }
    fn new_instance(&self) -> Option<sdk::EditorPanels> {
        Some(panels())
    }
    fn cancel_interaction(&mut self, host: &mut dyn DesktopClient) {
        self.state.borrow_mut().cancel(host);
        self.viewport.cancel();
    }
    fn draw_viewer_overlay(&mut self, context: ExtensionUi<'_>, rect: sdk::ViewerRect) {
        let mut state = self.state.borrow_mut();
        state.sync(context.host);
        self.viewport.draw(context, rect, &mut state);
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.state.borrow_mut();
        state.sync(host);
        if ui.is_window_focused()
            && !ui.io().want_text_input()
            && ui.io().key_ctrl()
            && ui.is_key_pressed(sdk::imgui::Key::G)
            && let Err(error) = state.group(host)
        {
            state.error = error;
        }
        let mut add = None;
        {
            let _disabled = ui.begin_disabled_with_cond(
                state.busy()
                    || state.editing()
                    || host.state().playing
                    || state.current().is_none(),
            );
            if let Some(_menu) = sdk::toolbar::menu_button(ui, "Add", "Add a 3D object") {
                for (label, kind) in [
                    ("Box", crate::tools::Kind::Box),
                    ("Sphere", crate::tools::Kind::Sphere),
                    ("Group", crate::tools::Kind::Group),
                    ("Camera", crate::tools::Kind::Camera),
                    ("Light", crate::tools::Kind::Light),
                ] {
                    if ui.menu_item(label) {
                        add = Some(kind);
                    }
                }
            }
            self.viewport.toolbar(ui);
        }
        if let Some(kind) = add
            && let Err(error) = state.create(host, kind)
        {
            state.error = error;
        }
        let Some(scene) = state.current() else {
            ui.text_disabled("Select a 3D document.");
            return;
        };
        fn rows(
            scene: &Scene,
            ids: &[ObjectId],
            depth: usize,
            output: &mut Vec<(ObjectId, usize)>,
        ) {
            for id in ids {
                output.push((*id, depth));
                if let Content::Group { children, .. } = &scene.objects[id].content {
                    rows(scene, children, depth + 1, output);
                }
            }
        }
        let identity = state.document.map(|document| (document, state.revision()));
        if identity != self.identity {
            self.rows.clear();
            rows(scene, &scene.roots, 0, &mut self.rows);
            self.identity = identity;
        }
        let mut clicked = None;
        let mut deleted = None;
        let mut duplicated = None;
        let mut unique = None;
        let mut grouped = false;
        let mut moved = None;
        let mut clipper = sdk::imgui::ListClipper::new(self.rows.len()).begin(ui);
        while clipper.step() {
            for (id, depth) in &self.rows[clipper.display_start()..clipper.display_end()] {
                let object = &scene.objects[id];
                let _id = ui.push_id(&id.to_string());
                let indent = *depth as f32 * ui.clone_style().indent_spacing();
                ui.indent_by(indent);
                if ui
                    .selectable_config(&object.name)
                    .selected(state.selected.contains(id))
                    .build()
                {
                    let mut selection = if ui.io().key_ctrl() {
                        state.selected.clone()
                    } else {
                        vec![]
                    };
                    if let Some(index) = selection.iter().position(|selected| selected == id) {
                        selection.remove(index);
                    } else {
                        selection.push(*id);
                    }
                    clicked = Some(selection);
                }
                if let Some(_menu) = ui.begin_popup_context_item() {
                    {
                        let _disabled = ui.begin_disabled_with_cond(
                            state.busy() || state.editing() || host.state().playing,
                        );
                        if ui.menu_item("Duplicate") {
                            duplicated = Some(*id);
                        }
                    }
                    if matches!(object.content, Content::Instance { .. }) {
                        let _disabled = ui.begin_disabled_with_cond(
                            state.busy()
                                || state.editing()
                                || host.state().playing
                                || crate::editing::editable(scene, *id).is_err(),
                        );
                        if ui.menu_item("Make geometry unique") {
                            unique = Some((*id, true));
                        }
                        if ui.menu_item("Make material unique") {
                            unique = Some((*id, false));
                        }
                    }
                    {
                        let _disabled = ui.begin_disabled_with_cond(
                            state.busy()
                                || state.editing()
                                || host.state().playing
                                || !state.selected.contains(id),
                        );
                        if ui.menu_item("Group selection") {
                            grouped = true;
                        }
                    }
                    {
                        let _disabled = ui.begin_disabled_with_cond(
                            state.busy()
                                || state.editing()
                                || host.state().playing
                                || crate::editing::editable(scene, *id).is_err(),
                        );
                        for (label, keep_world) in [
                            ("Parent (keep world transform)", true),
                            ("Parent (keep local transform)", false),
                        ] {
                            if let Some(_menu) = ui.begin_menu(label) {
                                let current = crate::editing::placement(scene, *id)
                                    .ok()
                                    .and_then(|p| p.parent);
                                if current.is_some() && ui.menu_item("Scene root") {
                                    moved = Some((*id, None, keep_world));
                                }
                                for (candidate, _) in &self.rows {
                                    if *candidate == *id
                                        || current == Some(*candidate)
                                        || !matches!(
                                            scene.objects[candidate].content,
                                            Content::Group { .. }
                                        )
                                    {
                                        continue;
                                    }
                                    let mut ancestor = Some(*candidate);
                                    let mut descendant = false;
                                    while let Some(owner) = ancestor {
                                        if owner == *id {
                                            descendant = true;
                                            break;
                                        }
                                        ancestor = crate::editing::placement(scene, owner)
                                            .ok()
                                            .and_then(|p| p.parent);
                                    }
                                    if descendant
                                        || crate::editing::editable(scene, *candidate).is_err()
                                    {
                                        continue;
                                    }
                                    let _target = ui.push_id(&candidate.to_string());
                                    if ui.menu_item(&scene.objects[candidate].name) {
                                        moved = Some((*id, Some(*candidate), keep_world));
                                    }
                                }
                            }
                        }
                    }
                    let _disabled = ui.begin_disabled_with_cond(
                        state.busy()
                            || state.editing()
                            || host.state().playing
                            || crate::editing::editable(scene, *id).is_err()
                            || scene.active_camera == *id,
                    );
                    if ui.menu_item("Delete object") {
                        deleted = Some(*id);
                    }
                }
                ui.unindent_by(indent);
            }
        }
        if let Some(objects) = clicked {
            state.cancel(host);
            host.command(DesktopCommand::Select(Selection {
                document: state.document,
                objects,
            }));
        }
        if let Some(id) = duplicated
            && let Err(error) = state.duplicate(host, id)
        {
            state.error = error;
        }
        if let Some((id, geometry)) = unique
            && let Err(error) = state.make_unique(host, id, geometry)
        {
            state.error = error;
        }
        if let Some(id) = deleted
            && let Err(error) = state.delete(host, id)
        {
            state.error = error;
        }
        if let Some((id, parent, keep_world)) = moved
            && let Err(error) = state.move_to(host, id, parent, keep_world)
        {
            state.error = error;
        }
        if grouped && let Err(error) = state.group(host) {
            state.error = error;
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
