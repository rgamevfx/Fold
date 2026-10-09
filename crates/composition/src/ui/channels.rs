//! Map provider clocks and channel identities onto their owning layer rows.
use crate::Composition;
use fold_foundation::ObjectId;
use fold_ui::sdk::animation_editor::Channel;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct KeyIds(std::cell::RefCell<BTreeMap<(ObjectId, String, ObjectId), ObjectId>>);
type GraphChannels = BTreeMap<ObjectId, Vec<fold_platform::animation::Channel>>;
pub(super) struct Channels {
    pub rows: Vec<Channel>,
    bindings: Vec<(usize, ObjectId, usize)>,
    provider_keys: BTreeMap<ObjectId, ObjectId>,
}
impl Channels {
    pub fn collect(
        composition: &Composition,
        graph_channels: &GraphChannels,
        ids: &KeyIds,
    ) -> Result<Self, String> {
        let mut channels: Vec<_> = composition
            .layers
            .iter()
            .flat_map(|l| {
                l.animation.iter().map(move |(path, curve)| Channel {
                    object: l.id,
                    node_label: l.name.clone(),
                    property: if path == "scene.opacity.0" {
                        "opacity"
                    } else if path.starts_with("scene.scale.") {
                        "scale"
                    } else if path == "scene.rotation.0" {
                        "rotation"
                    } else if path == "transform.4" || path == "transform.5" {
                        "position"
                    } else {
                        "axes"
                    }
                    .into(),
                    property_label: if path == "scene.opacity.0" {
                        "Opacity"
                    } else if path.starts_with("scene.scale.") {
                        "Scale"
                    } else if path == "scene.rotation.0" {
                        "Rotation"
                    } else if path == "transform.4" || path == "transform.5" {
                        "Position"
                    } else {
                        "Transform axes"
                    }
                    .into(),
                    component: match path.as_str() {
                        "scene.opacity.0" | "scene.rotation.0" => "Value",
                        "scene.scale.0" => "X",
                        "scene.scale.1" => "Y",
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
        let mut graph_bindings = vec![];
        for layer in &composition.layers {
            if let Some(graph_channels) = graph_channels.get(&layer.id) {
                for (index, channel) in graph_channels
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| !c.curve.keys.is_empty())
                {
                    let mut displayed = channel.clone();
                    let shift = layer
                        .origin
                        .checked_sub(layer.source_start)
                        .map_err(|e| e.to_string());
                    if let Err(error) = shift.and_then(|delta| displayed.shift(delta)) {
                        return Err(error);
                    }
                    let path = format!("graph:{}", serde_json::to_string(&channel.target).unwrap());
                    graph_bindings.push((channels.len(), layer.id, index));
                    channels.push(Channel {
                        object: layer.id,
                        node_label: layer.name.clone(),
                        property: format!(
                            "graph:{:?}:{}:{}",
                            channel.target.group,
                            serde_json::to_string(&channel.target.object).unwrap(),
                            channel.property
                        ),
                        property_label: format!(
                            "{} — {}",
                            channel.property_label, channel.node_label
                        ),
                        component: if channel.component.is_empty() {
                            "Value".into()
                        } else {
                            channel.component.clone()
                        },
                        path,
                        curve: displayed.curve,
                    });
                }
            }
        }
        let mut provider_keys = BTreeMap::new();
        for channel in &mut channels {
            for key in &mut channel.curve.keys {
                let provider = key.id;
                key.id = *ids
                    .0
                    .borrow_mut()
                    .entry((channel.object, channel.path.clone(), provider))
                    .or_insert_with(ObjectId::new);
                provider_keys.insert(key.id, provider);
            }
        }
        Ok(Self {
            provider_keys,
            rows: channels,
            bindings: graph_bindings,
        })
    }
    pub fn project_objects(&mut self, hierarchy: &super::hierarchy::Projection) {
        for (index, layer, _) in &self.bindings {
            let channel = &mut self.rows[*index];
            let Some(path) = channel.path.strip_prefix("graph:") else {
                continue;
            };
            if let Ok(target) = serde_json::from_str::<fold_platform::animation::Target>(path)
                && let Some(object) = target.group
                && let Some(view) = hierarchy.view(*layer, object)
            {
                channel.object = view;
            }
        }
    }
    pub fn apply(
        mut self,
        composition: &mut Composition,
        graph_channels: &mut GraphChannels,
        time: fold_foundation::Time,
        ids: &KeyIds,
    ) -> Result<(), String> {
        // Build the whole proposal before publishing it, including exact-time failures.
        for (index, channel) in self.rows.iter_mut().enumerate() {
            let owner = self
                .bindings
                .iter()
                .find(|(i, _, _)| *i == index)
                .map_or(channel.object, |(_, layer, _)| *layer);
            for key in &mut channel.curve.keys {
                let ui_id = key.id;
                key.id = self.provider_keys.get(&ui_id).copied().unwrap_or(ui_id);
                ids.0
                    .borrow_mut()
                    .insert((owner, channel.path.clone(), key.id), ui_id);
            }
        }
        let mut proposal = composition.clone();
        let mut graphs = graph_channels.clone();
        for (index, channel) in self.rows.iter().enumerate() {
            let owner = self
                .bindings
                .iter()
                .find(|(i, _, _)| *i == index)
                .map_or(channel.object, |(_, layer, _)| *layer);
            if proposal.locked(owner) {
                continue;
            }
            if let Some((_, layer_id, graph_index)) =
                self.bindings.iter().find(|(i, _, _)| *i == index)
            {
                let layer = proposal.layer(*layer_id)?;
                let edited =
                    &mut graphs.get_mut(layer_id).ok_or("Missing animated layer")?[*graph_index];
                edited.curve = channel.curve.clone();
                edited.shift(layer.local_time(fold_foundation::Time::ZERO)?)?;
            } else {
                let layer = proposal.layer_mut(channel.object)?;
                if channel.curve.keys.is_empty() {
                    if let Some(value) = layer
                        .animation
                        .get(&channel.path)
                        .and_then(|c| c.sample(time))
                    {
                        if channel.path == "scene.opacity.0" {
                            layer.opacity = value.clamp(0., 1.);
                        } else if channel.path.starts_with("scene.scale.")
                            || channel.path == "scene.rotation.0"
                        {
                            let (mut scale, mut rotation, shear) =
                                crate::layer::components(layer.offset);
                            match channel.path.as_str() {
                                "scene.scale.0" => scale[0] = value,
                                "scene.scale.1" => scale[1] = value,
                                _ => rotation = value,
                            }
                            crate::layer::set_components(&mut layer.offset, scale, rotation, shear);
                        } else if let Some(index) = channel
                            .path
                            .strip_prefix("transform.")
                            .and_then(|s| s.parse::<usize>().ok())
                            && let Some(base) = layer.offset.get_mut(index)
                        {
                            *base = value;
                        }
                    }
                    layer.animation.remove(&channel.path);
                } else {
                    layer
                        .animation
                        .insert(channel.path.clone(), channel.curve.clone());
                }
            }
        }
        let mut changes = BTreeMap::new();
        for (layer_id, channels) in &graphs {
            let Some(fold_platform::image_source::Source::Output(source)) =
                &proposal.layer(*layer_id)?.source
            else {
                continue;
            };
            for (index, channel) in channels.iter().enumerate() {
                if channel.curve == graph_channels[layer_id][index].curve {
                    continue;
                }
                let target = (source.document, channel.target.clone());
                if let Some(existing) = changes.get(&target) {
                    // Independent insertions at the same time may allocate different IDs.
                    let mut equivalent = channel.curve.clone();
                    let existing: &fold_animation::Curve = existing;
                    for (a, b) in equivalent.keys.iter_mut().zip(&existing.keys) {
                        a.id = b.id;
                    }
                    if &equivalent != existing {
                        return Err("Shared source received conflicting key edits".into());
                    }
                } else {
                    changes.insert(target, channel.curve.clone());
                }
            }
        }
        for (layer_id, channels) in &mut graphs {
            let Some(fold_platform::image_source::Source::Output(source)) =
                &proposal.layer(*layer_id)?.source
            else {
                continue;
            };
            for (index, channel) in channels.iter_mut().enumerate() {
                // Keep the editing owner and clock. Other aliases refresh on commit;
                // rewriting them here would turn gesture baselines into extra edits.
                if channel.curve == graph_channels[layer_id][index].curve {
                    continue;
                }
                if let Some(curve) = changes.get(&(source.document, channel.target.clone())) {
                    channel.curve = curve.clone();
                }
            }
        }
        *composition = proposal;
        *graph_channels = graphs;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::Time;
    #[test]
    fn shared_source_keys_are_scoped_to_layer_and_range_moves_do_not_retime_source() {
        use fold_platform::image_source::Source;
        let source = Source::Output(fold_project::DocumentRef {
            document: fold_foundation::DocumentId::new(),
            output: "video".into(),
            extensions: Default::default(),
        });
        let mut c = Composition::default();
        let a = crate::Layer::new("A", Some(source.clone()), c.info.clone()).unwrap();
        let mut b = crate::Layer::new("B", Some(source), c.info.clone()).unwrap();
        b.origin = Time::new(2, 1).unwrap();
        let (a_id, b_id) = (a.id, b.id);
        c.layers = vec![a, b];
        let mut curve = fold_animation::Curve::default();
        let provider_id = curve.insert(Time::ZERO, 120.);
        let channel = fold_platform::animation::Channel {
            target: fold_platform::animation::Target {
                group: None,
                object: ObjectId::new(),
                path: "radius.0".into(),
            },
            node_label: "Orbit".into(),
            property: "radius".into(),
            property_label: "Radius".into(),
            component: "Value".into(),
            curve,
        };
        let mut graphs = BTreeMap::from([(a_id, vec![channel.clone()]), (b_id, vec![channel])]);
        let original = graphs.clone();
        let ids = KeyIds::default();
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        let selected = projected.rows[0].curve.keys[0].id;
        assert_ne!(selected, projected.rows[1].curve.keys[0].id);
        let delta = Time::new(1, 1).unwrap();
        for key in projected
            .rows
            .iter_mut()
            .flat_map(|c| &mut c.curve.keys)
            .filter(|k| k.id == selected)
        {
            key.time = key.time.checked_add(delta).unwrap();
        }
        let end = c.layers[0].end.checked_add(delta).unwrap();
        c.layers[0].set_range(delta, end).unwrap();
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert_eq!(
            graphs, original,
            "moving a layer must not move its source keys or another layer's keys"
        );
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        assert_eq!(projected.rows[0].curve.keys[0].id, selected);
        projected.rows[0].curve.keys[0].value = 150.;
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert_eq!(graphs[&a_id][0].curve.keys[0].value, 150.);
        assert_eq!(graphs[&a_id][0].curve.keys[0].id, provider_id);
        assert_eq!(
            graphs[&b_id], original[&b_id],
            "unselected alias must not become an edit with another source clock"
        );
        // A drag keeps the mouse-down rows as its baseline over multiple frames.
        for value in [160., 120.] {
            let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
            projected.rows[0].curve.keys[0].value = value;
            projected.rows[1].curve.keys[0].value = 120.;
            projected
                .apply(&mut c, &mut graphs, Time::ZERO, &ids)
                .unwrap();
            assert_eq!(graphs[&a_id][0].curve.keys[0].value, value);
            assert_eq!(graphs[&b_id], original[&b_id]);
        }
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        projected.rows[0].curve.keys.clear();
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert!(graphs[&a_id][0].curve.keys.is_empty());
        assert_eq!(
            graphs[&b_id], original[&b_id],
            "last-key removal must use only the editing layer's clock"
        );
    }
    #[test]
    fn graph_keys_live_under_layer_and_round_trip_exact_source_time() {
        let ids = KeyIds::default();
        let mut c = Composition::default();
        let mut layer = crate::Layer::new("Layer", None, c.info.clone()).unwrap();
        layer.origin = Time::new(7, 3).unwrap();
        layer.source_start = Time::new(1, 5).unwrap();
        let id = layer.id;
        let mut curve = fold_animation::Curve::default();
        curve.insert(Time::new(1, 7).unwrap(), 3.);
        let channel = fold_platform::animation::Channel {
            target: fold_platform::animation::Target {
                group: None,
                object: ObjectId::new(),
                path: "width.0".into(),
            },
            node_label: "Rectangle".into(),
            property: "width".into(),
            property_label: "Width".into(),
            component: "Value".into(),
            curve: curve.clone(),
        };
        c.layers.push(layer);
        let mut graphs = BTreeMap::from([(id, vec![channel])]);
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        assert_eq!(projected.rows.len(), 1);
        assert_eq!(projected.rows[0].object, id);
        assert_eq!(projected.rows[0].property_label, "Width — Rectangle");
        let expected = Time::new(1, 7)
            .unwrap()
            .checked_add(Time::new(7, 3).unwrap())
            .unwrap()
            .checked_sub(Time::new(1, 5).unwrap())
            .unwrap();
        assert_eq!(projected.rows[0].curve.keys[0].time, expected);
        projected.rows[0].curve.keys[0].time =
            expected.checked_add(Time::new(1, 24).unwrap()).unwrap();
        projected.rows[0].curve.keys[0].value = 9.;
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert_eq!(
            graphs[&id][0].curve.keys[0].time,
            Time::new(1, 7)
                .unwrap()
                .checked_add(Time::new(1, 24).unwrap())
                .unwrap()
        );
        assert_eq!(graphs[&id][0].curve.keys[0].value, 9.);
        assert!(
            c.layers[0].animation.is_empty(),
            "graph keys remain provider-owned"
        );
        c.layers[0].locked = true;
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        projected.rows[0].curve.keys.clear();
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert_eq!(graphs[&id][0].curve.keys.len(), 1);
        c.layers[0].locked = false;
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        projected.rows[0].curve.keys.clear();
        projected
            .apply(&mut c, &mut graphs, Time::ZERO, &ids)
            .unwrap();
        assert!(
            Channels::collect(&c, &graphs, &ids)
                .unwrap()
                .rows
                .is_empty()
        );
        let mut opacity = fold_animation::Curve::default();
        opacity.insert(Time::ZERO, 0.2);
        opacity.insert(Time::new(1, 1).unwrap(), 0.8);
        c.layers[0]
            .animation
            .insert("scene.opacity.0".into(), opacity);
        let mut projected = Channels::collect(&c, &graphs, &ids).unwrap();
        projected.rows[0].curve.keys.clear();
        projected
            .apply(&mut c, &mut graphs, Time::new(1, 2).unwrap(), &ids)
            .unwrap();
        assert_eq!(
            c.layers[0].opacity, 0.5,
            "deleting the last keys retains the evaluated value"
        );
        assert!(c.layers[0].animation.is_empty());
    }
}
