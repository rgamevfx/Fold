//! Deterministic inspection of native changes, using merge's property boundaries.
use crate::{
    merge::{Part, Record, Shape, child},
    model::Construction,
};
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Addition,
    Removal,
    Value,
    Connection,
    Protection,
    Order,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub kind: Kind,
    pub path: Vec<Part>,
    pub native_id: Option<ObjectId>,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// Inspection only: callers still validate and commit through `editing`.
/// Added/removed parents carry their complete payload, without repeating children.
pub fn construction(before: &Construction, after: &Construction) -> Result<Vec<Change>, String> {
    before.validate_structure()?;
    after.validate_structure()?;
    let before = serde_json::to_value(before).map_err(|error| error.to_string())?;
    let after = serde_json::to_value(after).map_err(|error| error.to_string())?;
    let mut changes = vec![];
    walk(
        Some(&before),
        Some(&after),
        Shape::Record(Record::Construction),
        &[],
        None,
        Kind::Value,
        &mut changes,
    );
    Ok(changes)
}

fn id(value: &Value) -> Option<ObjectId> {
    serde_json::from_value(value.get("id")?.clone()).ok()
}

fn walk(
    before: Option<&Value>,
    after: Option<&Value>,
    shape: Shape,
    path: &[Part],
    owner: Option<ObjectId>,
    kind: Kind,
    changes: &mut Vec<Change>,
) {
    if before == after {
        return;
    }
    let owner = if matches!(shape, Shape::Record(_)) {
        before.and_then(id).or_else(|| after.and_then(id)).or(owner)
    } else {
        owner
    };
    if let (Some(before), Some(after)) = (before, after) {
        match shape {
            Shape::Records(record) => {
                if let (Some(before), Some(after)) = (before.as_array(), after.as_array()) {
                    // Do not lose opaque or malformed embedded payloads during inspection.
                    let (Some(old), Some(new)) = (index(before), index(after)) else {
                        changes.push(Change {
                            kind,
                            path: path.to_vec(),
                            native_id: owner,
                            before: Some(Value::Array(before.clone())),
                            after: Some(Value::Array(after.clone())),
                        });
                        return;
                    };
                    for id in old
                        .keys()
                        .chain(new.keys())
                        .copied()
                        .collect::<BTreeSet<_>>()
                    {
                        let mut next = path.to_vec();
                        next.push(Part::Element(id));
                        walk(
                            old.get(&id).copied(),
                            new.get(&id).copied(),
                            Shape::Record(record),
                            &next,
                            Some(id),
                            Kind::Value,
                            changes,
                        );
                    }
                    let old_order: Vec<_> = before.iter().filter_map(id).collect();
                    let new_order: Vec<_> = after.iter().filter_map(id).collect();
                    if old_order != new_order {
                        let mut next = path.to_vec();
                        next.push(Part::Order);
                        changes.push(Change {
                            kind: Kind::Order,
                            path: next,
                            native_id: owner,
                            before: Some(serde_json::json!(old_order)),
                            after: Some(serde_json::json!(new_order)),
                        });
                    }
                    return;
                }
            }
            Shape::Record(Record::Key | Record::Stop) | Shape::Atomic => {}
            Shape::Record(_) | Shape::Map(_) | Shape::Dictionary => {
                if let (Some(old), Some(new)) = (before.as_object(), after.as_object()) {
                    for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
                        let mut next = path.to_vec();
                        next.push(Part::Field(key.clone()));
                        let child_shape = match shape {
                            Shape::Record(record) => {
                                let old_shape = child(record, key, before);
                                if old_shape == child(record, key, after) {
                                    old_shape
                                } else {
                                    Shape::Atomic
                                }
                            }
                            Shape::Map(record) => Shape::Record(record),
                            _ => Shape::Atomic,
                        };
                        let kind = match (shape, key.as_str()) {
                            (Shape::Record(Record::Input | Record::Parameter), "protected")
                            | (Shape::Record(Record::Group), "locked") => Kind::Protection,
                            (Shape::Record(Record::Output), "value")
                            | (Shape::Record(Record::Instance), "definition")
                            | (Shape::Record(Record::Binding), "reference") => Kind::Connection,
                            (Shape::Record(Record::Binding), "expression" | "values")
                                if connections(key, old.get(key))
                                    != connections(key, new.get(key)) =>
                            {
                                Kind::Connection
                            }
                            _ => Kind::Value,
                        };
                        walk(
                            old.get(key),
                            new.get(key),
                            child_shape,
                            &next,
                            owner,
                            kind,
                            changes,
                        );
                    }
                    return;
                }
            }
        }
    }
    changes.push(Change {
        kind: if before.is_none() {
            Kind::Addition
        } else if after.is_none() {
            Kind::Removal
        } else {
            kind
        },
        path: path.to_vec(),
        native_id: owner,
        before: before.cloned(),
        after: after.cloned(),
    });
}

fn index(values: &[Value]) -> Option<BTreeMap<ObjectId, &Value>> {
    let mut records = BTreeMap::new();
    for value in values {
        if records.insert(id(value)?, value).is_some() {
            return None;
        }
    }
    Some(records)
}

fn connections(
    key: &str,
    value: Option<&Value>,
) -> Option<Vec<(ObjectId, crate::model::Reference)>> {
    use crate::binding::Binding;
    let binding = if key == "expression" {
        Binding::Expression {
            expression: serde_json::from_value(value?.clone()).ok()?,
        }
    } else {
        Binding::List {
            values: serde_json::from_value(value?.clone()).ok()?,
        }
    };
    let mut result = vec![];
    let mut pending = vec![&binding];
    while let Some(binding) = pending.pop() {
        match binding {
            Binding::Link { reference } => result.push((reference.id, reference.target)),
            Binding::Expression { expression } => {
                result.extend(
                    expression
                        .edges()
                        .ok()?
                        .into_iter()
                        .map(|edge| (edge.id, edge.target)),
                );
            }
            Binding::List { values } => pending.extend(values),
            _ => {}
        }
    }
    Some(result)
}
