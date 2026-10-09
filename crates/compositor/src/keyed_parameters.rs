//! Compositor's animation adapter for the Composition editor.
use crate::Composite;
use fold_foundation::{ObjectId, Time};
use fold_platform::animation::{Channel, Target};
use fold_project::Document;

pub(crate) fn channels(document: &Document, _: Option<ObjectId>) -> Result<Vec<Channel>, String> {
    let graph = Composite::from_document(document)?;
    let mut result = vec![];
    for (index, node) in graph.nodes.iter().enumerate() {
        for property in node.animated_properties() {
            for (i, component) in property.channels.iter().enumerate() {
                let path = crate::animation::path(&property.id, i);
                if let Some(curve) = node.animation.curve(&path)? {
                    result.push(Channel {
                        target: Target {
                            group: None,
                            object: node.id,
                            path,
                        },
                        node_label: format!("{} {}", node.parameters.operator().label(), index + 1),
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
    let mut graph = Composite::from_document(document)?;
    for channel in channels {
        if channel.target.group.is_some() {
            return Err("Compositor channel has no group scope".into());
        }
        let node = graph.node_mut(channel.target.object)?;
        if channel.curve.keys.is_empty() {
            node.remove_animation_channel(&channel.target.path, time)?;
        } else {
            channel.curve.validate()?;
            node.animation
                .insert(channel.target.path.clone(), channel.curve.clone())?;
        }
    }
    let mut result = graph.document(document.id)?;
    result.extensions = document.extensions.clone();
    Ok(result)
}
