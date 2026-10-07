use super::*;
use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_project::{EditBatch, Item, ItemId, Mutation, Parent, Project, Revision};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub request_id: String,
    pub revision: Revision,
    pub project_epoch: u64,
    pub label: String,
    #[serde(default)]
    pub ids: Vec<String>,
    pub operations: Vec<Value>,
    #[serde(default)]
    pub validate_only: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Forget {
        namespace: String,
        keys: Vec<String>,
    },
    Remember {
        document: DocumentId,
        namespace: String,
        bindings: Value,
    },
    Create {
        id: DocumentId,
        kind: String,
        name: String,
    },
    Edit {
        document: DocumentId,
        edits: Vec<Value>,
    },
    Command {
        id: String,
        arguments: Value,
    },
    Remove {
        document: DocumentId,
    },
    Rename {
        document: DocumentId,
        name: String,
    },
    Place {
        source: DocumentId,
        target: DocumentId,
        at: Time,
        #[serde(default)]
        source_start: Option<Time>,
        #[serde(default)]
        duration: Option<Time>,
    },
    PlaceAsset {
        source: AssetId,
        target: DocumentId,
        at: Time,
        #[serde(default)]
        audio_only: bool,
    },
    Import {
        paths: Vec<std::path::PathBuf>,
    },
}
fn resolve(v: &mut Value, ids: &BTreeMap<String, Value>) -> Result<(), String> {
    match v {
        Value::Object(o) if o.len() == 1 && o.contains_key("$ref") => {
            let key = o["$ref"].as_str().ok_or("$ref must be a name")?;
            *v = ids
                .get(key)
                .ok_or_else(|| format!("undefined batch reference {key}"))?
                .clone();
        }
        Value::Object(o) => {
            for value in o.values_mut() {
                resolve(value, ids)?;
            }
        }
        Value::Array(a) => {
            for value in a {
                resolve(value, ids)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn stage(
    snapshot: &CommittedSnapshot,
    batch: &Batch,
    cancel: &Cancel,
) -> Result<(EditBatch, Value), String> {
    if batch.revision != snapshot.revision() {
        return Err("revision conflict; inspect current state".into());
    }
    if batch.operations.is_empty()
        || batch.operations.len() > 128
        || batch.ids.len() > 512
        || batch.label.is_empty()
        || batch.label.len() > 160
    {
        return Err(
            "batch requires 1..128 operations, <=512 IDs and a 1..160 character label".into(),
        );
    }
    let mut ids = BTreeMap::new();
    for name in &batch.ids {
        if name.is_empty()
            || name.len() > 80
            || ids.insert(name.clone(), json!(ObjectId::new())).is_some()
        {
            return Err("batch ID names must be unique and 1..80 characters".into());
        }
    }
    let registry = crate::packages::builtins();
    let mut workspace = Project::fork(snapshot);
    let mut mutations = Vec::new();
    let mut receipts = Vec::new();
    for (i, operation) in batch.operations.iter().enumerate() {
        cancel.check()?;
        let mut value = operation.clone();
        resolve(&mut value, &ids)?;
        let snap = workspace.snapshot();
        let changes = match decode::<Operation>(&value)
            .map_err(|e| format!("operation {i}: {e}"))?
        {
            Operation::Remember {
                document,
                namespace,
                bindings,
            } => {
                vec![Mutation::SetSettings(super::handles::remember(
                    &snap, document, &namespace, bindings,
                )?)]
            }
            Operation::Forget { namespace, keys } => vec![Mutation::SetSettings(
                super::handles::forget(&snap, &namespace, &keys)?,
            )],
            Operation::Create { id, kind, name } => {
                if snap.state().documents.contains_key(&id) {
                    return Err("document ID already exists".into());
                }
                vec![
                    Mutation::PutDocument(registry.create(&kind, id)?),
                    Mutation::PutItem(Item {
                        id: ItemId::Document(id),
                        name,
                        parent: Parent::Root,
                        order: snap.state().organization.items.len() as u64,
                        extensions: Default::default(),
                    }),
                ]
            }
            Operation::Edit { document, edits } => {
                if edits.len() > 512 {
                    return Err("at most 512 edits per document operation".into());
                }
                let old = snap
                    .state()
                    .documents
                    .get(&document)
                    .ok_or("missing document")?;
                let (document, receipt) = registry
                    .author_cancellable(old, &edits, cancel)
                    .map_err(|e| format!("operation {i}: {e}"))?;
                receipts.push(json!({"operation":i,"document":document.id,"changes":receipt}));
                vec![Mutation::PutDocument(document)]
            }
            Operation::Command { id, arguments } => {
                registry
                    .stage(
                        &snap,
                        &fold_platform::packages::CommandRequest {
                            id,
                            base: snap.revision(),
                            arguments: serde_json::to_vec(&arguments).map_err(|e| e.to_string())?,
                        },
                        cancel,
                    )?
                    .mutations
            }
            Operation::Remove { document } => vec![Mutation::RemoveDocument(document)],
            Operation::Rename { document, name } => {
                let mut item = snap
                    .state()
                    .organization
                    .items
                    .iter()
                    .find(|i| i.id == ItemId::Document(document))
                    .ok_or("missing project item")?
                    .clone();
                item.name = name;
                vec![Mutation::PutItem(item)]
            }
            Operation::Place {
                source,
                target,
                at,
                source_start,
                duration,
            } => {
                registry
                    .place(
                        &snap,
                        &fold_platform::browser::Placement {
                            source: fold_platform::browser::PlacementSource::Output(
                                fold_project::DocumentRef {
                                    document: source,
                                    output: "video".into(),
                                    extensions: Default::default(),
                                },
                            ),
                            target,
                            track: None,
                            at,
                            source_start: source_start.unwrap_or(Time::ZERO),
                            duration,
                            audio_only: false,
                            new_track: false,
                            mode: Default::default(),
                        },
                    )?
                    .mutations
            }
            Operation::PlaceAsset {
                source,
                target,
                at,
                audio_only,
            } => {
                registry
                    .place(
                        &snap,
                        &fold_platform::browser::Placement {
                            source: fold_platform::browser::PlacementSource::Asset(source),
                            target,
                            track: None,
                            at,
                            source_start: Time::ZERO,
                            duration: None,
                            audio_only,
                            new_track: false,
                            mode: Default::default(),
                        },
                    )?
                    .mutations
            }
            Operation::Import { paths } => {
                crate::ingest::import(
                    &snap,
                    &crate::ingest::ImportRequest {
                        base: snap.revision(),
                        paths,
                        destination: Parent::Root,
                    },
                    cancel,
                )?
                .into_batch()?
                .mutations
            }
        };
        cancel.check()?;
        workspace
            .commit(EditBatch {
                base: snap.revision(),
                mutations: changes.clone(),
            })
            .map_err(|e| format!("operation {i}: {e}"))?;
        mutations.extend(changes);
    }
    Ok((
        EditBatch {
            base: snapshot.revision(),
            mutations,
        },
        json!({"ids":ids,"label":batch.label,"validated":true,"receipts":receipts}),
    ))
}
pub fn examples() -> Value {
    json!({"request_id":"unique-client-request","revision":0,"project_epoch":0,"label":"Build title","ids":["motion","text"],"operations":[
     {"op":"create","id":{"$ref":"motion"},"kind":"fold.motion.document","name":"Title"},
     {"op":"edit","document":{"$ref":"motion"},"edits":[{"op":"object","id":{"$ref":"text"},"kind":"fold.motion.text","name":"Title"}]}
    ],"reference_syntax":"Use {\"$ref\":\"name\"} for batch-allocated identities. Literal strings, including $text artwork, are never rewritten.","other_operations":[
     {"op":"rename","document":"UUID","name":"New name"},{"op":"remove","document":"UUID"},
     {"op":"place","source":"UUID","target":"UUID","at":{"numerator":0,"denominator":1}},
     {"op":"import","paths":["/absolute/media.mov"]},
     {"op":"place_asset","source":"UUID","target":"UUID","at":{"numerator":0,"denominator":1},"audio_only":false},
     {"op":"command","id":"fold.app.insert-document","arguments":{"source":"UUID","sequence":"UUID","at":{"numerator":0,"denominator":1}}}
    ]})
}

pub fn operation_schema() -> Value {
    fold_platform::authoring::reference_schema(
        serde_json::to_value(schemars::schema_for!(Operation)).expect("operation schema"),
    )
}
