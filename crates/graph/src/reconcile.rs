//! Conservative three-way reconciliation of shared native records. Callers own
//! semantic validation and protection; opaque values are atomic properties.
use crate::model::{Node, Parameter};
use std::collections::{BTreeMap, BTreeSet};

pub fn property<T: Clone + PartialEq>(base: &T, proposed: &T, current: &T) -> Result<T, String> {
    if proposed == base || proposed == current {
        Ok(current.clone())
    } else if current == base {
        Ok(proposed.clone())
    } else {
        Err("Native property changed concurrently".into())
    }
}
pub fn map<K: Ord + Clone, V: Clone + PartialEq>(
    base: &BTreeMap<K, V>,
    proposed: &BTreeMap<K, V>,
    current: &BTreeMap<K, V>,
) -> Result<BTreeMap<K, V>, String> {
    let keys: BTreeSet<_> = base
        .keys()
        .chain(proposed.keys())
        .chain(current.keys())
        .cloned()
        .collect();
    let mut result = BTreeMap::new();
    for key in keys {
        if let Some(value) = property(
            &base.get(&key).cloned(),
            &proposed.get(&key).cloned(),
            &current.get(&key).cloned(),
        )? {
            result.insert(key, value);
        }
    }
    Ok(result)
}
pub fn node(base: &Node, proposed: &Node, current: &Node) -> Result<Node, String> {
    let mut result = current.clone();
    result.id = property(&base.id, &proposed.id, &current.id)?;
    result.name = property(&base.name, &proposed.name, &current.name)?;
    let contract = property(
        &(base.operation.clone(), base.version, base.mode.clone()),
        &(
            proposed.operation.clone(),
            proposed.version,
            proposed.mode.clone(),
        ),
        &(
            current.operation.clone(),
            current.version,
            current.mode.clone(),
        ),
    )?;
    (result.operation, result.version, result.mode) = contract;
    result.parameters = parameters(&base.parameters, &proposed.parameters, &current.parameters)?;
    result.extensions = map(&base.extensions, &proposed.extensions, &current.extensions)?;
    Ok(result)
}
pub fn parameters(
    base: &BTreeMap<String, Parameter>,
    proposed: &BTreeMap<String, Parameter>,
    current: &BTreeMap<String, Parameter>,
) -> Result<BTreeMap<String, Parameter>, String> {
    map(base, proposed, current)
}
