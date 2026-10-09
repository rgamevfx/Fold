//! Three-way native merge. Expected base values and persistent IDs define conflicts.
//! Opaque extension payloads, literals and expressions are atomic properties.
use crate::model::Construction;
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Part {
    Field(String),
    Element(ObjectId),
    Order,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub path: Vec<Part>,
    pub native_id: Option<ObjectId>,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
    pub proposed: Option<Value>,
    pub reason: String,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Record {
    Construction,
    Canvas,
    Definition,
    Input,
    Node,
    Parameter,
    Instance,
    Output,
    Presentation,
    Group,
    Binding,
    Track,
    Ramp,
    Stop,
    Key,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Record(Record),
    Map(Record),
    Records(Record),
    Dictionary,
    Atomic,
}
/// No mutation is published here. Callers must revalidate dependencies/protection
/// and create a new immutable candidate generation before observing/applying it.
pub fn construction(
    base: &Construction,
    proposed: &Construction,
    current: &Construction,
) -> Result<Construction, Vec<Conflict>> {
    let serialize = |value: &Construction| {
        serde_json::to_value(value).map_err(|error| {
            vec![Conflict {
                path: vec![],
                native_id: None,
                expected: None,
                actual: None,
                proposed: None,
                reason: error.to_string(),
            }]
        })
    };
    let base = serialize(base)?;
    let proposed = serialize(proposed)?;
    let current = serialize(current)?;
    let mut merger = Merger { conflicts: vec![] };
    let merged = merger.value(
        Some(&base),
        Some(&proposed),
        Some(&current),
        Shape::Record(Record::Construction),
        &[],
        None,
    );
    if !merger.conflicts.is_empty() {
        return Err(merger.conflicts);
    }
    serde_json::from_value(merged.unwrap_or(Value::Null)).map_err(|error| {
        vec![Conflict {
            path: vec![],
            native_id: None,
            expected: Some(base),
            actual: Some(current),
            proposed: Some(proposed),
            reason: format!("Merged native shape is invalid: {error}"),
        }]
    })
}
struct Merger {
    conflicts: Vec<Conflict>,
}
impl Merger {
    fn conflict(
        &mut self,
        path: &[Part],
        native_id: Option<ObjectId>,
        values: [Option<&Value>; 3],
        reason: &str,
    ) -> Option<Value> {
        self.conflicts.push(Conflict {
            path: path.to_vec(),
            native_id,
            expected: values[0].cloned(),
            proposed: values[1].cloned(),
            actual: values[2].cloned(),
            reason: reason.into(),
        });
        values[2].cloned()
    }
    fn value(
        &mut self,
        base: Option<&Value>,
        proposed: Option<&Value>,
        current: Option<&Value>,
        shape: Shape,
        path: &[Part],
        native_id: Option<ObjectId>,
    ) -> Option<Value> {
        if proposed == base {
            return current.cloned();
        }
        if current == base {
            return proposed.cloned();
        }
        let values = [base, proposed, current];
        let (Some(base), Some(proposed), Some(current)) = (base, proposed, current) else {
            return self.conflict(
                path,
                native_id,
                values,
                "Concurrent addition, removal or descendant edit",
            );
        };
        match shape {
            Shape::Records(record) => {
                self.records([base, proposed, current], record, path, native_id)
            }
            Shape::Record(Record::Key | Record::Stop) => self.conflict(
                path,
                native_id,
                values,
                "The same key or ramp stop changed concurrently",
            ),
            Shape::Record(record) => {
                let native_id = base
                    .get("id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .or(native_id);
                let coupled: &[&str] = match record {
                    Record::Node => &["id", "operation", "version", "mode"],
                    Record::Input | Record::Output => &["id", "ty"],
                    Record::Instance => &["id", "definition"],
                    Record::Track => &["id", "ty", "combination"],
                    Record::Ramp => &["id", "ty"],
                    Record::Binding => &["kind"],
                    _ => &["id"],
                };
                let replaced_identity =
                    base.get("id") != proposed.get("id") || base.get("id") != current.get("id");
                let proposal_context = coupled.iter().any(|key| base.get(key) != proposed.get(key));
                let current_context = coupled.iter().any(|key| base.get(key) != current.get(key));
                // Names are labels, not execution context. Concurrent label-only
                // edits do not invalidate another branch's contract change.
                let beyond_name = |branch: &Value| match (base.as_object(), branch.as_object()) {
                    (Some(base), Some(branch)) => base
                        .keys()
                        .chain(branch.keys())
                        .any(|key| key != "name" && base.get(key) != branch.get(key)),
                    _ => true,
                };
                if replaced_identity
                    || proposal_context && beyond_name(current)
                    || current_context && beyond_name(proposed)
                {
                    return self.conflict(
                        path,
                        native_id,
                        values,
                        "Identity or contract context changed concurrently",
                    );
                }
                self.object([base, proposed, current], shape, path, native_id)
            }
            Shape::Map(_) | Shape::Dictionary => {
                self.object([base, proposed, current], shape, path, native_id)
            }
            Shape::Atomic => self.conflict(
                path,
                native_id,
                values,
                "The same property changed concurrently",
            ),
        }
    }
    fn object(
        &mut self,
        values: [&Value; 3],
        shape: Shape,
        path: &[Part],
        native_id: Option<ObjectId>,
    ) -> Option<Value> {
        let [Some(base), Some(proposed), Some(current)] = values.map(Value::as_object) else {
            return self.conflict(
                path,
                native_id,
                values.map(Some),
                "Property shape changed concurrently",
            );
        };
        let keys: BTreeSet<_> = base
            .keys()
            .chain(proposed.keys())
            .chain(current.keys())
            .collect();
        let mut result = serde_json::Map::new();
        for key in keys {
            let mut next = path.to_vec();
            next.push(Part::Field(key.clone()));
            let child = match shape {
                Shape::Record(record) => child(record, key, values[0]),
                Shape::Map(record) => Shape::Record(record),
                _ => Shape::Atomic,
            };
            if let Some(value) = self.value(
                base.get(key),
                proposed.get(key),
                current.get(key),
                child,
                &next,
                native_id,
            ) {
                result.insert(key.clone(), value);
            }
        }
        Some(Value::Object(result))
    }
    fn records(
        &mut self,
        values: [&Value; 3],
        record: Record,
        path: &[Part],
        native_id: Option<ObjectId>,
    ) -> Option<Value> {
        let [Some(base), Some(proposed), Some(current)] = values.map(Value::as_array) else {
            return self.conflict(
                path,
                native_id,
                values.map(Some),
                "Record collection shape changed",
            );
        };
        fn parse(values: &[Value]) -> Option<(Vec<ObjectId>, BTreeMap<ObjectId, &Value>)> {
            let mut order = vec![];
            let mut records = BTreeMap::new();
            for value in values {
                let id = serde_json::from_value(value.get("id")?.clone()).ok()?;
                if records.insert(id, value).is_some() {
                    return None;
                }
                order.push(id);
            }
            Some((order, records))
        }
        let (
            Some((base_order, base)),
            Some((proposed_order, proposed)),
            Some((current_order, current)),
        ) = (parse(base), parse(proposed), parse(current))
        else {
            return self.conflict(
                path,
                native_id,
                values.map(Some),
                "Invalid record identity in collection",
            );
        };
        let ids: BTreeSet<_> = base
            .keys()
            .chain(proposed.keys())
            .chain(current.keys())
            .copied()
            .collect();
        let mut records = BTreeMap::new();
        for id in ids {
            let mut next = path.to_vec();
            next.push(Part::Element(id));
            if let Some(value) = self.value(
                base.get(&id).copied(),
                proposed.get(&id).copied(),
                current.get(&id).copied(),
                Shape::Record(record),
                &next,
                Some(id),
            ) {
                records.insert(id, value);
            }
        }
        if record == Record::Stop {
            let mut stops = vec![];
            for (id, value) in records {
                let Some(position) = value.get("position").and_then(Value::as_f64) else {
                    return self.conflict(
                        path,
                        Some(id),
                        values.map(Some),
                        "Invalid ramp stop position",
                    );
                };
                stops.push((position, id, value));
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            return Some(Value::Array(
                stops.into_iter().map(|(_, _, value)| value).collect(),
            ));
        }
        if record == Record::Key {
            let mut keys = vec![];
            for (id, value) in records {
                let Some(time) = value
                    .get("time")
                    .and_then(|v| serde_json::from_value::<Time>(v.clone()).ok())
                else {
                    return self.conflict(path, Some(id), values.map(Some), "Invalid key time");
                };
                keys.push((time, id, value));
            }
            keys.sort_by_key(|(time, id, _)| (*time, *id));
            return Some(Value::Array(
                keys.into_iter().map(|(_, _, value)| value).collect(),
            ));
        }
        let Some(order) = order(&base_order, &proposed_order, &current_order, &records) else {
            let mut next = path.to_vec();
            next.push(Part::Order);
            return self.conflict(
                &next,
                native_id,
                values.map(Some),
                "Concurrent record ordering constraints conflict",
            );
        };
        Some(Value::Array(
            order
                .into_iter()
                .filter_map(|id| records.remove(&id))
                .collect(),
        ))
    }
}
pub(crate) fn child(record: Record, key: &str, base: &Value) -> Shape {
    use Record::*;
    match (record, key) {
        (Construction, "canvas") => Shape::Record(Canvas),
        (Construction, "definitions") => Shape::Records(Definition),
        (Construction, "root") => Shape::Record(Instance),
        (Construction, "presentation") => Shape::Record(Presentation),
        (Canvas, "width" | "height" | "rate" | "start" | "end") => Shape::Atomic,
        (Definition, "inputs") => Shape::Records(Input),
        (Definition, "nodes") => Shape::Records(Node),
        (Definition, "instances") => Shape::Records(Instance),
        (Definition, "outputs") => Shape::Records(Output),
        (Node, "parameters") => {
            let resolved = base
                .get("operation")
                .and_then(Value::as_str)
                .zip(base.get("version").and_then(Value::as_u64))
                .zip(base.get("mode").and_then(Value::as_str))
                .is_some_and(|((key, version), mode)| {
                    u32::try_from(version)
                        .is_ok_and(|version| crate::registry::resolve(key, version, mode).is_ok())
                });
            if resolved {
                Shape::Map(Parameter)
            } else {
                Shape::Atomic
            }
        }
        (Parameter, "value") => Shape::Record(Binding),
        (Binding, "track") if base.get("kind").and_then(Value::as_str) == Some("track") => {
            Shape::Record(Track)
        }
        (Track, "keys") => Shape::Records(Key),
        (Binding, "ramp") if base.get("kind").and_then(Value::as_str) == Some("ramp") => {
            Shape::Record(Ramp)
        }
        (Ramp, "stops") => Shape::Records(Stop),
        (Instance, "overrides") => Shape::Map(Parameter),
        (Presentation, "groups") => Shape::Records(Group),
        (Presentation, "positions" | "annotations") => Shape::Dictionary,
        _ => Shape::Atomic,
    }
}
/// Merge independent insertions with deterministic UUID tie-breaking. Existing
/// relative order is changed only when a branch explicitly changed it.
fn order(
    base: &[ObjectId],
    proposed: &[ObjectId],
    current: &[ObjectId],
    records: &BTreeMap<ObjectId, Value>,
) -> Option<Vec<ObjectId>> {
    let original: BTreeSet<_> = base.iter().copied().collect();
    let surviving = |ids: &[ObjectId]| {
        ids.iter()
            .filter(|id| original.contains(id) && records.contains_key(id))
            .copied()
            .collect::<Vec<_>>()
    };
    let baseline = surviving(base);
    let proposal = surviving(proposed);
    let latest = surviving(current);
    if proposal != baseline && latest != baseline {
        return None;
    }
    let chosen = if proposal != baseline {
        &proposal
    } else {
        &latest
    };
    let mut edges: BTreeMap<ObjectId, BTreeSet<ObjectId>> =
        records.keys().map(|id| (*id, BTreeSet::new())).collect();
    for pair in chosen.windows(2) {
        edges.get_mut(&pair[0])?.insert(pair[1]);
    }
    for branch in [proposed, current] {
        let branch: Vec<_> = branch
            .iter()
            .filter(|id| records.contains_key(id))
            .copied()
            .collect();
        for pair in branch.windows(2) {
            if !original.contains(&pair[0]) || !original.contains(&pair[1]) {
                edges.get_mut(&pair[0])?.insert(pair[1]);
            }
        }
    }
    let mut degrees: BTreeMap<_, usize> = records.keys().map(|id| (*id, 0)).collect();
    for targets in edges.values() {
        for target in targets {
            *degrees.get_mut(target)? += 1;
        }
    }
    let mut ready: BTreeSet<_> = degrees
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut result = vec![];
    while let Some(id) = ready.pop_first() {
        result.push(id);
        for target in &edges[&id] {
            let degree = degrees.get_mut(target)?;
            *degree -= 1;
            if *degree == 0 {
                ready.insert(*target);
            }
        }
    }
    (result.len() == records.len()).then_some(result)
}
