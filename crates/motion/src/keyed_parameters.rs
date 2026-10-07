//! Motion's keyed channels, scoped to a layer construction, including modifiers.
use crate::{
    Motion,
    animation::{Track, parameters::components},
    graph::{Graph, Input},
    nodes::interface::GroupSettings,
};
use fold_foundation::{ObjectId, Time};
use fold_platform::animation::{Channel, Target};
use fold_project::Document;
use std::collections::BTreeSet;

pub(crate) fn label(key: &str) -> String {
    if key == "domain" {
        return "Scope".into();
    }
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
pub(crate) fn input_label(motion: &Motion, node: &crate::graph::Node, key: &str) -> String {
    node.settings::<GroupSettings>()
        .ok()
        .and_then(|s| s.group)
        .and_then(|id| motion.groups.get(&id))
        .and_then(|g| g.inputs.iter().find(|p| p.id == key))
        .map(|p| label(&p.name))
        .unwrap_or_else(|| label(key))
}
pub(crate) fn channels(
    document: &Document,
    object: Option<ObjectId>,
) -> Result<Vec<Channel>, String> {
    let motion = Motion::from_document(document)?;
    let visible = object.map(|id| crate::authoring::network_scope::members(&motion, id));
    let mut result = vec![];
    collect(
        &motion,
        &motion.graph,
        None,
        visible.as_ref(),
        "",
        &mut BTreeSet::new(),
        &mut result,
    )?;
    if let Some(scene) = &motion.scene {
        for o in scene
            .objects
            .iter()
            .filter(|o| object.is_none_or(|id| id == o.id))
        {
            for (id, name, animation) in o
                .constraints
                .iter()
                .map(|c| (c.id, c.kind.label(), &c.animation))
                .chain(o.masks.iter().map(|m| (m.id, "Mask", &m.animation)))
            {
                for (path, curve) in animation {
                    let property = path.strip_suffix(".0").unwrap_or(path);
                    result.push(Channel {
                        target: Target {
                            group: None,
                            object: id,
                            path: path.clone(),
                        },
                        node_label: name.into(),
                        property: property.into(),
                        property_label: label(property),
                        component: "Value".into(),
                        curve: curve.clone(),
                    });
                }
            }
        }
    }
    Ok(result)
}
fn collect(
    motion: &Motion,
    graph: &Graph,
    group: Option<ObjectId>,
    visible: Option<&BTreeSet<ObjectId>>,
    prefix: &str,
    visited: &mut BTreeSet<ObjectId>,
    result: &mut Vec<Channel>,
) -> Result<(), String> {
    for (index, node) in graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| visible.is_none_or(|v| v.contains(&n.id)))
    {
        let definition = node
            .settings::<GroupSettings>()
            .ok()
            .and_then(|s| s.group)
            .and_then(|id| motion.groups.get(&id).map(|g| (id, g)));
        let name = definition.map(|(_, g)| g.name.as_str()).unwrap_or_else(|| {
            crate::graph::registry::find(&node.kind)
                .map(|d| d.name)
                .unwrap_or("Node")
        });
        let name = format!("{prefix}{name} {}", index + 1);
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
                    result.push(Channel {
                        target: Target {
                            group,
                            object: node.id,
                            path,
                        },
                        node_label: name.clone(),
                        property: input.clone(),
                        property_label: input_label(motion, node, input),
                        component: (*component).into(),
                        curve: curve.clone(),
                    });
                }
            }
        }
        if node.kind == "fold.motion.keyframes" {
            let track = node.settings::<Track>()?;
            track.validate()?;
            let names = components(&track.keys[0].value)
                .ok_or("Unsupported driver value")?
                .0;
            for (i, curve) in track
                .scalar_curves()
                .into_iter()
                .enumerate()
                .filter(|(_, c)| !c.keys.is_empty())
            {
                result.push(Channel {
                    target: Target {
                        group,
                        object: node.id,
                        path: format!("driver.{i}"),
                    },
                    node_label: name.clone(),
                    property: "value".into(),
                    property_label: "Value".into(),
                    component: names[i].into(),
                    curve,
                });
            }
        }
        if let Some((id, definition)) = definition
            && visited.insert(id)
        {
            collect(
                motion,
                &definition.graph,
                Some(id),
                None,
                &format!("{name} / "),
                visited,
                result,
            )?;
        }
    }
    Ok(())
}
pub(crate) fn edit(
    document: &Document,
    channels: &[Channel],
    time: Time,
) -> Result<Document, String> {
    let mut motion = Motion::from_document(document)?;
    for channel in channels {
        if !channel.curve.keys.is_empty() {
            channel.curve.validate()?;
        }
        if channel.target.group.is_none()
            && let Some((base, animation)) = motion.scene.as_mut().and_then(|s| {
                crate::scene::animation::channel_mut(s, channel.target.object, &channel.target.path)
            })
        {
            if channel.curve.keys.is_empty() {
                *base = animation
                    .get(&channel.target.path)
                    .and_then(|c| c.sample(time))
                    .unwrap_or(*base);
                animation.remove(&channel.target.path);
            } else {
                animation.insert(channel.target.path.clone(), channel.curve.clone());
            }
            continue;
        }
        let graph = match channel.target.group {
            Some(id) => {
                &mut motion
                    .groups
                    .get_mut(&id)
                    .ok_or("Missing modifier graph")?
                    .graph
            }
            None => &mut motion.graph,
        };
        let node = graph
            .nodes
            .iter_mut()
            .find(|n| n.id == channel.target.object)
            .ok_or("Missing animated node")?;
        if let Some(component) = channel
            .target
            .path
            .strip_prefix("driver.")
            .and_then(|s| s.parse::<usize>().ok())
        {
            let mut track = node.settings::<Track>()?;
            let mut curves = track.scalar_curves();
            let curve = curves
                .get_mut(component)
                .ok_or("Missing driver component")?;
            if channel.curve.keys.is_empty() {
                let (_, mut values) =
                    components(&track.keys[0].value).ok_or("Unsupported driver value")?;
                values[component] = curve.sample(time).unwrap_or(values[component]);
                crate::animation::parameters::set_components(&mut track.keys[0].value, &values);
            }
            *curve = channel.curve.clone();
            node.settings["keys"] = serde_json::to_value(track.keys).map_err(|e| e.to_string())?;
            node.settings["channels"] = serde_json::to_value(curves).map_err(|e| e.to_string())?;
        } else if channel.curve.keys.is_empty() {
            node.remove_animation_channel(&channel.target.path, time);
        } else {
            node.animation
                .insert(channel.target.path.clone(), channel.curve.clone());
        }
    }
    let mut result = motion.document(document.id)?;
    result.extensions = document.extensions.clone();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_provider_does_not_author_composition_layer_channels() {
        let mut motion = Motion::new_scene();
        let object =
            crate::authoring::scene::create_object(&mut motion, "fold.motion.ellipse").unwrap();
        let mut curve = fold_animation::Curve::default();
        curve.insert(Time::ZERO, 0.25);
        // Evaluation state supplied by Composition is not graph-authored animation.
        motion.scene.as_mut().unwrap().objects[0]
            .animation
            .insert("scene.opacity.0".into(), curve.clone());
        let document = motion.document(fold_foundation::DocumentId::new()).unwrap();
        assert!(channels(&document, Some(object)).unwrap().is_empty());
        assert!(channels(&document, None).unwrap().is_empty());
        let proposed = Channel {
            target: Target {
                group: None,
                object,
                path: "scene.opacity.0".into(),
            },
            node_label: "Layer".into(),
            property: "opacity".into(),
            property_label: "Opacity".into(),
            component: "Value".into(),
            curve,
        };
        assert!(edit(&document, &[proposed], Time::ZERO).is_err());
        assert!(
            crate::automation::author(
                &document,
                &[serde_json::json!({
                    "op":"object_properties", "object":object, "value":{"opacity":0.5}
                })]
            )
            .is_err()
        );
    }
}
