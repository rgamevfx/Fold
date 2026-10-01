//! Timeline-owned trusted panel contributions. The shared shell only docks and
//! invokes them. Selection is host UI state; drag/zoom state stays panel-local.
mod canvas;
mod inspector;
mod interaction;
#[cfg(test)]
mod tests;
use crate::{ClipEdit, Sequence, SequenceEdit, TrackKind, package};
use canvas::{Action, Canvas, CanvasModel};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection},
    packages::CommandRequest,
};
use fold_project::{Revision, Snapshot};
use fold_ui::sdk::{
    ExtensionUi, Panel, PanelRegistry,
    imgui::{Key, Ui},
    toolbar::{ToolbarIcon, icon_button, menu_button, tooltip},
};
use std::collections::BTreeMap;

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    registry.register(crate::PACKAGE, TimelinePanel::default())?;
    registry.register(crate::PACKAGE, inspector::Inspector::default())
}
fn current(snapshot: &Snapshot, selection: &Selection) -> Result<(DocumentId, Sequence), String> {
    if let Some(id) = selection.document
        && let Some(document) = snapshot.state().documents.get(&id)
        && document.type_id == crate::SEQUENCE
    {
        return Ok((id, Sequence::from_document(document)?));
    }
    let (reference, sequence) = crate::active(snapshot)?;
    Ok((reference.document, sequence))
}
fn request(
    base: Revision,
    document: DocumentId,
    edit: SequenceEdit,
) -> Result<CommandRequest, String> {
    Ok(CommandRequest {
        id: package::EDIT.into(),
        base,
        arguments: serde_json::to_vec(&package::EditArgs { document, edit })
            .map_err(|e| e.to_string())?,
    })
}
fn submit(host: &mut dyn DesktopClient, base: Revision, document: DocumentId, edit: SequenceEdit) {
    // These owned scalar/enum arguments have no fallible custom serializers.
    if let Ok(request) = request(base, document, edit) {
        host.command(DesktopCommand::Extension(request));
    }
}
#[derive(Default)]
struct TimelinePanel {
    canvas: Canvas,
    document: Option<DocumentId>,
    cached: Option<(
        Revision,
        DocumentId,
        Sequence,
        BTreeMap<fold_foundation::AssetId, String>,
    )>,
}
impl Panel for TimelinePanel {
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::SEQUENCE)
    }
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let Some(snapshot) = host.snapshot() else {
            ui.text("No project is open.");
            return;
        };
        let state = host.state().clone();
        if self.cached.as_ref().is_none_or(|(revision, id, _, _)| {
            *revision != snapshot.revision()
                || state
                    .selection
                    .document
                    .is_some_and(|selected| selected != *id)
        }) {
            self.cached = current(&snapshot, &state.selection)
                .ok()
                .map(|(id, sequence)| {
                    let labels = snapshot
                        .state()
                        .assets
                        .iter()
                        .map(|(id, asset)| {
                            (
                                *id,
                                std::path::Path::new(&asset.location)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                            )
                        })
                        .collect();
                    (snapshot.revision(), id, sequence, labels)
                });
        }
        let Some((revision, document, sequence, labels)) = &self.cached else {
            ui.text_wrapped("Import media to start a sequence.");
            return;
        };
        if self.document != Some(*document) {
            self.canvas = Canvas::default();
            self.document = Some(*document);
        }
        let keyboard = ui.is_window_focused() && !ui.io().want_text_input();
        let wide = ui.content_region_avail()[0] >= ui.current_font_size() * 42.;
        let mut split = false;
        let mut lift = false;
        let mut unlink = false;
        if let Some(_menu) = menu_button(ui, "Edit", "Clip editing actions") {
            let _disabled =
                ui.begin_disabled_with_cond(selected_clip(sequence, &state.selection).is_none());
            split = ui.menu_item_with_shortcut("Split", "S / Ctrl+K");
            lift = ui.menu_item_with_shortcut("Lift", "Delete");
            unlink = ui.menu_item("Unlink audio and video");
        }
        ui.same_line();
        if let Some(_menu) = menu_button(ui, "Tracks", "Add a track") {
            for (label, kind) in [
                ("Add video track", TrackKind::Video),
                ("Add audio track", TrackKind::Audio),
            ] {
                if ui.menu_item(label) {
                    submit(host, *revision, *document, SequenceEdit::AddTrack { kind });
                }
            }
        }
        ui.same_line();
        if let Some(_menu) = menu_button(ui, "View", "Timeline view and editing options") {
            if ui.menu_item("Fit sequence") {
                self.canvas.fit = true;
            }
            if ui.menu_item("Zoom in") {
                self.canvas.view.zoom(1.3, 0.0, 0.0);
            }
            if ui.menu_item("Zoom out") {
                self.canvas.view.zoom(1.0 / 1.3, 0.0, 0.0);
            }
            ui.separator();
            ui.menu_item_toggle_no_shortcut("Snapping", &mut self.canvas.snapping, true);
            ui.menu_item_toggle_no_shortcut("Linked selection", &mut self.canvas.linked, true);
        }
        if wide {
            ui.same_line();
            {
                let _disabled = ui
                    .begin_disabled_with_cond(selected_clip(sequence, &state.selection).is_none());
                split |= ui.button("Split");
                tooltip(ui, "Split at playhead (S / Ctrl+K)");
                ui.same_line();
                lift |= ui.button("Lift");
                tooltip(ui, "Remove selected clips without closing the gap (Delete)");
            }
            ui.same_line();
            ui.checkbox("Snap", &mut self.canvas.snapping);
            tooltip(ui, "Snap to clip edges and playhead");
            ui.same_line();
            ui.checkbox("Linked", &mut self.canvas.linked);
            tooltip(ui, "Edit linked audio and video together");
        }
        ui.same_line();
        if icon_button(ui, "fit-sequence", ToolbarIcon::FrameAll, "Fit sequence") {
            self.canvas.fit = true;
        }
        ui.same_line();
        icon_button(
            ui,
            "timeline-help",
            ToolbarIcon::Help,
            "Drag clips or trim edges; click the ruler to seek.\nCtrl+wheel: zoom; Shift+wheel: pan; wheel: scroll tracks.\nTrack controls: E visibility, M mute, S solo, L lock.",
        );
        if (split
            || (keyboard
                && (ui.is_key_pressed(Key::S)
                    || (ui.io().key_ctrl() && ui.is_key_pressed(Key::K)))))
            && let Some(id) = selected_clip(sequence, &state.selection)
            && let Ok(at) = interaction::time(i64::from(state.frame), sequence.rate)
        {
            submit(
                host,
                *revision,
                *document,
                SequenceEdit::Clip {
                    id,
                    linked: self.canvas.linked,
                    edit: ClipEdit::Split {
                        at,
                        right_id: ObjectId::new(),
                    },
                },
            );
        }
        if (lift || (keyboard && ui.is_key_pressed(Key::Delete)))
            && let Some(id) = selected_clip(sequence, &state.selection)
        {
            submit(
                host,
                *revision,
                *document,
                SequenceEdit::Clip {
                    id,
                    linked: self.canvas.linked,
                    edit: ClipEdit::Lift,
                },
            );
        }
        if unlink && let Some(id) = selected_clip(sequence, &state.selection) {
            submit(
                host,
                *revision,
                *document,
                SequenceEdit::Clip {
                    id,
                    linked: true,
                    edit: ClipEdit::Unlink,
                },
            );
        }
        if !self.canvas.message.is_empty() {
            ui.text_wrapped(&self.canvas.message);
        }
        ui.separator();
        for action in self.canvas.draw(
            ui,
            CanvasModel {
                sequence,
                revision: *revision,
                playhead: state.frame,
                selected: &state.selection.objects,
                labels,
            },
        ) {
            match action {
                Action::Select(objects) => host.command(DesktopCommand::Select(Selection {
                    document: Some(*document),
                    objects,
                })),
                Action::Seek(frame) => host.command(DesktopCommand::Seek(frame)),
                Action::Pause => host.command(DesktopCommand::Pause),
                Action::Commit { base, edit } => submit(host, base, *document, edit),
            }
        }
    }
}
fn selected_clip(sequence: &Sequence, selection: &Selection) -> Option<ObjectId> {
    selection
        .objects
        .iter()
        .find(|id| sequence.clips.iter().any(|c| c.id == **id))
        .copied()
}
fn frame_input(ui: &Ui, label: &str, value: &mut i32) {
    ui.input_int(label, value);
}
