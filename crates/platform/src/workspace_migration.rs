//! Upgrade explicit-follow and single-owner groups to independent viewer sources.
use super::{LinkGroup, PanelInstanceId};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn upgrade_legacy(mut value: Value) -> Result<Value, String> {
    match value.get("version").and_then(Value::as_u64) {
        Some(3) => return Ok(value),
        Some(2) => {
            value["version"] = json!(3);
            return Ok(value); // Missing playback overrides follow provider defaults.
        }
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
    value["version"] = json!(3);
    Ok(value)
}

pub(super) fn upgrade(value: Value) -> Result<Value, String> {
    if value.get("version").and_then(Value::as_u64) == Some(4) {
        return Ok(value);
    }
    let mut value = upgrade_legacy(value)?;
    let sources = value["group_sources"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let editors = value["editors"].as_object_mut().ok_or("missing editors")?;
    let mut occupied = std::collections::BTreeSet::new();
    let mut ids: Vec<_> = editors.keys().cloned().collect();
    ids.sort_by_key(|id| id.parse::<u64>().unwrap_or(0));
    for id in ids {
        let e = editors.get_mut(&id).unwrap();
        let contribution = e["contribution"].as_str().unwrap_or("").to_owned();
        let group = e["group"].as_str().unwrap_or("A").to_owned();
        let group = if occupied.contains(&(group.clone(), contribution.clone())) {
            ["A", "B", "C", "D"]
                .into_iter()
                .find(|g| !occupied.contains(&(g.to_string(), contribution.clone())))
                .unwrap_or("Unlinked")
                .to_owned()
        } else {
            group
        };
        if group != "Unlinked" {
            occupied.insert((group.clone(), contribution));
        }
        e["group"] = json!(group);
    }
    let editors = editors.clone();
    for viewer in value["viewers"]
        .as_object_mut()
        .ok_or("missing viewers")?
        .values_mut()
    {
        if viewer["binding"] == "Linked" {
            let group = viewer["group"].as_str().unwrap_or("A");
            if let Some(editor) = sources
                .get(group)
                .and_then(Value::as_u64)
                .and_then(|id| editors.get(&id.to_string()))
            {
                viewer["source"] = editor["contribution"].clone();
                viewer["group"] = editor["group"].clone();
                if editor["group"] != "Unlinked" {
                    continue;
                }
                if let Some(document) = editor["navigation"]
                    .as_array()
                    .and_then(|n| n.last())
                    .and_then(|n| n.get("document"))
                {
                    viewer["binding"] =
                        json!({"Pinned": {"document": document, "output": "video"}});
                    continue;
                }
            }
            viewer["binding"] = viewer
                .get("last_output")
                .filter(|o| !o.is_null())
                .map(|o| json!({"Pinned": o}))
                .unwrap_or(json!("Unbound"));
        }
    }
    let inspected = value["inspector_group"].as_str().unwrap_or("A");
    if let Some(editor) = sources
        .get(inspected)
        .and_then(Value::as_u64)
        .and_then(|id| editors.get(&id.to_string()))
    {
        value["inspector_group"] = editor["group"].clone();
    }
    let mut selections = serde_json::Map::new();
    for id in sources.values() {
        if let Some(editor) = id.as_u64().and_then(|id| editors.get(&id.to_string())) {
            selections.insert(editor["group"].as_str().unwrap_or("A").into(), id.clone());
        }
    }
    value.as_object_mut().unwrap().remove("group_sources");
    value["group_selections"] = Value::Object(selections);
    value["version"] = json!(4);
    Ok(value)
}
