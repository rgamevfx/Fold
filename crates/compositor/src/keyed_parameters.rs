//! Compositor's animation adapter for the Composition editor.
use crate::Composite;
use fold_foundation::{ObjectId, Time};
use fold_graph::binding::Binding;
use fold_platform::animation::{Channel, Target};
use fold_project::Document;

pub(crate) fn channels(document: &Document, _: Option<ObjectId>) -> Result<Vec<Channel>, String> {
    let native = Composite::read_document(document)?;
    let graph = native.prepare_document(document)?;
    let mut result = vec![];
    for (index, node) in graph.nodes.iter().enumerate() {
        if let Some(instance) = node.instance.as_ref() {
            for (name, parameter) in &instance.overrides {
                let Binding::List { values } = parameter.binding()? else {
                    return Err("Invalid instance override".into());
                };
                for (component, value) in values.into_iter().enumerate() {
                    if let Binding::Track { track } = value {
                        result.push(Channel {
                            target: Target {
                                group: Some(instance.id),
                                object: node.id,
                                path: format!(
                                    "instance:{}:{component}",
                                    serde_json::to_string(name).map_err(|e| e.to_string())?
                                ),
                            },
                            node_label: node.name.clone(),
                            property: name.clone(),
                            property_label: name.clone(),
                            component: component.to_string(),
                            curve: fold_graph::curves::curves(&track)?.remove(0).1,
                        });
                    }
                }
            }
        }
        for property in node.animated_properties() {
            for (i, component) in property.channels.iter().enumerate() {
                let path = crate::animation::path(&property.id, i);
                if let Some(curve) = node.tracks().curve(&path)? {
                    result.push(Channel {
                        target: Target {
                            group: None,
                            object: node.id,
                            path,
                        },
                        node_label: format!("{} {}", node.config().operator().label(), index + 1),
                        property: property.id.clone(),
                        property_label: property.label.clone(),
                        component: (*component).into(),
                        curve: curve.clone(),
                    });
                }
            }
        }
    }
    Ok(result)
}
pub(crate) fn edit(
    document: &Document,
    channels: &[Channel],
    time: Time,
) -> Result<Document, String> {
    let before = Composite::from_document(document)?;
    let mut graph = before.clone();
    for channel in channels {
        let node = graph.node_mut(channel.target.object)?;
        if let Some(group) = channel.target.group {
            let mut instance = node.native_instance()?.ok_or("Missing effect instance")?;
            if instance.id != group {
                return Err("Effect occurrence identity changed".into());
            }
            let (name, component) = channel
                .target
                .path
                .strip_prefix("instance:")
                .and_then(|s| s.rsplit_once(':'))
                .ok_or("Invalid instance channel path")?;
            let name: String = serde_json::from_str(name).map_err(|e| e.to_string())?;
            let component: usize = component
                .parse()
                .map_err(|_| "Invalid instance component")?;
            let parameter = instance
                .overrides
                .get_mut(&name)
                .ok_or("Missing instance override")?;
            let Binding::List { mut values } = parameter.binding()? else {
                return Err("Invalid instance override".into());
            };
            let value = values
                .get_mut(component)
                .ok_or("Missing instance component")?;
            let Binding::Track { track } = value else {
                return Err("Instance component is not animated".into());
            };
            *value = if channel.curve.keys.is_empty() {
                Binding::Literal {
                    value: track.sample(time)?,
                }
            } else {
                Binding::Track {
                    track: fold_graph::curves::replace(
                        track,
                        &[(String::new(), channel.curve.clone())],
                    )?,
                }
            };
            parameter.value =
                serde_json::to_value(Binding::List { values }).map_err(|e| e.to_string())?;
            node.set_native_instance(instance)?;
            continue;
        }
        if channel.curve.keys.is_empty() {
            node.remove_animation_channel(&channel.target.path, time)?;
        } else {
            channel.curve.validate()?;
            node.animation
                .insert(channel.target.path.clone(), channel.curve.clone())?;
        }
    }
    before.check_protection(&graph)?;
    let mut result = graph.document(document.id)?;
    result.extensions = document.extensions.clone();
    Ok(result)
}
