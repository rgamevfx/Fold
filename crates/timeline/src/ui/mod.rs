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
            ui.text_wrapped("Import MP4 footage or PCM16 WAVE audio from Media / Selection. The timeline package creates video and audio tracks with linked A/V clips.");
            ui.separator();
            ui.text_wrapped(&state.status);
            return;
        };
        if self.document != Some(*document) {
            self.canvas = Canvas::default();
            self.document = Some(*document);
        }
        let keyboard = ui.is_window_focused() && !ui.io().want_text_input();
        if ui.button(if state.playing { "Pause" } else { "Play" })
            || (keyboard && ui.is_key_pressed(Key::Space))
        {
            if state.playing {
                host.command(DesktopCommand::Pause);
            } else {
                // Transport belongs to this workspace even if a source-view
                // tab-focus transition has not reached the shell yet.
                host.command(DesktopCommand::ActivateWorkspace(crate::SEQUENCE.into()));
                host.command(DesktopCommand::Play);
            }
        }
        ui.same_line();
        if (ui.button("Split")
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
        ui.same_line();
        if (ui.button("Lift") || (keyboard && ui.is_key_pressed(Key::Delete)))
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
        ui.same_line();
        if ui.button("Unlink")
            && let Some(id) = selected_clip(sequence, &state.selection)
        {
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
        for (label, kind) in [("+ Video", TrackKind::Video), ("+ Audio", TrackKind::Audio)] {
            ui.same_line();
            if ui.button(label) {
                submit(host, *revision, *document, SequenceEdit::AddTrack { kind });
            }
        }
        ui.same_line();
        ui.checkbox("Snap", &mut self.canvas.snapping);
        ui.same_line();
        ui.checkbox("Linked", &mut self.canvas.linked);
        ui.same_line();
        if ui.button("Fit") {
            self.canvas.fit = true;
        }
        ui.same_line();
        if ui.button("-") {
            self.canvas.view.zoom(1.0 / 1.3, 0.0, 0.0);
        }
        ui.same_line();
        if ui.button("+") {
            self.canvas.view.zoom(1.3, 0.0, 0.0);
        }
        if keyboard {
            if ui.is_key_pressed(Key::LeftArrow) {
                host.command(DesktopCommand::Seek(state.frame.saturating_sub(1)));
            }
            if ui.is_key_pressed(Key::RightArrow) {
                host.command(DesktopCommand::Seek(state.frame.saturating_add(1)));
            }
            if ui.is_key_pressed(Key::Home) {
                host.command(DesktopCommand::Seek(0));
            }
            if ui.is_key_pressed(Key::End) {
                host.command(DesktopCommand::Seek(state.frames - 1));
            }
        }
        ui.text(format!(
            "Frame {}  |  {}/{} fps  |  {}  |  underruns {}",
            state.frame,
            sequence.rate[0],
            sequence.rate[1],
            if state.priming {
                "Priming"
            } else if state.playing && state.audio_clock {
                "Audio clock"
            } else if state.playing {
                "Monotonic clock"
            } else {
                "Paused"
            },
            state.underruns
        ));
        if !self.canvas.message.is_empty() {
            ui.text(&self.canvas.message);
        } else {
            ui.text("Drag clips / trim edges • ruler seeks • Ctrl+wheel zooms • Shift+wheel pans • wheel scrolls tracks • E visibility, M mute, S solo, L lock");
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
