//! Bounded native patches. UUID paths and expected values refer to one base.
use crate::{
    merge::{Part, Record, Shape, child},
    model::Construction,
};
use fold_foundation::ObjectId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const MAX_PATCHES: usize = 128;
pub const MAX_BYTES: usize = 1024 * 1024;

/// Explicit presence distinguishes deleting a property from writing JSON null.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Slot {
    Absent {},
    Present { value: Value },
}
impl Slot {
    fn from_option(value: Option<&Value>) -> Self {
        match value {
            Some(value) => Self::Present {
                value: value.clone(),
            },
            None => Self::Absent {},
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub path: Vec<Part>,
    pub expected: Slot,
    pub value: Slot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Validation(crate::validation::Diagnostic),
    Invalid {
        index: Option<usize>,
        reason: String,
    },
    Conflict {
        index: usize,
        path: Vec<Part>,
        expected: Slot,
        actual: Slot,
    },
}
fn invalid(index: Option<usize>, reason: impl Into<String>) -> Error {
    Error::Invalid {
        index,
        reason: reason.into(),
    }
}

/// All preconditions read the same base. Overlapping paths are rejected; explicit
/// collection orders apply after record changes. No partial result is published.
/// Editing's candidate gate must still validate scope, protection and semantics.
pub fn construction(base: &Construction, patches: &[Patch]) -> Result<Construction, Error> {
    if patches.is_empty() || patches.len() > MAX_PATCHES {
        return Err(invalid(None, "Expected 1..128 patches"));
    }
    let bytes = serde_json::to_vec(patches).map_err(|e| invalid(None, e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid(None, "Patch payload exceeds 1 MiB"));
    }
    base.validate_structure().map_err(|e| invalid(None, e))?;
    let base = serde_json::to_value(base).map_err(|e| invalid(None, e.to_string()))?;
    let root = Shape::Record(Record::Construction);
    for (index, patch) in patches.iter().enumerate() {
        if patch.path.is_empty() || patch.path.len() > 32 {
            return Err(invalid(Some(index), "Native path requires 1..32 parts"));
        }
        if patches[..index]
            .iter()
            .any(|other| patch.path.starts_with(&other.path) || other.path.starts_with(&patch.path))
        {
            return Err(invalid(Some(index), "Duplicate or overlapping patch paths"));
        }
        let actual = read(&base, root, &patch.path).map_err(|e| invalid(Some(index), e))?;
        if actual != patch.expected {
            return Err(Error::Conflict {
                index,
                path: patch.path.clone(),
                expected: patch.expected.clone(),
                actual,
            });
        }
    }
    let mut result = base;
    // Ordering can describe the final membership, independently of request order.
    for order_pass in [false, true] {
        for (index, patch) in patches.iter().enumerate() {
            if matches!(patch.path.last(), Some(Part::Order)) != order_pass {
                continue;
            }
            write(&mut result, root, &patch.path, &patch.value)
                .map_err(|e| invalid(Some(index), e))?;
        }
    }
    let result: Construction =
        serde_json::from_value(result).map_err(|e| invalid(None, e.to_string()))?;
    result
        .validate_structure_detailed()
        .map_err(Error::Validation)?;
    Ok(result)
}

fn field_shape(shape: Shape, key: &str, parent: &Value) -> Result<Shape, String> {
    match shape {
        Shape::Record(Record::Key) | Shape::Atomic => {
            Err("Cannot descend into an atomic native property".into())
        }
        Shape::Record(record) => Ok(child(record, key, parent)),
        Shape::Map(record) => Ok(Shape::Record(record)),
        Shape::Dictionary => Ok(Shape::Atomic),
        Shape::Records(_) => Err("Native collections require UUID element paths".into()),
    }
}
fn record_id(value: &Value) -> Result<ObjectId, String> {
    serde_json::from_value(value.get("id").ok_or("Missing native record ID")?.clone())
        .map_err(|e| e.to_string())
}
fn order(value: &Value) -> Result<Value, String> {
    let ids: Vec<_> = value
        .as_array()
        .ok_or("Expected native collection")?
        .iter()
        .map(record_id)
        .collect::<Result<_, _>>()?;
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("Duplicate native collection ID".into());
    }
    serde_json::to_value(ids).map_err(|e| e.to_string())
}
fn read(value: &Value, shape: Shape, path: &[Part]) -> Result<Slot, String> {
    let (head, rest) = path.split_first().ok_or("Empty native path")?;
    let (next, shape) = match head {
        Part::Field(key) => {
            let next_shape = field_shape(shape, key, value)?;
            (
                value.as_object().ok_or("Expected native object")?.get(key),
                next_shape,
            )
        }
        Part::Element(id) => {
            let Shape::Records(record) = shape else {
                return Err("UUID element requires a native collection".into());
            };
            let values = value.as_array().ok_or("Expected native collection")?;
            (
                values.iter().find(|v| record_id(v).as_ref() == Ok(id)),
                Shape::Record(record),
            )
        }
        Part::Order if rest.is_empty() && matches!(shape, Shape::Records(_)) => {
            return Ok(Slot::Present {
                value: order(value)?,
            });
        }
        Part::Order => return Err("Order must terminate a native collection path".into()),
    };
    if rest.is_empty() {
        Ok(Slot::from_option(next))
    } else {
        read(next.ok_or("Missing native path parent")?, shape, rest)
    }
}
fn write(value: &mut Value, shape: Shape, path: &[Part], replacement: &Slot) -> Result<(), String> {
    let (head, rest) = path.split_first().ok_or("Empty native path")?;
    match head {
        Part::Field(key) => {
            let next_shape = field_shape(shape, key, value)?;
            let values = value.as_object_mut().ok_or("Expected native object")?;
            if rest.is_empty() {
                match replacement {
                    Slot::Absent {} => {
                        values.remove(key);
                    }
                    Slot::Present { value } => {
                        values.insert(key.clone(), value.clone());
                    }
                }
                Ok(())
            } else {
                write(
                    values.get_mut(key).ok_or("Missing native path parent")?,
                    next_shape,
                    rest,
                    replacement,
                )
            }
        }
        Part::Element(id) => {
            let Shape::Records(record) = shape else {
                return Err("UUID element requires a native collection".into());
            };
            let values = value.as_array_mut().ok_or("Expected native collection")?;
            let index = values.iter().position(|v| record_id(v).as_ref() == Ok(id));
            if rest.is_empty() {
                match replacement {
                    Slot::Absent {} => {
                        if let Some(index) = index {
                            values.remove(index);
                        }
                    }
                    Slot::Present { value } => {
                        if record_id(value)? != *id {
                            return Err("Replacement ID differs from its native path".into());
                        }
                        if let Some(index) = index {
                            values[index] = value.clone();
                        } else {
                            values.push(value.clone());
                        }
                    }
                }
                Ok(())
            } else {
                write(
                    &mut values[index.ok_or("Missing native path parent")?],
                    Shape::Record(record),
                    rest,
                    replacement,
                )
            }
        }
        Part::Order if rest.is_empty() && matches!(shape, Shape::Records(_)) => {
            let Slot::Present { value: replacement } = replacement else {
                return Err("Cannot remove collection order".into());
            };
            let wanted: Vec<ObjectId> =
                serde_json::from_value(replacement.clone()).map_err(|e| e.to_string())?;
            let current: Vec<ObjectId> =
                serde_json::from_value(order(value)?).map_err(|e| e.to_string())?;
            if wanted.len() != current.len()
                || wanted.iter().collect::<BTreeSet<_>>() != current.iter().collect()
            {
                return Err("Order must contain every final collection ID exactly once".into());
            }
            let values = value.as_array_mut().ok_or("Expected native collection")?;
            let mut records = std::mem::take(values)
                .into_iter()
                .map(|v| Ok((record_id(&v)?, v)))
                .collect::<Result<std::collections::BTreeMap<_, _>, String>>()?;
            *values = wanted
                .into_iter()
                .map(|id| {
                    records
                        .remove(&id)
                        .ok_or_else(|| "Missing ordered record".into())
                })
                .collect::<Result<_, String>>()?;
            Ok(())
        }
        Part::Order => Err("Order must terminate a native collection path".into()),
    }
}
