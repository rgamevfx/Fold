//! Deliberate changes to an asset interface. Called only on a private candidate.
use super::*;
use crate::fields::Datum;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Migration {
    /// Preserve values, connections, animation and generated input identity.
    Rename { from: String, to: String },
    /// Explicitly discard the removed input's values, connections and keys.
    Remove { input: String },
    /// Explicitly discard old bindings/keys and initialize the new typed value.
    Replace {
        from: String,
        to: String,
        value: Datum,
    },
}
impl Migration {
    fn from(&self) -> &str {
        match self {
            Self::Rename { from, .. } | Self::Replace { from, .. } => from,
            Self::Remove { input } => input,
        }
    }
    fn to(&self) -> Option<&str> {
        match self {
            Self::Rename { to, .. } | Self::Replace { to, .. } => Some(to),
            Self::Remove { .. } => None,
        }
    }
}
pub(super) fn validate(
    old: Option<&GroupDefinition>,
    new: &[Port],
    edits: &[Migration],
) -> Result<(), String> {
    let Some(old) = old else {
        return if edits.is_empty() {
            Ok(())
        } else {
            Err("migration requires an existing definition".into())
        };
    };
    let mut sources = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for edit in edits {
        let source = old
            .inputs
            .iter()
            .find(|p| p.id == edit.from())
            .ok_or("migration source input is missing")?;
        if !sources.insert(edit.from()) {
            return Err("duplicate migration source".into());
        }
        if let Some(to) = edit.to() {
            if !targets.insert(to) {
                return Err("duplicate migration destination".into());
            }
            if to != edit.from() && old.inputs.iter().any(|p| p.id == to) {
                return Err(
                    "migration destination collides with an existing input; use separate revisions"
                        .into(),
                );
            }
            let dest = new
                .iter()
                .find(|p| p.id == to)
                .ok_or("migration destination is missing from new source")?;
            match edit {
                Migration::Rename { .. } if source.kind != dest.kind => {
                    return Err(
                        "rename must retain the input type; use replace with an explicit value"
                            .into(),
                    );
                }
                Migration::Replace { value, .. } => {
                    value.validate()?;
                    if value.kind() != dest.kind {
                        return Err("migration replacement value has wrong type".into());
                    }
                }
                _ => {}
            }
        }
        if edit.to() != Some(edit.from()) && new.iter().any(|p| p.id == edit.from()) {
            return Err("migration source input still exists in new source".into());
        }
    }
    for p in &old.inputs {
        if !sources.contains(p.id.as_str()) && !new.iter().any(|n| n.id == p.id && n.kind == p.kind)
        {
            return Err(format!(
                "asset interface conflict: removed or changed input {}; explicit migration required",
                p.id
            ));
        }
    }
    Ok(())
}
pub(super) fn identities(ids: &mut BTreeMap<String, ObjectId>, edits: &[Migration]) {
    for edit in edits {
        if let Migration::Rename { from, to } = edit {
            if let Some(id) = ids.remove(&format!("@{from}")) {
                ids.insert(format!("@{to}"), id);
            }
        }
    }
}
pub(super) fn instances(
    m: &mut Motion,
    definition: ObjectId,
    edits: &[Migration],
) -> Result<(), String> {
    if edits.is_empty() {
        return Ok(());
    }
    for (id, group) in &m.groups {
        if *id != definition
            && record(m, *id)?.is_some()
            && group.graph.nodes.iter().any(|n| {
                n.kind == "fold.motion.group_instance"
                    && n.settings.get("group") == Some(&json!(definition))
            })
        {
            return Err("migration affects another source-owned asset; coordinated source revisions are not supported; retain the interface or create a new definition".into());
        }
    }
    for n in m
        .graph
        .nodes
        .iter_mut()
        .chain(m.groups.values_mut().flat_map(|g| g.graph.nodes.iter_mut()))
    {
        if n.kind != "fold.motion.group_instance"
            || n.settings.get("group") != Some(&json!(definition))
        {
            continue;
        }
        for edit in edits {
            let binding = n.inputs.remove(edit.from());
            if let Some(visibility) = n
                .extensions
                .get_mut("fold.motion.input_visibility")
                .and_then(Value::as_object_mut)
            {
                if let Some(value) = visibility.remove(edit.from()) {
                    if let Some(to) = edit.to() {
                        visibility.insert(to.into(), value);
                    }
                }
            }
            let prefix = format!("{}.", edit.from());
            let paths: Vec<_> = n
                .animation
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            let curves: Vec<_> = paths
                .into_iter()
                .map(|p| {
                    let c = n.animation.remove(&p).unwrap();
                    (p, c)
                })
                .collect();
            match edit {
                Migration::Rename { to, .. } => {
                    if let Some(binding) = binding {
                        n.inputs.insert(to.clone(), binding);
                    }
                    for (path, curve) in curves {
                        n.animation
                            .insert(format!("{to}.{}", &path[prefix.len()..]), curve);
                    }
                }
                Migration::Replace { to, value, .. } => {
                    n.inputs.insert(to.clone(), Input::Value(value.clone()));
                }
                Migration::Remove { .. } => {}
            }
        }
    }
    Ok(())
}
