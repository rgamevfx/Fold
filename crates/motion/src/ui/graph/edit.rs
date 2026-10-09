use super::{Target, source};
use crate::{
    binding::Binding,
    model::{Construction, Reference},
    registry,
};
use fold_foundation::ObjectId;
use fold_ui::sdk::graph_canvas::*;

fn connect(
    value: &mut Construction,
    definition: ObjectId,
    socket: &Socket,
    reference: Option<Reference>,
) -> Result<(), String> {
    let target: Target = serde_json::from_str(&socket.key).map_err(|e| e.to_string())?;
    let definition = value
        .definitions
        .iter_mut()
        .find(|d| d.id == definition)
        .ok_or("Missing asset")?;
    match target {
        Target::Output => {
            let output = definition
                .outputs
                .iter_mut()
                .find(|o| o.id == socket.node)
                .ok_or("Missing output")?;
            output.value.target = reference.ok_or("An output requires a connection")?;
        }
        Target::Override { input } => {
            let instance = definition
                .instances
                .iter_mut()
                .find(|i| i.id == socket.node)
                .ok_or("Missing instance")?;
            if let Some(reference) = reference {
                if let Some(parameter) = instance.overrides.get_mut(&input) {
                    let binding = match parameter.binding()? {
                        Binding::Link {
                            reference: mut edge,
                        } => {
                            edge.target = reference;
                            Binding::Link { reference: edge }
                        }
                        Binding::Literal { .. } => Binding::Link {
                            reference: reference.into(),
                        },
                        _ => return Err("Edit expression or track overrides in Source".into()),
                    };
                    parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
                } else {
                    instance.overrides.insert(
                        input,
                        crate::model::Parameter::new(Binding::Link {
                            reference: reference.into(),
                        })?,
                    );
                }
            } else {
                instance.overrides.remove(&input);
            }
        }
        Target::OverrideEdge { input, edge } => {
            let instance = definition
                .instances
                .iter_mut()
                .find(|i| i.id == socket.node)
                .ok_or("Missing instance")?;
            let parameter = instance
                .overrides
                .get_mut(&input)
                .ok_or("Missing override")?;
            let mut binding = parameter.binding()?;
            crate::connections::retarget(
                &mut binding,
                edge,
                reference.ok_or("Edit expression dependencies in Source")?,
            )?;
            parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        }
        Target::Value { parameter }
        | Target::Edge { parameter, .. }
        | Target::Append { parameter } => {
            let node = definition
                .nodes
                .iter_mut()
                .find(|n| n.id == socket.node)
                .ok_or("Missing node")?;
            let operation = registry::resolve(&node.operation, node.version, &node.mode)?;
            let (name, parameter) = node
                .parameters
                .iter_mut()
                .find(|(_, p)| p.id == parameter)
                .ok_or("Missing parameter")?;
            let mut binding = parameter.binding()?;
            match target {
                Target::Value { .. } => {
                    binding = if let Some(reference) = reference {
                        match binding {
                            Binding::Link {
                                reference: mut edge,
                            } => {
                                edge.target = reference;
                                Binding::Link { reference: edge }
                            }
                            Binding::Literal { .. } => Binding::Link {
                                reference: reference.into(),
                            },
                            Binding::Expression { expression }
                                if expression.edges()?.is_empty() =>
                            {
                                Binding::Link {
                                    reference: reference.into(),
                                }
                            }
                            _ => {
                                return Err(
                                    "Use individual expression connections or Source".into()
                                );
                            }
                        }
                    } else {
                        operation
                            .parameters
                            .get(name)
                            .and_then(|p| p.default.clone())
                            .ok_or("This input requires a value or replacement connection")?
                    };
                }
                Target::Edge { edge, .. } => {
                    if let Some(reference) = reference {
                        crate::connections::retarget(&mut binding, edge, reference)?;
                    } else if let Binding::List { values } = &mut binding {
                        let index = values.iter().position(|b| matches!(b, Binding::Link { reference } if reference.id == edge)).ok_or("Edit expression dependencies in Source")?;
                        values.remove(index);
                    } else {
                        return Err("Edit expression dependencies in Source".into());
                    }
                }
                Target::Append { .. } => {
                    let Binding::List { values } = &mut binding else {
                        return Err("Not a list input".into());
                    };
                    values.push(Binding::Link {
                        reference: reference.ok_or("No appended connection")?.into(),
                    });
                }
                _ => unreachable!(),
            }
            parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn reference(wire: &WireView, definition: &crate::model::Definition) -> Result<Reference, String> {
    let reference: Reference = serde_json::from_str(&wire.source.key).map_err(|e| e.to_string())?;
    let owner = if let Reference::Parameter { parameter, .. } = reference {
        if definition.inputs.iter().any(|p| p.id == parameter) {
            definition.id
        } else {
            definition
                .nodes
                .iter()
                .find(|n| n.parameters.values().any(|p| p.id == parameter))
                .map(|n| n.id)
                .ok_or("Missing parameter source")?
        }
    } else {
        source(reference, definition.id).node
    };
    if owner != wire.source.node {
        return Err("Connection source identity mismatch".into());
    }
    Ok(reference)
}

pub(in crate::ui) fn apply(
    value: &Construction,
    definition: ObjectId,
    changes: &[GraphChange],
) -> Result<Construction, String> {
    let mut after = value.clone();
    for change in changes {
        match change {
            GraphChange::Connect(wire) => connect(
                &mut after,
                definition,
                &wire.target,
                Some(reference(wire, value.definition(definition)?)?),
            )?,
            GraphChange::Disconnect(socket) => connect(&mut after, definition, socket, None)?,
            GraphChange::Rewire { remove, add } => {
                for socket in remove
                    .iter()
                    .filter(|socket| !add.iter().any(|wire| wire.target == **socket))
                {
                    connect(&mut after, definition, socket, None)?;
                }
                for wire in add {
                    connect(
                        &mut after,
                        definition,
                        &wire.target,
                        Some(reference(wire, value.definition(definition)?)?),
                    )?;
                }
            }
            GraphChange::Positions(positions) => {
                let owned = graph_ids(after.definition(definition)?);
                for (id, position) in positions {
                    if !owned.contains(id) || !position.iter().all(|v| v.is_finite()) {
                        return Err("Invalid position target or coordinates".into());
                    }
                    after
                        .presentation
                        .positions
                        .insert(*id, position.map(f64::from));
                }
            }
            GraphChange::Delete(ids) => {
                let asset = after
                    .definitions
                    .iter_mut()
                    .find(|d| d.id == definition)
                    .ok_or("Missing asset")?;
                if ids.contains(&definition) {
                    return Err("Channel References belongs to the container. Edit its parameters in the Inspector.".into());
                }
                let owned = crate::reconcile::identities(asset)?;
                let visible = graph_ids(asset);
                if ids.iter().any(|id| !visible.contains(id)) {
                    return Err("Deletion is outside this graph".into());
                }
                asset.nodes.retain(|n| !ids.contains(&n.id));
                asset.inputs.retain(|n| !ids.contains(&n.id));
                asset.instances.retain(|n| !ids.contains(&n.id));
                asset.outputs.retain(|n| !ids.contains(&n.id));
                let retained = crate::reconcile::identities(asset)?;
                let removed: std::collections::BTreeSet<_> = owned
                    .keys()
                    .filter(|id| !retained.contains_key(id))
                    .copied()
                    .collect();
                after
                    .presentation
                    .positions
                    .retain(|id, _| !removed.contains(id));
                after
                    .presentation
                    .annotations
                    .retain(|id, _| !removed.contains(id));
                for group in &mut after.presentation.groups {
                    group.members.retain(|id| !removed.contains(id));
                }
            }
            GraphChange::Add { template, position } => {
                if !position.iter().all(|v| v.is_finite()) {
                    return Err("Invalid graph position".into());
                }
                let asset = after
                    .definitions
                    .iter_mut()
                    .find(|d| d.id == definition)
                    .ok_or("Missing asset")?;
                let ids = super::super::graph_templates::add(asset, template)?;
                for (index, id) in ids.iter().enumerate() {
                    after.presentation.positions.insert(
                        *id,
                        [
                            f64::from(position[0]) - (ids.len() - index - 1) as f64 * 240.,
                            f64::from(position[1]),
                        ],
                    );
                }
            }
        }
    }
    after.validate_structure()?;
    crate::protection::validate(value, &after).map_err(|e| e.reason)?;
    after.validate_semantics()?;
    Ok(after)
}

fn graph_ids(asset: &crate::model::Definition) -> std::collections::BTreeSet<ObjectId> {
    std::iter::once(asset.id)
        .chain(asset.nodes.iter().map(|v| v.id))
        .chain(asset.instances.iter().map(|v| v.id))
        .chain(asset.outputs.iter().map(|v| v.id))
        .collect()
}
