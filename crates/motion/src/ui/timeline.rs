use super::state::State;
use fold_foundation::{ObjectId, Rounding, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_ui::sdk::{
    animation_editor::{self, Context, Track},
    imgui::Ui,
};

pub(super) struct Timeline {
    editor: animation_editor::Editor,
    definition: Option<ObjectId>,
    time: Time,
}
impl Default for Timeline {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            definition: None,
            time: Time::ZERO,
        }
    }
}
impl Timeline {
    pub fn draw(&mut self, ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
        let (Some(document), Some(construction)) = (state.document, state.construction.clone())
        else {
            return;
        };
        let selected = host.state().selection.objects.clone();
        let definition = construction
            .definitions
            .iter()
            .find(|d| d.nodes.iter().any(|n| selected.contains(&n.id)))
            .or_else(|| {
                construction.definitions.iter().find(|d| {
                    d.id == state
                        .graph_definition
                        .unwrap_or(construction.root.definition)
                })
            });
        let Some(definition) = definition else {
            return;
        };
        if self.definition != Some(definition.id) {
            self.editor = Default::default();
            self.definition = Some(definition.id);
            self.time = Time::ZERO;
        }
        self.editor.set_linear_only(true);
        self.editor.show_auto_key(false);
        ui.text_disabled(format!("{} · Asset time", definition.name));
        let mut native = match crate::keyed::channels(&construction, Some(definition.id)) {
            Ok(channels) => channels,
            Err(error) => {
                ui.text_wrapped(error);
                return;
            }
        };
        let track_count = definition
            .nodes
            .iter()
            .flat_map(|node| node.parameters.values())
            .filter(|parameter| {
                matches!(
                    parameter.binding(),
                    Ok(crate::binding::Binding::Track { .. })
                )
            })
            .count();
        if track_count > native.len() {
            ui.text_wrapped("Select a typed-track node to edit its keys in the Inspector.");
        }
        let locks = match crate::protection::locked_ids(&construction) {
            Ok(locks) => locks,
            Err(error) => {
                ui.text_wrapped(error.reason);
                return;
            }
        };
        let locked_properties: std::collections::BTreeSet<_> =
            locks.iter().map(ToString::to_string).collect();
        let locked_nodes: std::collections::BTreeSet<_> = native
            .iter()
            .filter(|channel| locked_properties.contains(&channel.property))
            .map(|channel| channel.target.object)
            .collect();
        let tracks: Vec<_> = definition
            .nodes
            .iter()
            .map(|node| Track {
                id: node.id,
                parent: None,
                label: node.name.clone(),
                range: None,
                layer: None,
                locked: locks.contains(&node.id) || locked_nodes.contains(&node.id),
            })
            .collect();
        let mut channels: Vec<_> = native
            .iter()
            .map(|channel| animation_editor::Channel {
                object: channel.target.object,
                node_label: channel.node_label.clone(),
                property: channel.property.clone(),
                property_label: channel.property_label.clone(),
                component: channel.component.clone(),
                path: channel.target.path.clone(),
                curve: channel.curve.clone(),
            })
            .collect();
        let frames = construction
            .canvas
            .end
            .to_ticks(
                construction.canvas.rate[0],
                construction.canvas.rate[1],
                Rounding::Ceil,
            )
            .ok()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(1)
            .max(1);
        let mut auto_key = false;
        let response = self.editor.draw(
            ui,
            &mut channels,
            Context {
                cache: &Default::default(),
                document,
                generation: state.base.0,
                time: self.time,
                rate: construction.canvas.rate,
                frames,
                nodes: &selected,
                tracks: &tracks,
                auto_key: &mut auto_key,
            },
        );
        if let Some(time) = response.seek {
            self.time = time;
        }
        state.key_time = Some((definition.id, self.time));
        if let Some(id) = response.select_node.or(response.open_node) {
            host.command(DesktopCommand::Select(Selection {
                document: Some(document),
                objects: vec![id],
            }));
        }
        if response.edit.cancelled {
            state.cancel(host);
            return;
        }
        if response.edit.changed {
            for (native, channel) in native.iter_mut().zip(channels) {
                native.curve = channel.curve;
            }
            match crate::keyed::replace(&construction, &native) {
                Ok(after) => {
                    state.construction = Some(after);
                    state.preview(host);
                    state.error.clear();
                }
                Err(error) => {
                    state.error = error;
                    state.cancel(host);
                    self.editor.reset_gesture();
                    return;
                }
            }
        }
        if response.edit.finished
            && state.editing
            && let Some(after) = state.construction.clone()
        {
            state.commit(host, after);
        }
    }
}
