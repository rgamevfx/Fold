//! Version-1 explicit-follow bindings become link groups without changing targets.
use super::{LinkGroup, PanelInstanceId};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn upgrade(mut value: Value) -> Result<Value, String> {
    match value.get("version").and_then(Value::as_u64) {
        Some(2) => return Ok(value),
        Some(1) => {}
        _ => return Err("incompatible workspace version".into()),
    }
    let mut assigned = BTreeMap::<PanelInstanceId, LinkGroup>::new();
    let mut viewers = value
        .get("viewers")
        .and_then(Value::as_object)
        .cloned()
        .ok_or("missing workspace viewers")?;
    let editors = value
        .get_mut("editors")
        .and_then(Value::as_object_mut)
        .ok_or("missing workspace editors")?;
    // Stable numeric order, independent of JSON member ordering.
    let mut ids: Vec<_> = viewers
        .keys()
        .map(|s| s.parse::<u64>().map(|id| (id, s.clone())))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    ids.sort();
    for (_, id) in ids {
        let viewer = viewers.get_mut(&id).unwrap();
        let Some(owner) = viewer.get("binding").and_then(|b| b.get("Follow")) else {
            continue;
        };
        let owner = PanelInstanceId(owner.as_u64().ok_or("invalid legacy follow identity")?);
        let editor = editors.get_mut(&owner.0.to_string());
        let group = assigned.get(&owner).copied().or_else(|| {
            editor.as_ref()?;
            LinkGroup::ALL.get(assigned.len()).copied()
        });
        if let Some(group) = group {
            assigned.insert(owner, group);
            editor.unwrap()["group"] = json!(group);
            viewer["group"] = json!(group);
            viewer["binding"] = json!("Linked");
            viewer["editor"] = Value::Null;
        } else {
            // More than four independent legacy sources are frozen, not merged.
            let output = editor
                .and_then(|e| {
                    e.get("navigation")
                        .and_then(Value::as_array)?
                        .last()?
                        .get("document")
                        .cloned()
                })
                .map(|document| json!({"document": document, "output": "video"}))
                .or_else(|| viewer.get("last_output").filter(|v| !v.is_null()).cloned());
            viewer["binding"] = output
                .map(|o| json!({"Pinned": o}))
                .unwrap_or(json!("Unbound"));
        }
    }
    let groups: serde_json::Map<_, _> = assigned
        .iter()
        .map(|(id, group)| (group.label().into(), json!(id)))
        .collect();
    let inspector_group = value
        .get("focused_editor")
        .and_then(Value::as_u64)
        .and_then(|id| assigned.get(&PanelInstanceId(id)))
        .copied()
        .unwrap_or_default();
    value["viewers"] = Value::Object(viewers);
    value["group_sources"] = Value::Object(groups);
    value["inspector_group"] = json!(inspector_group);
    value["version"] = json!(2);
    Ok(value)
}
