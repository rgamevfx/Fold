use super::state::Shared;
use crate::{
    animation::{Track, parameters::components},
    graph::{Input, registry},
};
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopCommand, Selection, TransportAction};
use fold_ui::sdk::{
    ExtensionUi,
    animation_editor::{Channel, Context, Editor, Track as RowTrack},
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
        self.draw_view(shared, context, false);
    }
    pub fn draw_scoped(&mut self, shared: &Shared, context: ExtensionUi<'_>) {
        self.draw_view(shared, context, true);
    }
    fn draw_view(&mut self, shared: &Shared, context: ExtensionUi<'_>, scoped: bool) {
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
        let Some(mut motion) = (if scoped {
            state.view_motion()
        } else {
            state.motion.clone()
        }) else {
            return;
        };
        let mut channels = vec![];
        let mut tracks = vec![];
        if !scoped && let Some(scene) = &motion.scene {
            for object in &scene.objects {
                tracks.push(RowTrack {
                    id: object.id,
                    parent: object.parent,
                    label: object.name.clone(),
                    layer: Some(fold_ui::sdk::animation_editor::LayerControls {
                        accepts_children: motion
                            .graph
                            .node(object.source.node)
                            .is_ok_and(|n| n.kind == "fold.motion.scene_children"),
                        visible: object.visible,
                        icon: match motion
                            .graph
                            .node(object.source.node)
                            .map(|n| n.kind.as_str())
                            .unwrap_or("")
                        {
                            "fold.motion.text" => "T",
                            "fold.motion.scene_children" => "G",
                            "fold.motion.path" => "P",
                            "fold.motion.group_instance" => "N",
                            _ => "S",
                        },
                    }),
                    locked: object.locked,
                    range: Some((object.start, object.end)),
                });
                for (id, label) in object
                    .transform
                    .iter()
                    .map(|&id| (id, "Transform".to_string()))
                    .chain(
                        object
                            .appearance
                            .iter()
                            .map(|&id| (id, "Appearance".into())),
                    )
                    .chain(object.modifiers.iter().map(|&id| {
                        let label = motion
                            .graph
                            .node(id)
                            .ok()
                            .and_then(|n| {
                                n.settings::<crate::nodes::interface::GroupSettings>().ok()
                            })
                            .and_then(|s| s.group)
                            .and_then(|id| motion.groups.get(&id))
                            .map(|g| g.name.clone())
                            .unwrap_or("Modifier".into());
                        (id, label)
                    }))
                {
                    tracks.push(RowTrack {
                        id,
                        parent: Some(object.id),
                        label,
                        locked: object.locked,
                        range: None,
                        layer: None,
                    });
                }
                for (id, label) in object
                    .masks
                    .iter()
                    .map(|m| (m.id, "Mask".to_string()))
                    .chain(
                        object
                            .constraints
                            .iter()
                            .map(|c| (c.id, c.kind.label().to_string())),
                    )
                {
                    tracks.push(RowTrack {
                        id,
                        parent: Some(object.id),
                        label,
                        locked: object.locked,
                        range: None,
                        layer: None,
                    });
                }
            }
        }

        if !scoped && let Some(scene) = &motion.scene {
            for object in &scene.objects {
                for (id, label, animation) in
                    std::iter::once((object.id, object.name.clone(), &object.animation))
                        .chain(
                            object
                                .constraints
                                .iter()
                                .map(|c| (c.id, c.kind.label().to_string(), &c.animation)),
                        )
                        .chain(
                            object
                                .masks
                                .iter()
                                .map(|m| (m.id, "Mask".to_string(), &m.animation)),
                        )
                {
                    for (path, curve) in animation {
                        let property = path.strip_suffix(".0").unwrap_or(path).to_string();
                        let property_label = match property.as_str() {
                            "scene.opacity" | "opacity" => "Opacity",
                            "progress" => "Path progress",
                            "influence" => "Influence",
                            "feather" => "Feather",
                            _ => &property,
                        }
                        .to_string();
                        channels.push(Channel {
                            object: id,
                            node_label: label.clone(),
                            property,
                            property_label,
                            component: "Value".into(),
                            path: path.clone(),
                            curve: curve.clone(),
                        });
                    }
                }
            }
        }

        for (index, node) in motion.graph.nodes.iter().enumerate() {
            if !scoped && motion.scene.is_some() && !tracks.iter().any(|t| t.id == node.id) {
                continue;
            }
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
                            property_label: super::input_label(&motion, node, input),
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
        fn ordered(
            parent: Option<ObjectId>,
            all: &[RowTrack],
            result: &mut Vec<RowTrack>,
            depth: usize,
        ) {
            if depth > all.len() {
                return;
            }
            for track in all.iter().filter(|t| t.parent == parent) {
                result.push(track.clone());
                ordered(Some(track.id), all, result, depth + 1);
            }
        }
        let mut sorted = vec![];
        ordered(None, &tracks, &mut sorted, 0);
        let tracks = sorted;
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
                tracks: &tracks,
                auto_key: &mut state.auto_key,
            },
        );
        if response.visibility.is_some() || response.lock.is_some() {
            state.change_scene(host, |m| {
                if let Some((id, visible)) = response.visibility {
                    let o = m
                        .scene
                        .as_mut()
                        .unwrap()
                        .objects
                        .iter_mut()
                        .find(|o| o.id == id)
                        .ok_or("missing object")?;
                    if o.locked {
                        return Err("unlock this object before changing visibility".into());
                    }
                    o.visible = visible;
                }
                if let Some((id, locked)) = response.lock {
                    m.scene
                        .as_mut()
                        .unwrap()
                        .objects
                        .iter_mut()
                        .find(|o| o.id == id)
                        .ok_or("missing object")?
                        .locked = locked;
                }
                Ok(None)
            });
            return;
        }
        if let Some(node) = response.select_node {
            if !scoped {
                state.group = None;
                state.parents.clear();
            }
            state.selected = vec![node];
            if !scoped {
                state.scene_scope = state.motion.as_ref().and_then(|m| {
                    m.scene
                        .as_ref()?
                        .objects
                        .iter()
                        .find(|o| o.owns(node))?
                        .parent
                });
                state.source_history.clear();
            }
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
            if let Some((source, parent)) = response.reparent {
                state.change_scene(host, |m| {
                    crate::authoring::scene::place_object(m, source, parent, None, time)?;
                    Ok(Some(source))
                });
                return;
            }
            if let Some((source, target, after)) = response.reorder {
                state.change_scene(host, |m| {
                    crate::authoring::scene::drop_beside(m, source, target, after, time)?;
                    Ok(Some(source))
                });
                return;
            }
            if response.edit.changed {
                if let Some((id, start, end)) = response.range
                    && let Some(object) = motion
                        .scene
                        .as_mut()
                        .and_then(|s| s.objects.iter_mut().find(|o| o.id == id))
                {
                    object.start = start;
                    object.end = end;
                }
                for channel in channels {
                    if let Some((base, animation)) = motion.scene.as_mut().and_then(|s| {
                        crate::scene::animation::channel_mut(s, channel.object, &channel.path)
                    }) {
                        if channel.curve.keys.is_empty() {
                            *base = animation
                                .get(&channel.path)
                                .and_then(|c| c.sample(time))
                                .unwrap_or(*base);
                            animation.remove(&channel.path);
                        } else {
                            animation.insert(channel.path, channel.curve);
                        }
                        continue;
                    }
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
                if scoped {
                    state.replace_view(motion);
                } else {
                    state.motion = Some(motion);
                }
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
