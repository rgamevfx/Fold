//! Revision-bound edits carry changed authored values, not unchanged resources.
//! The host checks the base revision before applying these package-owned changes.
use crate::Motion;
use fold_foundation::DocumentId;
use fold_media::Cancel;
use fold_project::{EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Part {
    Key(String),
    Index(usize),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Change {
    Set { path: Vec<Part>, value: Value },
    Remove { path: Vec<Part> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Changes {
    document: DocumentId,
    changes: Vec<Change>,
}
impl Changes {
    pub fn between(document: DocumentId, before: &Motion, after: &Motion) -> Result<Self, String> {
        let mut changes = Vec::new();
        // Compare typed fields first: unchanged embedded resources never become JSON arrays.
        fn field<T: PartialEq + Serialize>(
            name: &str,
            before: &T,
            after: &T,
            changes: &mut Vec<Change>,
        ) -> Result<(), String> {
            if before != after {
                diff(
                    &serde_json::to_value(before).map_err(|e| e.to_string())?,
                    &serde_json::to_value(after).map_err(|e| e.to_string())?,
                    &mut vec![Part::Key(name.into())],
                    changes,
                );
            }
            Ok(())
        }
        field("info", &before.info, &after.info, &mut changes)?;
        field("graph", &before.graph, &after.graph, &mut changes)?;
        field("scene", &before.scene, &after.scene, &mut changes)?;
        field("fonts", &before.fonts, &after.fonts, &mut changes)?;
        field("controls", &before.controls, &after.controls, &mut changes)?;
        field("groups", &before.groups, &after.groups, &mut changes)?;
        if before.extensions != after.extensions {
            diff(
                &serde_json::to_value(&before.extensions).map_err(|e| e.to_string())?,
                &serde_json::to_value(&after.extensions).map_err(|e| e.to_string())?,
                &mut Vec::new(),
                &mut changes,
            );
        }
        Ok(Self { document, changes })
    }
}
fn diff(before: &Value, after: &Value, path: &mut Vec<Part>, changes: &mut Vec<Change>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().filter(|key| !b.contains_key(*key)) {
                path.push(Part::Key(key.clone()));
                changes.push(Change::Remove { path: path.clone() });
                path.pop();
            }
            for (key, value) in b {
                path.push(Part::Key(key.clone()));
                if let Some(old) = a.get(key) {
                    diff(old, value, path, changes);
                } else {
                    changes.push(Change::Set {
                        path: path.clone(),
                        value: value.clone(),
                    });
                }
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (old, value)) in a.iter().zip(b).enumerate() {
                path.push(Part::Index(index));
                diff(old, value, path, changes);
                path.pop();
            }
        }
        _ => changes.push(Change::Set {
            path: path.clone(),
            value: after.clone(),
        }),
    }
}
fn apply(root: &mut Value, change: Change) -> Result<(), String> {
    let (path, value) = match change {
        Change::Set { path, value } => (path, Some(value)),
        Change::Remove { path } => (path, None),
    };
    let Some((last, parents)) = path.split_last() else {
        *root = value.ok_or("cannot remove a motion field")?;
        return Ok(());
    };
    let mut target = root;
    for part in parents {
        target = match part {
            Part::Key(key) => target.as_object_mut().and_then(|o| o.get_mut(key)),
            Part::Index(index) => target.as_array_mut().and_then(|a| a.get_mut(*index)),
        }
        .ok_or("motion change path does not exist")?;
    }
    match (target, last, value) {
        (Value::Object(object), Part::Key(key), Some(value)) => {
            object.insert(key.clone(), value);
        }
        (Value::Object(object), Part::Key(key), None) => {
            object
                .remove(key)
                .ok_or("motion change field does not exist")?;
        }
        (Value::Array(array), Part::Index(index), Some(value)) => {
            *array
                .get_mut(*index)
                .ok_or("motion change index out of range")? = value;
        }
        _ => return Err("invalid motion change path".into()),
    }
    Ok(())
}
pub(crate) fn stage(
    snapshot: &Snapshot,
    args: &[u8],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    let args: Changes = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    let previous = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("missing motion document")?;
    // Normalize older schemas through the same loader as the editing UI. Only
    // changed fields become JSON values; embedded resources stay typed bytes.
    let mut motion = Motion::from_document(previous)?;
    let mut fields = BTreeMap::<String, Vec<Change>>::new();
    let mut extensions = Vec::new();
    for mut change in args.changes {
        cancel.check()?;
        let path = match &mut change {
            Change::Set { path, .. } | Change::Remove { path } => path,
        };
        let Some(Part::Key(field)) = path.first() else {
            return Err("motion change needs a field path".into());
        };
        if matches!(
            field.as_str(),
            "info" | "graph" | "scene" | "fonts" | "controls" | "groups"
        ) {
            let field = field.clone();
            path.remove(0);
            fields.entry(field).or_default().push(change);
        } else {
            extensions.push(change);
        }
    }
    for (field, changes) in fields {
        match field.as_str() {
            "info" => apply_changes(&mut motion.info, changes, cancel)?,
            "graph" => apply_changes(&mut motion.graph, changes, cancel)?,
            "scene" => apply_changes(&mut motion.scene, changes, cancel)?,
            "fonts" => apply_changes(&mut motion.fonts, changes, cancel)?,
            "controls" => apply_changes(&mut motion.controls, changes, cancel)?,
            "groups" => apply_changes(&mut motion.groups, changes, cancel)?,
            _ => unreachable!("partitioned authored field"),
        }
    }
    if !extensions.is_empty() {
        apply_changes(&mut motion.extensions, extensions, cancel)?;
    }
    let mut document = motion.document(args.document)?;
    document.extensions = previous.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}

fn apply_changes<T: Serialize + DeserializeOwned>(
    field: &mut T,
    changes: Vec<Change>,
    cancel: &Cancel,
) -> Result<(), String> {
    let mut value = serde_json::to_value(&*field).map_err(|e| e.to_string())?;
    for change in changes {
        cancel.check()?;
        apply(&mut value, change)?;
    }
    *field = serde_json::from_value(value).map_err(|e| e.to_string())?;
    Ok(())
}
