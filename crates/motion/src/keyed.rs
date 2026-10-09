//! Lossless scalar-track projection into the shared native curve editor.
use crate::{
    binding::{Binding, Interpolation},
    model::Construction,
    types::Literal,
};
use fold_foundation::ObjectId;
use fold_platform::animation::{Channel, Target};
use std::collections::BTreeSet;

/// Convert a literal parameter to an editable native keyframes node. The target
/// parameter retains its ID and metadata; the new connection is explicit.
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
        let owner_name = node.name.clone();
        let (port, target) = node
            .parameters
            .iter_mut()
            .find(|(_, p)| p.id == parameter)
            .ok_or("Missing parameter")?;
        let Binding::Literal { value } = target.binding()? else {
            return Err("Only authored literal values can start a key track".into());
        };
        if !value.ty().is_interpolatable() {
            return Err("This value type does not support interpolated keys".into());
        }
        let track = crate::binding::Track::constant(value, time)?;
        let mut stem = format!("{owner_name} {port} keys");
        while stem.len() > 240 {
            stem.pop();
        }
        let mut animation = crate::registry::resolve("fold.motion.keyframes", 1, "value")?
            .node(&stem, [("track".into(), Binding::Track { track })].into())?;
        target.value = serde_json::to_value(Binding::Link {
            reference: crate::model::Reference::Node { node: animation.id }.into(),
        })
        .map_err(|e| e.to_string())?;
        let names: BTreeSet<_> = definition
            .inputs
            .iter()
            .map(|i| i.name.as_str())
            .chain(definition.nodes.iter().map(|n| n.name.as_str()))
            .chain(definition.instances.iter().map(|i| i.name.as_str()))
            .collect();
        let mut suffix = 2;
        while names.contains(animation.name.as_str()) {
            animation.name = format!("{stem} {suffix}");
            suffix += 1;
        }
        definition.nodes.push(animation);
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

/// Only direct, resolved scalar/distance node tracks are qualified here. Other
/// bindings remain native and available in source; they are never flattened.
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
                if !matches!(
                    track.ty,
                    crate::types::Type::Scalar | crate::types::Type::Distance { .. }
                ) {
                    continue;
                }
                track.validate()?;
                let curve = fold_animation::Curve {
                    keys: track
                        .keys
                        .iter()
                        .map(|key| {
                            let value = match key.value {
                                Literal::Scalar { value } | Literal::Distance { value, .. } => {
                                    value
                                }
                                _ => unreachable!("validated scalar track"),
                            };
                            fold_animation::Key {
                                id: key.id,
                                time: key.time,
                                value,
                                interpolation: match key.interpolation {
                                    Interpolation::Linear => fold_animation::Interpolation::Linear,
                                    Interpolation::Hold => fold_animation::Interpolation::Hold,
                                },
                                tangents: Default::default(),
                                incoming: Default::default(),
                                outgoing: Default::default(),
                            }
                        })
                        .collect(),
                };
                curve.validate()?;
                result.push(Channel {
                    target: Target {
                        group: Some(definition.id),
                        object: node.id,
                        path: track.id.to_string(),
                    },
                    node_label: node.name.clone(),
                    property: parameter.id.to_string(),
                    property_label: operation
                        .parameters
                        .get(name)
                        .map_or(name, |p| &p.title)
                        .clone(),
                    component: String::new(),
                    curve,
                });
            }
        }
    }
    Ok(result)
}

/// Projection metadata is immutable routing data, not a retargeting API. Existing
/// native keys retain their extensions; new keys receive new empty extension maps.
pub fn replace(construction: &Construction, edits: &[Channel]) -> Result<Construction, String> {
    let available = channels(construction, None)?;
    let by_target: std::collections::BTreeMap<_, _> = available
        .iter()
        .map(|channel| (&channel.target, channel))
        .collect();
    let mut owners = std::collections::BTreeMap::new();
    for channel in &available {
        for key in &channel.curve.keys {
            if owners.insert(key.id, &channel.target).is_some() {
                return Err("Duplicate native key identity".into());
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut result = construction.clone();
    for edit in edits {
        if !seen.insert(edit.target.clone()) {
            return Err("Duplicate animation channel".into());
        }
        let original = *by_target
            .get(&edit.target)
            .ok_or("Unknown Motion animation channel")?;
        let mut identity = edit.clone();
        identity.curve = original.curve.clone();
        if &identity != original {
            return Err("Animation edit changed channel identity or metadata".into());
        }
        if edit.curve == original.curve {
            continue;
        }
        edit.curve.validate()?;
        let node = result
            .definitions
            .iter_mut()
            .flat_map(|d| &mut d.nodes)
            .find(|n| n.id == edit.target.object)
            .ok_or("Missing track owner")?;
        let parameter = node
            .parameters
            .values_mut()
            .find(|p| p.id.to_string() == edit.property)
            .ok_or("Missing track parameter")?;
        let Binding::Track { mut track } = parameter.binding()? else {
            return Err("Track binding changed".into());
        };
        let mut keys = vec![];
        for key in &edit.curve.keys {
            if owners
                .get(&key.id)
                .is_some_and(|owner| **owner != edit.target)
            {
                return Err("Key identity belongs to another track".into());
            }
            let interpolation = match key.interpolation {
                fold_animation::Interpolation::Linear => Interpolation::Linear,
                fold_animation::Interpolation::Hold => Interpolation::Hold,
                _ => {
                    return Err(
                        "Motion tracks currently support Linear and Hold interpolation".into(),
                    );
                }
            };
            if key.tangents != fold_animation::Tangents::Auto
                || key.incoming != Default::default()
                || key.outgoing != Default::default()
            {
                return Err("Motion tracks do not support tangent edits".into());
            }
            let value = match track.ty {
                crate::types::Type::Scalar => Literal::Scalar { value: key.value },
                crate::types::Type::Distance { unit } => Literal::Distance {
                    value: key.value,
                    unit,
                },
                _ => return Err("This track requires typed native key editing".into()),
            };
            keys.push(crate::binding::Key {
                id: key.id,
                time: key.time,
                value,
                interpolation,
                extensions: track
                    .keys
                    .iter()
                    .find(|old| old.id == key.id)
                    .map(|old| old.extensions.clone())
                    .unwrap_or_default(),
            });
        }
        track.keys = keys;
        track.validate()?;
        parameter.value =
            serde_json::to_value(Binding::Track { track }).map_err(|e| e.to_string())?;
    }
    // The same protection and semantic gate applies even for non-UI callers.
    crate::protection::validate(construction, &result).map_err(|e| e.reason)?;
    result.validate_semantics()?;
    Ok(result)
}
