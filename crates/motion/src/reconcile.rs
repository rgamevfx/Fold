//! Explicit native identity reconciliation. Names and opaque JSON never imply identity.
use crate::{
    binding::Binding,
    expression::Expr,
    model::{Definition, Edge, Parameter, Reference},
    registry,
    types::{AssetReference, Literal},
};
use fold_foundation::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Definition,
    Input,
    Node,
    Parameter,
    Instance,
    Output,
    Edge,
    Track,
    Ramp,
    Stop,
    Key,
}
pub type IdentityMap = BTreeMap<ObjectId, ObjectId>;
fn take(ids: &mut BTreeMap<ObjectId, Kind>, id: ObjectId, kind: Kind) -> Result<(), String> {
    if ids.insert(id, kind).is_some() {
        return Err(format!("Duplicate native identity {id:?}"));
    }
    if ids.len() > crate::model::MAX_OBJECTS {
        return Err("Native object budget exceeded".into());
    }
    Ok(())
}
fn binding_ids(
    binding: &Binding,
    ids: &mut BTreeMap<ObjectId, Kind>,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Binding depth exceeds 64".into());
    }
    match binding {
        Binding::Link { reference } => {
            reference.validate()?;
            take(ids, reference.id, Kind::Edge)?;
        }
        Binding::Expression { expression } => {
            for edge in expression.edges()? {
                take(ids, edge.id, Kind::Edge)?;
            }
        }
        Binding::Ramp { ramp } => {
            ramp.validate()?;
            take(ids, ramp.id, Kind::Ramp)?;
            for stop in &ramp.stops {
                take(ids, stop.id, Kind::Stop)?;
            }
        }
        Binding::Track { track } => {
            track.validate()?;
            take(ids, track.id, Kind::Track)?;
            for key in &track.keys {
                take(ids, key.id, Kind::Key)?;
            }
        }
        Binding::List { values } => {
            if values.len() > 4096 {
                return Err("Binding list exceeds 4096 values".into());
            }
            for value in values {
                binding_ids(value, ids, depth + 1)?;
            }
        }
        Binding::Literal { value } => value.validate()?,
    }
    Ok(())
}
/// IDs owned by a definition, excluding referenced external definitions/resources.
/// Unresolved argument payloads remain opaque; only their known outer records count.
pub fn identities(definition: &Definition) -> Result<BTreeMap<ObjectId, Kind>, String> {
    let mut ids = BTreeMap::new();
    take(&mut ids, definition.id, Kind::Definition)?;
    for input in &definition.inputs {
        take(&mut ids, input.id, Kind::Input)?;
    }
    for node in &definition.nodes {
        take(&mut ids, node.id, Kind::Node)?;
        for parameter in node.parameters.values() {
            take(&mut ids, parameter.id, Kind::Parameter)?;
            if registry::resolve(&node.operation, node.version, &node.mode).is_ok() {
                binding_ids(&parameter.binding()?, &mut ids, 0)?;
            }
        }
    }
    for instance in &definition.instances {
        take(&mut ids, instance.id, Kind::Instance)?;
        for parameter in instance.overrides.values() {
            take(&mut ids, parameter.id, Kind::Parameter)?;
            binding_ids(&parameter.binding()?, &mut ids, 0)?;
        }
    }
    for output in &definition.outputs {
        take(&mut ids, output.id, Kind::Output)?;
        output.value.validate()?;
        take(&mut ids, output.value.id, Kind::Edge)?;
    }
    Ok(ids)
}
/// Mappings are simultaneous, one-to-one and restricted to records of the same
/// kind in the selected previous definition. Unmapped records keep their IDs.
pub fn definition(
    before: &Definition,
    proposed: &Definition,
    mapping: &IdentityMap,
) -> Result<Definition, String> {
    let source = identities(proposed)?;
    let target = identities(before)?;
    let mut destinations = BTreeSet::new();
    for (from, to) in mapping {
        let kind = source
            .get(from)
            .ok_or("Identity mapping source is outside the fragment")?;
        if target.get(to) != Some(kind) {
            return Err("Identity mapping target is outside the selected definition or has a different kind".into());
        }
        if !destinations.insert(*to) {
            return Err("Identity mapping is not one-to-one".into());
        }
    }
    let mut result = proposed.clone();
    remap(&mut result, mapping)?;
    identities(&result)?;
    if result.id != before.id {
        return Err(
            "Definition replacement requires its original UUID or an explicit definition mapping"
                .into(),
        );
    }
    Ok(result)
}
/// Reconcile source mappings alongside the native identities they describe.
pub fn fragment(
    before: &Definition,
    proposed: &crate::source::Fragment,
    mapping: &IdentityMap,
) -> Result<crate::source::Fragment, String> {
    let definition = definition(before, &proposed.definition, mapping)?;
    let mut spans = BTreeMap::new();
    for (id, span) in &proposed.spans {
        let id = mapping.get(id).copied().unwrap_or(*id);
        if spans.insert(id, *span).is_some() {
            return Err("Identity mapping merges source spans".into());
        }
    }
    let unresolved = proposed
        .unresolved
        .iter()
        .map(|id| mapping.get(id).copied().unwrap_or(*id))
        .collect();
    Ok(crate::source::Fragment {
        definition,
        spans,
        unresolved,
    })
}
fn id(value: &mut ObjectId, mapping: &IdentityMap) {
    if let Some(target) = mapping.get(value) {
        *value = *target;
    }
}
fn reference(value: &mut Reference, mapping: &IdentityMap) {
    match value {
        Reference::Input { input } => id(input, mapping),
        Reference::Node { node } => id(node, mapping),
        Reference::Instance { instance, output } => {
            id(instance, mapping);
            id(output, mapping);
        }
    }
}
fn edge(value: &mut Edge, mapping: &IdentityMap) {
    id(&mut value.id, mapping);
    reference(&mut value.target, mapping);
}
fn literal(value: &mut Literal, mapping: &IdentityMap) {
    if let Literal::AssetRef {
        value: AssetReference::Definition(definition),
    } = value
    {
        id(definition, mapping);
    }
}
fn expression(value: &mut Expr, mapping: &IdentityMap) {
    match value {
        Expr::Literal { value } => literal(value, mapping),
        Expr::Reference { reference } => edge(reference, mapping),
        Expr::Unary { value, .. } => expression(value, mapping),
        Expr::Binary { left, right, .. } => {
            expression(left, mapping);
            expression(right, mapping);
        }
        Expr::Call { arguments, .. } => {
            for argument in arguments {
                expression(argument, mapping);
            }
        }
        Expr::Vector { components, .. } => {
            for component in components {
                expression(component, mapping);
            }
        }
        _ => {}
    }
}
fn binding(value: &mut Binding, mapping: &IdentityMap) {
    match value {
        Binding::Literal { value } => literal(value, mapping),
        Binding::Link { reference } => edge(reference, mapping),
        Binding::Expression { expression: value } => expression(value, mapping),
        Binding::Ramp { ramp } => {
            id(&mut ramp.id, mapping);
            for stop in &mut ramp.stops {
                id(&mut stop.id, mapping);
                literal(&mut stop.value, mapping);
            }
        }
        Binding::Track { track } => {
            id(&mut track.id, mapping);
            if let Some(base) = &mut track.base {
                literal(base, mapping);
            }
            for key in &mut track.keys {
                id(&mut key.id, mapping);
                literal(&mut key.value, mapping);
            }
        }
        Binding::List { values } => {
            for value in values {
                binding(value, mapping);
            }
        }
    }
}
fn parameter(value: &mut Parameter, mapping: &IdentityMap, known: bool) -> Result<(), String> {
    id(&mut value.id, mapping);
    if known {
        let mut decoded = value.binding()?;
        binding(&mut decoded, mapping);
        value.value = serde_json::to_value(decoded).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn remap(value: &mut Definition, mapping: &IdentityMap) -> Result<(), String> {
    id(&mut value.id, mapping);
    for input in &mut value.inputs {
        id(&mut input.id, mapping);
        literal(&mut input.default, mapping);
    }
    for node in &mut value.nodes {
        id(&mut node.id, mapping);
        let known = registry::resolve(&node.operation, node.version, &node.mode).is_ok();
        for value in node.parameters.values_mut() {
            parameter(value, mapping, known)?;
        }
    }
    for instance in &mut value.instances {
        id(&mut instance.id, mapping);
        id(&mut instance.definition, mapping);
        let mut overrides = BTreeMap::new();
        for (mut input, mut value) in std::mem::take(&mut instance.overrides) {
            id(&mut input, mapping);
            parameter(&mut value, mapping, true)?;
            if overrides.insert(input, value).is_some() {
                return Err("Identity mapping merges override keys".into());
            }
        }
        instance.overrides = overrides;
    }
    for output in &mut value.outputs {
        id(&mut output.id, mapping);
        edge(&mut output.value, mapping);
    }
    Ok(())
}
