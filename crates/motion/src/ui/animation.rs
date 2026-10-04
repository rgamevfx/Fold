use super::state::Shared;
use crate::{
    animation::{Track, parameters::components},
    graph::{Input, registry},
};
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopCommand, Selection, TransportAction};
use fold_ui::sdk::{
    ExtensionUi,
    animation_editor::{Channel, Context, Editor},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Animation {
    editor: Editor,
    generation: u64,
    legacy: BTreeMap<ObjectId, Vec<fold_animation::Curve>>,
}
impl Animation {
    pub fn draw(&mut self, shared: &Shared, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = shared.borrow_mut();
        state.sync(host);
        if self.generation != state.generation {
            self.legacy.clear();
            self.editor.reset_gesture();
            self.generation = state.generation;
        }
        let Some(document) = state.document else {
            return;
        };
        let Some(mut motion) = state.view_motion() else {
            return;
        };
        let mut channels = vec![];
        for (index, node) in motion.graph.nodes.iter().enumerate() {
            let name = registry::find(&node.kind)
                .map(|d| d.name)
                .unwrap_or("Unavailable node");
            let label = format!("{name} {}", index + 1);
            for (input, value) in &node.inputs {
                let Input::Value(value) = value else {
                    continue;
                };
                let Some((names, _)) = components(value) else {
                    continue;
                };
                for (i, component) in names.iter().enumerate() {
                    let path = format!("{input}.{i}");
                    if let Some(curve) = node.animation.get(&path) {
                        channels.push(Channel {
                            object: node.id,
                            node_label: label.clone(),
                            property: input.clone(),
                            property_label: super::inspector::property_label(input),
                            component: (*component).into(),
                            path,
                            curve: curve.clone(),
                        });
                    }
                }
            }
            if node.kind == "fold.motion.keyframes"
                && let Ok(track) = node.settings::<Track>()
            {
                let curves = self
                    .legacy
                    .entry(node.id)
                    .or_insert_with(|| track.scalar_curves());
                let names = components(&track.keys[0].value).unwrap().0;
                for (i, curve) in curves.iter().enumerate() {
                    if curve.keys.is_empty() {
                        continue;
                    }
                    channels.push(Channel {
                        object: node.id,
                        node_label: label.clone(),
                        property: "value".into(),
                        property_label: "Value".into(),
                        component: names[i].into(),
                        path: format!("driver.{i}"),
                        curve: curve.clone(),
                    });
                }
            }
        }
        let selected = state.selected.clone();
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        let response = self.editor.draw(
            ui,
            &mut channels,
            Context {
                document,
                generation: state.generation,
                time,
                rate: motion.info.rate,
                frames: motion.info.frames,
                nodes: &selected,
                auto_key: &mut state.auto_key,
            },
        );
        if let Some(node) = response.select_node {
            state.selected = vec![node];
            host.command(DesktopCommand::Select(Selection {
                document: Some(document),
                objects: vec![node],
            }));
        }
        if let Some(time) = response.seek {
            host.command(DesktopCommand::Transport(TransportAction::Jump(
                fold_ui::sdk::time_view::frame(time, motion.info.rate).round() as u32,
            )));
        }
        if response.edit.cancelled {
            state.cancel(host);
        } else {
            if response.edit.changed {
                for channel in channels {
                    if let Some(node) = motion
                        .graph
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == channel.object)
                    {
                        if let Some(component) = channel
                            .path
                            .strip_prefix("driver.")
                            .and_then(|c| c.parse::<usize>().ok())
                        {
                            let curves = self.legacy.get_mut(&channel.object).unwrap();
                            if channel.curve.keys.is_empty() {
                                let mut track = node.settings::<Track>().unwrap();
                                let (_, mut base) = components(&track.keys[0].value).unwrap();
                                base[component] =
                                    curves[component].sample(time).unwrap_or(base[component]);
                                crate::animation::parameters::set_components(
                                    &mut track.keys[0].value,
                                    &base,
                                );
                                node.settings["keys"] = serde_json::to_value(track.keys).unwrap();
                            }
                            curves[component] = channel.curve;
                            node.settings["channels"] = serde_json::to_value(curves).unwrap();
                        } else if channel.curve.keys.is_empty() {
                            node.remove_animation_channel(&channel.path, time);
                        } else {
                            node.animation.insert(channel.path, channel.curve);
                        }
                    }
                }
                for node in &mut motion.graph.nodes {
                    if node.kind == "fold.motion.keyframes"
                        && let Ok(track) = node.settings::<Track>()
                        && !track.channels.is_empty()
                        && track.channels.iter().all(|c| c.keys.is_empty())
                    {
                        let value = track.sample(time).unwrap();
                        node.kind = "fold.motion.value".into();
                        node.settings.as_object_mut().unwrap().remove("keys");
                        node.settings.as_object_mut().unwrap().remove("channels");
                        node.settings["value_type"] = serde_json::to_value(value.kind()).unwrap();
                        node.set("value", value);
                    }
                }
                state.replace_view(motion);
                state.preview(host);
            }
            if response.edit.finished && state.editing {
                state.commit(host);
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
