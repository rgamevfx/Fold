//! Shared hierarchy, layer timing and animation for all graph types.
mod actions;
mod inspector;
#[cfg(test)]
mod render_tests;
mod state;
#[cfg(test)]
mod tests;
use crate::package;
use fold_platform::desktop::{DesktopCommand, TransportAction};
use fold_ui::sdk::{
    self, ExtensionUi, Panel, PanelRegistry,
    animation_editor::{Channel, Context, Editor, LayerControls, Track},
};
use state::Shared;

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = Shared::default();
    registry.register(crate::PACKAGE, Canvas::new(state.clone()))?;
    registry.register_inspector(crate::PACKAGE, inspector::Inspector(state))
}
struct Canvas {
    state: Shared,
    files: fold_ui::sdk::project_drop::FileDrop,
    animation: Editor,
}
impl Canvas {
    fn new(state: Shared) -> Self {
        Self {
            state,
            files: Default::default(),
            animation: Editor::default(),
        }
    }
}
impl Panel for Canvas {
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.files.begin_frame(position);
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn fold_platform::desktop::DesktopClient,
    ) -> bool {
        self.files.dropped(position, paths, host)
    }

    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITION)
    }
    fn supports_composition(&self) -> bool {
        true
    }
    fn layer_for_graph(
        &self,
        document: fold_foundation::DocumentId,
        object: Option<fold_foundation::ObjectId>,
    ) -> Option<(fold_foundation::ObjectId, bool)> {
        let state = self.state.borrow();
        let composition = state.composition.as_ref()?;
        let layer = composition.layers.iter().find(|l| {
            matches!(&l.source, Some(fold_platform::image_source::Source::Output(source)) if source.document == document) && l.graph_object() == object
        })?;
        Some((layer.id, composition.locked(layer.id)))
    }
    fn new_instance(&self) -> Option<sdk::EditorPanels> {
        let state = Shared::default();
        Some(sdk::EditorPanels {
            editor: Box::new(Self::new(state.clone())),
            inspector: Box::new(inspector::Inspector(state)),
        })
    }
    fn cancel_interaction(&mut self, host: &mut dyn fold_platform::desktop::DesktopClient) {
        if self.state.borrow().editing {
            self.state.borrow_mut().cancel(host);
        }
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.state.borrow_mut();
        state.sync(host);
        state.follow_layer(host);
        actions::draw(ui, host, &mut state);
        inspector::settings(ui, host, &mut state);
        let Some(document) = state.document else {
            ui.text_disabled("Create a composition or open one from Project.");
            return;
        };
        let Some(composition) = state.composition.as_ref() else {
            return;
        };
        let info = composition.info.clone();
        let tracks: Vec<_> = composition
            .layers
            .iter()
            .map(|l| Track {
                id: l.id,
                parent: l.parent,
                label: l.name.clone(),
                locked: composition.locked(l.id),
                range: Some((l.start, l.end)),
                layer: Some(LayerControls {
                    accepts_children: l.source.is_none() || l.graph_children,
                    visible: l.visible,
                    icon: if l.source.is_none() || l.graph_children {
                        "G"
                    } else {
                        "L"
                    },
                }),
            })
            .collect();
        let mut channels: Vec<_> = composition
            .layers
            .iter()
            .flat_map(|l| {
                l.animation.iter().map(move |(path, curve)| Channel {
                    object: l.id,
                    node_label: l.name.clone(),
                    property: if path == "scene.opacity.0" {
                        "opacity"
                    } else if path == "transform.4" || path == "transform.5" {
                        "position"
                    } else {
                        "axes"
                    }
                    .into(),
                    property_label: if path == "scene.opacity.0" {
                        "Opacity"
                    } else if path == "transform.4" || path == "transform.5" {
                        "Position"
                    } else {
                        "Transform axes"
                    }
                    .into(),
                    component: match path.as_str() {
                        "scene.opacity.0" => "",
                        "transform.0" => "XX",
                        "transform.1" => "YX",
                        "transform.2" => "XY",
                        "transform.3" => "YY",
                        "transform.4" => "X",
                        "transform.5" => "Y",
                        _ => "",
                    }
                    .into(),
                    path: path.clone(),
                    curve: curve.clone(),
                })
            })
            .collect();
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
        let origin = ui.cursor_screen_pos();
        let size = ui.content_region_avail();
        let selected = state.selected.clone();
        let time = state::time(host);
        let response = self.animation.draw(
            ui,
            &mut channels,
            Context {
                cache: &host
                    .cache_reference()
                    .filter(|key| key.target.is_some_and(|(id, _)| id == document))
                    .map(|key| fold_ui::sdk::cache_indicator::frames(host.cached_previews(), &key))
                    .unwrap_or_default(),
                document,
                generation: state.generation,
                time,
                rate: info.rate,
                frames: info.frames,
                nodes: &selected,
                tracks: &tracks,
                auto_key: &mut state.auto_key,
            },
        );
        if let Some(id) = response.select_node {
            state.select_layer(host, id, ui.io().key_shift(), ui.io().key_ctrl());
        }
        if let Some(id) = response.open_node {
            if let Err(e) = state.open(host, id) {
                state.error = e;
            }
        }
        if let Some(time) = response.seek {
            host.command(DesktopCommand::Transport(TransportAction::Jump(
                (sdk::time_view::frame(time, info.rate).round() as u32)
                    .min(info.frames.saturating_sub(1)),
            )));
        }
        if response.edit.cancelled {
            state.cancel(host);
        } else {
            let c = state.composition.as_mut().unwrap();
            let mut immediate = false;
            if let Some((id, value)) = response.visibility {
                if !c.locked(id) {
                    if let Ok(l) = c.layer_mut(id) {
                        l.visible = value;
                        immediate = true;
                    }
                }
            }
            if let Some((id, value)) = response.lock {
                if let Ok(l) = c.layer_mut(id) {
                    l.locked = value;
                    immediate = true;
                }
            }
            if let Some((id, start, end)) = response.range {
                if !c.locked(id) {
                    if let Err(e) = c.layer_mut(id).and_then(|l| l.set_range(start, end)) {
                        state.error = e;
                    }
                }
            }
            let c = state.composition.as_mut().unwrap();
            if let Some((id, target, before)) = response.reorder {
                if !c.locked(id)
                    && !c.locked(target)
                    && c.layer(id).unwrap().parent == c.layer(target).unwrap().parent
                {
                    let from = c.layers.iter().position(|l| l.id == id).unwrap();
                    let layer = c.layers.remove(from);
                    if let Some(at) = c.layers.iter().position(|l| l.id == target) {
                        c.layers.insert(at + usize::from(!before), layer);
                        immediate = true;
                    } else {
                        c.layers.insert(from, layer);
                    }
                }
            }
            if let Some((id, parent)) = response.reparent {
                if !c.locked(id) && parent.is_none_or(|p| !c.locked(p)) {
                    let old = c.layer(id).unwrap().parent;
                    c.layer_mut(id).unwrap().parent = parent;
                    if c.validate().is_ok() {
                        immediate = true;
                    } else {
                        c.layer_mut(id).unwrap().parent = old;
                    }
                }
            }
            if response.edit.changed {
                for channel in channels {
                    if !c.locked(channel.object) {
                        let l = c.layer_mut(channel.object).unwrap();
                        if channel.curve.keys.is_empty() {
                            l.animation.remove(&channel.path);
                        } else {
                            l.animation.insert(channel.path, channel.curve);
                        }
                    }
                }
                state.preview(host);
            }
            if immediate || response.edit.finished {
                state.commit(host);
            }
        }
        if response.delete_layers {
            actions::delete_selected(host, &mut state);
        }
        let mut request = sdk::project_drop::placement(document, time);
        self.files.region(ui, origin, size, request.clone(), None);
        for &(layer, min, max) in self.animation.layer_drop_rows() {
            let mut row_request = request.clone();
            row_request.track = Some(layer);
            self.files.region(
                ui,
                min,
                [max[0] - min[0], max[1] - min[1]],
                row_request,
                None,
            );
            if sdk::project_drop::inside(ui.io().mouse_pos(), min, max) {
                request.track = Some(layer);
            }
        }
        if let Some(entry) = sdk::project_drop::canvas_target(ui, origin, size) {
            sdk::project_drop::place_request(host, entry, request);
        }
    }
}
