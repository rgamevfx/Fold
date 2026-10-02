//! Typed Project payloads shared by browser and creative editors.
use super::imgui::{DragDropTargetFlags, Ui};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::browser::{BrowserCommand, Entry, Placement};
use fold_platform::desktop::{DesktopClient, DesktopCommand};

pub const PAYLOAD: &str = "FOLD_PROJECT_ENTRY";
pub fn source(ui: &Ui, entry: Entry, label: &str) {
    if let Some(_source) = ui.drag_drop_source_config(PAYLOAD).begin_payload(entry) {
        ui.text(label);
    }
}
pub fn target(ui: &Ui) -> Option<Entry> {
    let target = ui.drag_drop_target()?;
    let payload = target
        .accept_payload::<Entry, _>(PAYLOAD, DragDropTargetFlags::NONE)?
        .ok()?;
    payload.delivery.then_some(payload.data)
}
/// Overlay the canvas rectangle during drag-and-drop, accepting only Project
/// payloads. ImGui's own docking drags also pass through this layout path.
pub fn canvas_target(ui: &Ui, origin: [f32; 2], size: [f32; 2]) -> Option<Entry> {
    ui.drag_drop_payload()?;
    let previous = ui.cursor_screen_pos();
    ui.set_cursor_screen_pos(origin);
    ui.invisible_button("##project-drop", [size[0].max(1.), size[1].max(1.)]);
    let entry = target(ui);
    // A canvas can leave its next-item cursor beyond its last item's bounds
    // (e.g. trailing item spacing). Restoring that position without submitting
    // an item makes ImGui abort at End(), including during dock-tab drags.
    ui.set_cursor_screen_pos(previous);
    ui.dummy([0., 0.]);
    ui.set_cursor_screen_pos(previous);
    entry
}
pub fn place(
    host: &mut dyn DesktopClient,
    entry: Entry,
    target: DocumentId,
    track: Option<ObjectId>,
    at: Time,
) {
    let source = match entry.source() {
        Ok(source) => source,
        Err(error) => {
            host.command(DesktopCommand::Notify(error));
            return;
        }
    };
    host.command(DesktopCommand::Browser(BrowserCommand::Place(Placement {
        source,
        target,
        track,
        at,
        source_start: Time::ZERO,
        duration: None,
        audio_only: false,
    })));
}
