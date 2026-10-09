//! Typed tracks and instance overrides projected into the shared curve editor.
use crate::{binding::Binding, model::Construction};
use fold_foundation::ObjectId;
use fold_platform::animation::{Channel, Target};
use std::collections::BTreeSet;

/// Animate a parameter in place. Its identity and metadata remain unchanged.
pub fn animate(
    construction: &Construction,
    parameter: ObjectId,
    time: fold_foundation::Time,
) -> Result<Construction, String> {
    construction.validate_structure()?;
    let mut result = construction.clone();
    let mut found = false;
    for definition in &mut result.definitions {
        let Some(node) = definition
            .nodes
            .iter_mut()
            .find(|node| node.parameters.values().any(|p| p.id == parameter))
        else {
            continue;
        };
        let target = node
            .parameters
            .values_mut()
            .find(|p| p.id == parameter)
            .ok_or("Missing parameter")?;
        let Binding::Literal { value } = target.binding()? else {
            return Err("Only authored literal values can start a key track".into());
        };
        let track = crate::binding::Track::constant(value, time)?;
        target.value = serde_json::to_value(Binding::Track { track }).map_err(|e| e.to_string())?;
        found = true;
        break;
    }
    if !found {
        return Err("Missing parameter to animate".into());
    }
    crate::protection::validate(construction, &result).map_err(|e| e.reason)?;
    result.validate_semantics()?;
    Ok(result)
}

/// Typed tracks use the shared coupled-component projection.
pub(crate) use fold_graph::curves as track;
pub fn channels(
    construction: &Construction,
    object: Option<ObjectId>,
) -> Result<Vec<Channel>, String> {
    construction.validate_structure()?;
    let mut result = vec![];
    for definition in &construction.definitions {
        for node in &definition.nodes {
            let Ok(operation) = crate::registry::resolve(&node.operation, node.version, &node.mode)
            else {
                continue;
            };
            for (name, parameter) in &node.parameters {
                if object.is_some_and(|id| ![definition.id, node.id, parameter.id].contains(&id)) {
                    continue;
                }
                let Binding::Track { track } = parameter.binding()? else {
                    continue;
                };
                for (component, curve) in track::curves(&track)? {
                    result.push(Channel {
                        target: Target {
                            group: Some(definition.id),
                            object: node.id,
                            path: if component.is_empty() {
                                track.id.to_string()
                            } else {
                                format!("{}:{component}", track.id)
                            },
                        },
                        node_label: node.name.clone(),
                        property: parameter.id.to_string(),
                        property_label: operation
                            .parameters
                            .get(name)
                            .map_or(name, |p| &p.title)
                            .clone(),
                        component,
                        curve,
                    });
                }
            }
        }
    }
    for (group, instance) in std::iter::once((construction.root.definition, &construction.root))
        .chain(
            construction
                .definitions
                .iter()
                .flat_map(|d| d.instances.iter().map(move |i| (d.id, i))),
        )
    {
        let definition = construction.definition(instance.definition)?;
        for (input, parameter) in &instance.overrides {
            if object.is_some_and(|id| ![group, instance.id, parameter.id].contains(&id)) {
                continue;
            }
            let Binding::Track { track } = parameter.binding()? else {
                continue;
            };
            let label = definition
                .inputs
                .iter()
                .find(|i| i.id == *input)
                .ok_or("Missing override input")?;
            for (component, curve) in track::curves(&track)? {
                result.push(Channel {
                    target: Target {
                        group: Some(group),
                        object: instance.id,
                        path: format!("{}:{component}", track.id),
                    },
                    node_label: instance.name.clone(),
                    property: parameter.id.to_string(),
                    property_label: label.name.clone(),
                    component,
                    curve,
                });
            }
        }
    }
    Ok(result)
}

/// Channel identity is immutable routing data. Protection applies to the entire
/// proposal, including coupled vector/color keys and their opaque metadata.
pub fn replace(construction: &Construction, edits: &[Channel]) -> Result<Construction, String> {
    let available = channels(construction, None)?;
    let mut seen = BTreeSet::new();
    let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
    let owners: std::collections::BTreeMap<_, _> = available
        .iter()
        .flat_map(|c| {
            c.curve
                .keys
                .iter()
                .map(move |k| (k.id, (c.target.object, c.property.clone())))
        })
        .collect();
    for edit in edits {
        if !seen.insert(edit.target.clone()) {
            return Err("Duplicate animation channel".into());
        }
        let original = available
            .iter()
            .find(|c| c.target == edit.target)
            .ok_or("Unknown Motion animation channel")?;
        let mut identity = edit.clone();
        identity.curve = original.curve.clone();
        if &identity != original {
            return Err("Animation edit changed channel identity or metadata".into());
        }
        for key in &edit.curve.keys {
            if owners
                .get(&key.id)
                .is_some_and(|owner| owner != &(edit.target.object, edit.property.clone()))
            {
                return Err("Key identity belongs to another track".into());
            }
        }
        if edit.curve != original.curve {
            groups
                .entry((edit.target.object, edit.property.clone()))
                .or_default()
                .push((edit.component.clone(), edit.curve.clone()));
        }
    }
    let mut result = construction.clone();
    for ((object, property), edits) in groups {
        let parameter = if result.root.id == object {
            result
                .root
                .overrides
                .values_mut()
                .find(|p| p.id.to_string() == property)
        } else {
            result.definitions.iter_mut().find_map(|d| {
                if let Some(node) = d.nodes.iter_mut().find(|n| n.id == object) {
                    node.parameters
                        .values_mut()
                        .find(|p| p.id.to_string() == property)
                } else {
                    d.instances
                        .iter_mut()
                        .find(|i| i.id == object)
                        .and_then(|i| {
                            i.overrides
                                .values_mut()
                                .find(|p| p.id.to_string() == property)
                        })
                }
            })
        }
        .ok_or("Missing track parameter")?;
        let Binding::Track { track } = parameter.binding()? else {
            return Err("Track binding changed".into());
        };
        parameter.value = serde_json::to_value(Binding::Track {
            track: track::replace(&track, &edits)?,
        })
        .map_err(|e| e.to_string())?;
    }
    crate::protection::validate(construction, &result).map_err(|e| e.reason)?;
    result.validate_semantics()?;
    Ok(result)
}
