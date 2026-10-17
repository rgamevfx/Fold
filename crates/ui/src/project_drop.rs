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
    preview_target(ui).and_then(|(entry, delivery)| delivery.then_some(entry))
}
pub fn preview_target(ui: &Ui) -> Option<(Entry, bool)> {
    let target = ui.drag_drop_target()?;
    let payload = target
        .accept_payload::<Entry, _>(PAYLOAD, DragDropTargetFlags::BEFORE_DELIVERY)?
        .ok()?;
    Some((payload.data, payload.delivery))
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
    let mut request = placement(target, at);
    request.track = track;
    place_request(host, entry, request);
}

/// Current-frame native drop geometry. Cleared before each draw, so hidden tabs and
/// retargeted editors cannot consume a drop intended for another surface.
#[derive(Default)]
pub struct FileDrop {
    pub pointer: Option<[f32; 2]>,
    regions: Vec<([f32; 2], [f32; 2], Placement, Option<[[f32; 2]; 2]>)>,
}
impl FileDrop {
    pub fn begin_frame(&mut self, pointer: Option<[f32; 2]>) {
        self.pointer = pointer;
        self.regions.clear();
    }
    pub fn region(
        &mut self,
        ui: &Ui,
        origin: [f32; 2],
        size: [f32; 2],
        placement: Placement,
        transform: Option<[[f32; 2]; 2]>,
    ) {
        if !ui.is_window_hovered_with_flags(super::imgui::WindowHoveredFlags::CHILD_WINDOWS) {
            return;
        }
        let max = [origin[0] + size[0], origin[1] + size[1]];
        if self.pointer.is_some_and(|p| inside(p, origin, max)) {
            ui.get_window_draw_list()
                .add_rect(
                    origin,
                    max,
                    ui.style_color(super::imgui::StyleColor::DragDropTarget),
                )
                .thickness(2.)
                .build();
        }
        self.regions.push((origin, max, placement, transform));
    }
    pub fn placement(&self, position: [f32; 2]) -> Option<Placement> {
        let (_, _, request, transform) = self
            .regions
            .iter()
            .rev()
            .find(|(min, max, _, _)| inside(position, *min, *max))?;
        let mut request = request.clone();
        if let Some([origin, scale]) = transform {
            let point = [0, 1].map(|i| origin[i] + position[i] * scale[i]);
            if request.canvas_position.is_some() {
                request.canvas_position = Some(point.map(f64::from));
            } else if let Some(graph) = &mut request.graph {
                graph.position = point;
            }
        }
        Some(request)
    }
    pub fn dropped(
        &self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) -> bool {
        let Some(placement) = self.placement(position) else {
            return false;
        };
        import_and_place(host, paths, placement);
        true
    }
}
pub fn inside(p: [f32; 2], min: [f32; 2], max: [f32; 2]) -> bool {
    (0..2).all(|i| p[i] >= min[i] && p[i] < max[i])
}
pub fn placement(target: DocumentId, at: Time) -> Placement {
    Placement {
        graph: None,
        canvas_position: None,
        source: fold_platform::browser::PlacementSource::Asset(Default::default()),
        target,
        track: None,
        at,
        source_start: Time::ZERO,
        duration: None,
        audio_only: false,
        new_track: false,
        mode: Default::default(),
    }
}
pub fn import_and_place(
    host: &mut dyn DesktopClient,
    paths: &[std::path::PathBuf],
    placement: Placement,
) {
    host.command(DesktopCommand::Browser(BrowserCommand::ImportAndPlace {
        paths: paths.to_vec(),
        placement,
    }));
}
pub fn place_request(host: &mut dyn DesktopClient, entry: Entry, mut request: Placement) {
    if let Entry::Component(component, _) = entry {
        let args = fold_platform::gallery::Place {
            component,
            document: request.target,
            objects: if request.track.is_some() {
                vec![]
            } else if host.state().selection.document == Some(request.target) {
                host.state().selection.objects.clone()
            } else {
                vec![]
            },
            at: request.at,
            position: request.canvas_position,
            track: request.track,
            update: None,
        };
        gallery_place(host, args);
        return;
    }

    match entry.source(|id| host.default_output(id)) {
        Ok(source) => {
            request.source = source;
            host.command(DesktopCommand::Browser(BrowserCommand::Place(request)));
        }
        Err(error) => host.command(DesktopCommand::Notify(error)),
    }
}

pub fn gallery_place(host: &mut dyn DesktopClient, args: fold_platform::gallery::Place) {
    let Some(snapshot) = host.snapshot() else {
        return;
    };
    match serde_json::to_vec(&args) {
        Ok(arguments) => host.command(DesktopCommand::Extension(
            fold_platform::packages::CommandRequest {
                id: fold_platform::gallery::PLACE.into(),
                base: snapshot.revision(),
                arguments,
            },
        )),
        Err(error) => host.command(DesktopCommand::Notify(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_targets_use_graph_transform_and_clear_hidden_regions() {
        let mut drop = FileDrop::default();
        let target = DocumentId::new();
        let mut request = placement(target, Time::ZERO);
        request.graph = Some(fold_platform::browser::GraphPlacement {
            position: [0.; 2],
            group: None,
            object: None,
        });
        drop.regions.push((
            [100., 100.],
            [500., 400.],
            request,
            Some([[-100., -200.], [2., 2.]]),
        ));
        let p = drop.placement([150., 160.]).unwrap();
        assert_eq!(p.target, target);
        assert_eq!(p.graph.unwrap().position, [200., 120.]);
        drop.regions[0].2.graph.as_mut().unwrap().position = [7., 9.];
        drop.regions[0].2.canvas_position = Some([0.; 2]);
        let p = drop.placement([150., 160.]).unwrap();
        assert_eq!(p.graph.unwrap().position, [7., 9.]);
        assert_eq!(p.canvas_position, Some([200., 120.]));
        assert!(drop.placement([500., 200.]).is_none());
        drop.begin_frame(Some([150., 160.]));
        assert!(drop.placement([150., 160.]).is_none());
    }
}
