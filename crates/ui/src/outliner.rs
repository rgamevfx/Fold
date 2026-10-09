//! Shared hierarchy presentation. Providers own object semantics and mutations.
use crate::sdk::{ExtensionUi, Panel, imgui, toolbar};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::{
    desktop::{DesktopCommand, Selection},
    hierarchy::{Edit, EditArgs, Hierarchy},
    packages::CommandRequest,
};
use std::collections::BTreeMap;
pub const PANEL_ID: &str = "fold.ui.outliner";
#[derive(Default)]
pub struct Surface {
    names: BTreeMap<(DocumentId, ObjectId), String>,
    base: Option<(DocumentId, fold_project::Revision)>,
}
impl Panel for Surface {
    fn id(&self) -> &'static str {
        PANEL_ID
    }
    fn supports_document_type(&self, _: &str) -> bool {
        true
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let Some(document) = host.state().selection.document else {
            ui.text_disabled("Select a scene.");
            return;
        };
        let base = host
            .snapshot()
            .map(|snapshot| (document, snapshot.revision()));
        if base != self.base {
            self.names.clear();
            self.base = base;
        }
        let hierarchy = match host.hierarchy(document) {
            Ok(h) => h,
            Err(e) => {
                ui.text_wrapped(e);
                return;
            }
        };
        if hierarchy.rows.is_empty() && hierarchy.create.is_empty() {
            ui.text_disabled("Select a scene to organize its objects.");
            return;
        }
        let mut edit = None;
        if let Some(_menu) = toolbar::menu_button(ui, "Add", "Add scene object") {
            for kind in &hierarchy.create {
                let mut label = kind.clone();
                if let Some(c) = label.get_mut(0..1) {
                    c.make_ascii_uppercase();
                }
                if ui.menu_item(&label) {
                    edit = Some(Edit::Create { kind: kind.clone() });
                }
            }
        }
        let selected = host.state().selection.objects.clone();
        let mut select = None;
        fn rows(
            ui: &imgui::Ui,
            hierarchy: &Hierarchy,
            parent: Option<ObjectId>,
            document: DocumentId,
            selected: &[ObjectId],
            names: &mut BTreeMap<(DocumentId, ObjectId), String>,
            edit: &mut Option<Edit>,
            select: &mut Option<ObjectId>,
            depth: usize,
        ) {
            if depth > 256 {
                return;
            }
            for row in hierarchy.rows.iter().filter(|r| r.parent == parent) {
                let _id = ui.push_id(&row.id.to_string());
                let children = hierarchy.rows.iter().any(|r| r.parent == Some(row.id));
                let label = if row.visible {
                    row.name.clone()
                } else {
                    format!("{} (hidden)", row.name)
                };
                let token = ui
                    .tree_node_config("object")
                    .label(&label)
                    .open_on_arrow(true)
                    .span_avail_width(true)
                    .leaf(!children)
                    .selected(selected.contains(&row.id))
                    .opened(true, imgui::Condition::Once)
                    .push();
                if ui.is_item_clicked() {
                    *select = Some(row.id);
                }
                if let Some(_popup) = ui.begin_popup_context_item_with_label(Some("object-options"))
                {
                    ui.text_disabled(&row.kind);
                    let mut locked = row.locked;
                    if ui.checkbox("Locked", &mut locked) {
                        *edit = Some(Edit::Lock {
                            object: row.id,
                            value: locked,
                        });
                    }
                    let _disabled = ui.begin_disabled_with_cond(row.locked);
                    let mut visible = row.visible;
                    if ui.checkbox("Visible", &mut visible) {
                        *edit = Some(Edit::Visibility {
                            object: row.id,
                            value: visible,
                        });
                    }
                    let name = names
                        .entry((document, row.id))
                        .or_insert_with(|| row.name.clone());
                    if ui.input_text("Name", name).enter_returns_true(true).build() {
                        *edit = Some(Edit::Rename {
                            object: row.id,
                            name: name.clone(),
                        });
                    }
                    if let Some(_parents) = ui.begin_menu("Parent") {
                        if ui.menu_item("Scene root") {
                            *edit = Some(Edit::Reparent {
                                object: row.id,
                                parent: None,
                            });
                        }
                        for candidate in &hierarchy.rows {
                            if candidate.id != row.id
                                && ui.menu_item(&format!("{}##{}", candidate.name, candidate.id))
                            {
                                *edit = Some(Edit::Reparent {
                                    object: row.id,
                                    parent: Some(candidate.id),
                                });
                            }
                        }
                    }
                    if row.kind == "Camera" && ui.menu_item("Use as active camera") {
                        *edit = Some(Edit::ActiveCamera { object: row.id });
                    }
                    if ui.menu_item("Delete object and children") {
                        *edit = Some(Edit::Remove { object: row.id });
                    }
                }
                if let Some(_node) = token {
                    rows(
                        ui,
                        hierarchy,
                        Some(row.id),
                        document,
                        selected,
                        names,
                        edit,
                        select,
                        depth + 1,
                    );
                }
            }
        }
        rows(
            ui,
            &hierarchy,
            None,
            document,
            &selected,
            &mut self.names,
            &mut edit,
            &mut select,
            0,
        );
        if let Some(object) = select {
            host.command(DesktopCommand::Select(Selection {
                document: Some(document),
                objects: vec![object],
            }));
        }
        if let Some(edit) = edit
            && let Some(snapshot) = host.snapshot()
        {
            match serde_json::to_vec(&EditArgs {
                document,
                edits: vec![edit],
            }) {
                Ok(arguments) => host.command(DesktopCommand::Extension(CommandRequest {
                    id: fold_platform::hierarchy::EDIT.into(),
                    base: snapshot.revision(),
                    arguments,
                })),
                Err(e) => ui.text_wrapped(e.to_string()),
            }
        }
    }
}
