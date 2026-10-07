/// Shell-owned surface, dispatched to the linked editor's composition capability.
pub(crate) const PANEL_ID: &str = "fold.ui.composition";
pub(crate) struct Surface;
impl crate::sdk::Panel for Surface {
    fn id(&self) -> &'static str {
        PANEL_ID
    }
    fn supports_document_type(&self, _: &str) -> bool {
        true
    }
    fn draw(&mut self, context: crate::sdk::ExtensionUi<'_>) {
        context
            .ui
            .text_disabled("Choose a source editor for this group.");
    }
}

/// Reuse the retired dock slot; preserve unrelated unavailable contributions.
pub(crate) fn migrate_workspace(
    workspace: &mut fold_platform::workspace::Workspace,
    retired_editors: impl IntoIterator<Item = fold_platform::workspace::PanelInstanceId>,
) {
    let current = workspace
        .panels
        .iter()
        .find_map(|(&id, name)| (name == PANEL_ID).then_some(id));
    let retired: Vec<_> = workspace
        .panels
        .iter()
        .filter_map(|(&id, name)| (name == "fold.ui.animation").then_some(id))
        .collect();
    let mut target = current;
    for id in retired {
        if target.is_some() {
            workspace.panels.remove(&id);
            remove_window(
                &mut workspace.layout,
                &format!("[Window][fold.panel.{}]", id.0),
            );
        } else {
            workspace.panels.insert(id, PANEL_ID.into());
            target = Some(id);
        }
    }
    for id in retired_editors {
        remove_window(
            &mut workspace.layout,
            &format!("[Window][fold.editor.{}]", id.0),
        );
    }
    workspace.layout = workspace.layout.replace(
        "[Window][fold.ui.animation]",
        "[Window][fold.ui.composition]",
    );
}

// Remove only known retired window sections, retaining the docking tree and peers.
fn remove_window(layout: &mut String, header: &str) {
    if !layout.contains(header) {
        return;
    }
    let mut skip = false;
    *layout = layout
        .split_inclusive('\n')
        .filter(|line| {
            if line.starts_with('[') {
                skip = line.trim_end() == header;
            }
            !skip
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_slot_migrates_once_without_losing_other_panels() {
        let mut workspace = fold_platform::workspace::Workspace::default();
        let old = workspace.panel("fold.ui.animation");
        let other = workspace.panel("unavailable.extension");
        workspace.layout = format!(
            "[Window][fold.panel.{}]\nDockId=42\n\n[Window][other]\nPos=3,4\n",
            old.0
        );
        let layout = workspace.layout.clone();
        migrate_workspace(&mut workspace, []);
        assert_eq!(workspace.panel(PANEL_ID), old);
        assert_eq!(workspace.panels[&other], "unavailable.extension");
        assert_eq!(workspace.layout, layout);
        let migrated = serde_json::to_string(&workspace).unwrap();
        migrate_workspace(&mut workspace, []);
        assert_eq!(serde_json::to_string(&workspace).unwrap(), migrated);

        let duplicate = workspace.panel("fold.ui.animation");
        workspace.layout.push_str(&format!(
            "[Window][fold.panel.{}]\nDockId=43\n",
            duplicate.0
        ));
        migrate_workspace(&mut workspace, []);
        assert!(!workspace.panels.contains_key(&duplicate));
        assert_eq!(workspace.layout, layout);
        let editor = workspace.add_editor("test.composition", "test.document");
        workspace
            .layout
            .push_str(&format!("[Window][fold.editor.{}]\nDockId=44\n", editor.0));
        migrate_workspace(&mut workspace, [editor]);
        assert_eq!(workspace.layout, layout);
        assert!(workspace.editors.contains_key(&editor));
    }
}
