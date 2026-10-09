//! Protection is checked against the final native change, never source spelling.
use crate::model::{Construction, Definition, Group, Input, Instance, Node, Output, Parameter};
use fold_foundation::ObjectId;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub id: ObjectId,
    pub reason: String,
}
#[derive(PartialEq)]
enum Native<'a> {
    Definition(&'a Definition),
    Input(&'a Input),
    Node(&'a Node),
    Parameter(&'a Parameter),
    Output(&'a Output),
    Instance(&'a Instance),
    Group(&'a Group),
}
#[derive(PartialEq)]
struct Record<'a> {
    value: Native<'a>,
    owner: Option<ObjectId>,
    definition: Option<ObjectId>,
}
fn record<'a>(
    index: &mut BTreeMap<ObjectId, Record<'a>>,
    id: ObjectId,
    owner: Option<ObjectId>,
    definition: Option<ObjectId>,
    value: Native<'a>,
) {
    index.insert(
        id,
        Record {
            value,
            owner,
            definition,
        },
    );
}
fn index(construction: &Construction) -> BTreeMap<ObjectId, Record<'_>> {
    let mut index = BTreeMap::new();
    for definition in &construction.definitions {
        record(
            &mut index,
            definition.id,
            None,
            Some(definition.id),
            Native::Definition(definition),
        );
        for input in &definition.inputs {
            record(
                &mut index,
                input.id,
                Some(definition.id),
                Some(definition.id),
                Native::Input(input),
            );
        }
        for node in &definition.nodes {
            record(
                &mut index,
                node.id,
                Some(definition.id),
                Some(definition.id),
                Native::Node(node),
            );
            for parameter in node.parameters.values() {
                record(
                    &mut index,
                    parameter.id,
                    Some(node.id),
                    Some(definition.id),
                    Native::Parameter(parameter),
                );
            }
        }
        for output in &definition.outputs {
            record(
                &mut index,
                output.id,
                Some(definition.id),
                Some(definition.id),
                Native::Output(output),
            );
        }
        for instance in &definition.instances {
            instance_records(&mut index, instance, Some(definition.id));
        }
    }
    instance_records(&mut index, &construction.root, None);
    for group in &construction.presentation.groups {
        record(&mut index, group.id, None, None, Native::Group(group));
    }
    index
}
fn instance_records<'a>(
    index: &mut BTreeMap<ObjectId, Record<'a>>,
    instance: &'a Instance,
    parent: Option<ObjectId>,
) {
    record(
        index,
        instance.id,
        parent,
        parent,
        Native::Instance(instance),
    );
    for parameter in instance.overrides.values() {
        record(
            index,
            parameter.id,
            Some(instance.id),
            parent,
            Native::Parameter(parameter),
        );
    }
}
fn instances(construction: &Construction) -> impl Iterator<Item = &Instance> {
    std::iter::once(&construction.root)
        .chain(construction.definitions.iter().flat_map(|d| &d.instances))
}
fn anchors(
    construction: &Construction,
    guarded: &BTreeSet<ObjectId>,
) -> BTreeMap<ObjectId, serde_json::Value> {
    let mut anchors = BTreeMap::new();
    for definition in &construction.definitions {
        for node in &definition.nodes {
            for (name, parameter) in &node.parameters {
                if guarded.contains(&parameter.id) {
                    anchors.insert(
                        parameter.id,
                        json!([
                            definition.id,
                            node.id,
                            node.operation,
                            node.version,
                            node.mode,
                            name
                        ]),
                    );
                }
            }
        }
    }
    for instance in instances(construction) {
        for (input, parameter) in &instance.overrides {
            if guarded.contains(&parameter.id) {
                anchors.insert(
                    parameter.id,
                    json!([instance.id, instance.definition, input]),
                );
            }
        }
    }
    anchors
}
fn guarded(construction: &Construction) -> Result<BTreeSet<ObjectId>, Violation> {
    let fail = |id, reason: &str| Violation {
        id,
        reason: reason.into(),
    };
    let mut guarded = BTreeSet::new();
    for definition in &construction.definitions {
        for input in &definition.inputs {
            if input.protected {
                guarded.insert(input.id);
            }
        }
        for node in &definition.nodes {
            for parameter in node.parameters.values() {
                if parameter.protected {
                    guarded.insert(parameter.id);
                }
            }
        }
    }
    for instance in instances(construction) {
        for parameter in instance.overrides.values() {
            if parameter.protected {
                guarded.insert(parameter.id);
            }
        }
    }
    let mut groups: Vec<_> = construction
        .presentation
        .groups
        .iter()
        .filter(|g| g.locked)
        .map(|g| g.id)
        .collect();
    let mut seen_groups = BTreeSet::new();
    while let Some(id) = groups.pop() {
        if !seen_groups.insert(id) {
            continue;
        }
        guarded.insert(id);
        let group = construction
            .presentation
            .groups
            .iter()
            .find(|g| g.id == id)
            .ok_or_else(|| fail(id, "Missing locked group"))?;
        for member in &group.members {
            guarded.insert(*member);
            if construction
                .presentation
                .groups
                .iter()
                .any(|g| g.id == *member)
            {
                groups.push(*member);
            }
        }
    }
    Ok(guarded)
}
/// Native lock state for presentation, including guarded parent records.
pub fn locked_ids(construction: &Construction) -> Result<BTreeSet<ObjectId>, Violation> {
    construction
        .validate_structure()
        .map_err(|reason| Violation {
            id: construction.root.id,
            reason,
        })?;
    let guarded = guarded(construction)?;
    let records = index(construction);
    let mut locked = guarded.clone();
    for id in records.keys() {
        let mut cursor = Some(*id);
        while let Some(parent) = cursor {
            if guarded.contains(&parent) {
                locked.insert(*id);
                break;
            }
            cursor = records.get(&parent).and_then(|record| record.owner);
        }
    }
    Ok(locked)
}

/// Existing protected controls and locked groups cannot be removed, unprotected,
/// moved into replacement owners or changed via definition/instance replacement.
/// Lock creation is allowed; unlocking needs a separate explicit artist operation.
pub fn validate(before: &Construction, after: &Construction) -> Result<(), Violation> {
    let fail = |id, reason: &str| Violation {
        id,
        reason: reason.into(),
    };
    before
        .validate_structure()
        .map_err(|message| fail(before.root.id, &message))?;
    after
        .validate_structure()
        .map_err(|message| fail(after.root.id, &message))?;
    let old = index(before);
    let new = index(after);
    let guarded = guarded(before)?;
    let mut protected_inputs = BTreeSet::new();
    for definition in &before.definitions {
        for input in &definition.inputs {
            if guarded.contains(&input.id) || guarded.contains(&definition.id) {
                protected_inputs.insert(input.id);
            }
        }
    }
    let mut protected_definitions = BTreeSet::new();
    for id in &guarded {
        if old.get(id) != new.get(id) {
            return Err(fail(*id, "Protected record or its owner changed"));
        }
        if before.presentation.positions.get(id) != after.presentation.positions.get(id)
            || before.presentation.annotations.get(id) != after.presentation.annotations.get(id)
        {
            return Err(fail(*id, "Protected presentation changed"));
        }
        if let Some(definition) = old.get(id).and_then(|record| record.definition) {
            protected_definitions.insert(definition);
        }
    }
    let new_anchors = anchors(after, &guarded);
    for (id, anchor) in anchors(before, &guarded) {
        if new_anchors.get(&id) != Some(&anchor) {
            return Err(fail(id, "Protected parameter context changed"));
        }
    }
    // An instance override is another write path to a protected definition input.
    let old_instances: BTreeMap<_, _> = instances(before).map(|i| (i.id, i)).collect();
    for instance in instances(after) {
        for input in &protected_inputs {
            let prior = old_instances
                .get(&instance.id)
                .and_then(|i| i.overrides.get(input));
            if instance.overrides.get(input) != prior {
                return Err(fail(*input, "Protected input override changed"));
            }
        }
    }
    // Preserve the instance routes to protected definitions, including ancestor
    // assets. Keeping the old definition disconnected does not bypass protection.
    loop {
        let previous = protected_definitions.len();
        for definition in &before.definitions {
            if definition
                .instances
                .iter()
                .any(|i| protected_definitions.contains(&i.definition))
            {
                protected_definitions.insert(definition.id);
            }
        }
        if previous == protected_definitions.len() {
            break;
        }
    }
    let new_instances: BTreeMap<_, _> = instances(after).map(|i| (i.id, i)).collect();
    for instance in instances(before) {
        if protected_definitions.contains(&instance.definition) {
            let current = new_instances
                .get(&instance.id)
                .ok_or_else(|| fail(instance.id, "Protected asset instance removed"))?;
            if current.definition != instance.definition
                || old[&instance.id].owner != new[&instance.id].owner
            {
                return Err(fail(instance.id, "Protected asset route replaced"));
            }
        }
    }
    Ok(())
}
