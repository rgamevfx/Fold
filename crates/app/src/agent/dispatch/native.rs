//! Provider-owned project proposals share the same immutable candidate lifecycle.
use super::{Failure, operation_budget};
use crate::agent::wire::Scope;
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_project::{
    CommittedSnapshot, Document, EditBatch, Item, ItemId, Mutation, Project, Revision, Snapshot,
    proposals::Proposal,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Create {
        document: DocumentId,
        type_id: String,
        name: String,
    },
    AddLayer {
        composition: DocumentId,
        kind: String,
    },
    ConnectLayers {
        composition: DocumentId,
        source: Option<ObjectId>,
        target: ObjectId,
        port: String,
    },
    Author {
        document: DocumentId,
        expected: Value,
        edits: Vec<Value>,
    },
    Hierarchy {
        document: DocumentId,
        expected: Value,
        edits: Vec<fold_platform::hierarchy::Edit>,
    },
    CompositorSource {
        document: DocumentId,
        nodes: Option<Vec<ObjectId>>,
        expected: String,
        source: String,
    },
    ObjectSource {
        document: DocumentId,
        object: ObjectId,
        expected: String,
        source: String,
    },
}
#[derive(Clone)]
pub enum Candidate {
    Construction(Box<fold_motion::editing::Candidate>),
    Documents(Documents),
}
#[derive(Clone)]
pub struct Documents {
    pub(super) scope: Scope,
    before: CommittedSnapshot,
    after: fold_project::Candidate,
}
impl Candidate {
    pub fn snapshot(&self) -> &Snapshot {
        match self {
            Self::Construction(value) => value.snapshot(),
            Self::Documents(value) => value.after.snapshot(),
        }
    }
    pub fn validate_budget(&self) -> Result<(), Failure> {
        let registry = crate::packages::builtins();
        for document in self.snapshot().state().documents.values() {
            if !registry.supports(document) {
                continue;
            }
            if document.type_id == fold_motion::CONSTRUCTION {
                operation_budget(
                    &fold_motion::model::Construction::from_document(document)
                        .map_err(Failure::invalid)?,
                )?;
            } else if document.type_id == fold_motion::scene::TYPE {
                let scene =
                    fold_motion::scene::Scene::from_document(document).map_err(Failure::invalid)?;
                for object in scene.objects {
                    operation_budget(&object.graph)?;
                }
            }
        }
        Ok(())
    }
    /// Changing a publication can invalidate an unchanged consumer. Validate
    /// changed documents and their immediate consumers against the final snapshot;
    /// the provider service traverses each selected output's full dependency graph.
    pub fn validate_dependencies(&self, cancel: &Cancel) -> Result<(), Failure> {
        let changed: std::collections::BTreeSet<_> = match self {
            Self::Construction(value) => [value.document().id].into_iter().collect(),
            Self::Documents(value) => value
                .after
                .snapshot()
                .state()
                .documents
                .iter()
                .filter(|(id, document)| value.before.state().documents.get(id) != Some(document))
                .map(|(id, _)| *id)
                .collect(),
        };
        let registry = crate::packages::builtins();
        for document in self.snapshot().state().documents.values() {
            cancel.check().map_err(|e| Failure::new("cancelled", e))?;
            if !registry.supports(document)
                || !(changed.contains(&document.id)
                    || document
                        .dependencies
                        .iter()
                        .any(|source| changed.contains(&source.document)))
            {
                continue;
            }
            for output in registry.document_outputs(document) {
                registry
                    .validate_video_source(self.snapshot(), &output.reference, cancel)
                    .map_err(|error| Failure::invalid(error.to_string()))?;
            }
        }
        Ok(())
    }
}
impl Proposal for Candidate {
    type Scope = Scope;
    type Error = Failure;
    type Change = Value;
    fn scope(&self) -> Scope {
        match self {
            Self::Construction(value) => Scope {
                document: value.document().id,
                object: None,
                definition: match value.scope() {
                    fold_motion::editing::Scope::Document => None,
                    fold_motion::editing::Scope::Definition(id) => Some(id),
                },
            },
            Self::Documents(value) => value.scope,
        }
    }
    fn base_revision(&self) -> Revision {
        match self {
            Self::Construction(value) => value.base_revision(),
            Self::Documents(value) => value.before.revision(),
        }
    }
    fn document(&self) -> &Document {
        match self {
            Self::Construction(value) => value.document(),
            Self::Documents(value) => {
                &value.after.snapshot().state().documents[&value.scope.document]
            }
        }
    }
    fn source(&self) -> Result<String, Failure> {
        match self {
            Self::Construction(value) => value.source().map_err(Into::into),
            Self::Documents(value) => {
                if let Some(object) = value.scope.object {
                    let scene = fold_motion::scene::Scene::from_document(self.document())
                        .map_err(Failure::invalid)?;
                    return super::projection(
                        &scene.object(object).map_err(Failure::invalid)?.graph,
                        value.scope,
                    );
                }
                let documents = value
                    .after
                    .snapshot()
                    .state()
                    .documents
                    .iter()
                    .filter(|(id, document)| {
                        value.before.state().documents.get(id) != Some(document)
                    })
                    .map(|(_, document)| {
                        Ok(json!({
                            "document":document.id, "type":document.type_id,
                            "inspection":inspect(document)?
                        }))
                    })
                    .collect::<Result<Vec<_>, Failure>>()?;
                serde_json::to_string_pretty(&json!({"documents":documents}))
                    .map_err(|e| Failure::invalid(e.to_string()))
            }
        }
    }
    fn diff(&self) -> Result<Vec<Value>, Failure> {
        match self {
            Self::Construction(value) => value
                .diff()?
                .into_iter()
                .map(|change| {
                    serde_json::to_value(change).map_err(|e| Failure::invalid(e.to_string()))
                })
                .collect(),
            Self::Documents(value) => {
                if let Some(object) = value.scope.object {
                    let before = fold_motion::scene::Scene::from_document(
                        &value.before.state().documents[&value.scope.document],
                    )
                    .map_err(Failure::invalid)?;
                    let after = fold_motion::scene::Scene::from_document(self.document())
                        .map_err(Failure::invalid)?;
                    let changes = fold_motion::diff::construction(
                        &before.object(object).map_err(Failure::invalid)?.graph,
                        &after.object(object).map_err(Failure::invalid)?.graph,
                    )
                    .map_err(Failure::invalid)?;
                    return Ok(vec![
                        json!({"document":value.scope.document,"object":object,"changes":changes}),
                    ]);
                }
                value.after.snapshot().state().documents.iter()
                    .filter(|(id, document)| value.before.state().documents.get(id) != Some(document))
                    .map(|(id, document)| Ok(json!({
                        "document":id,
                        "before":value.before.state().documents.get(id).map(|d| inspect(d)).transpose()?,
                        "after":inspect(document)?
                    }))).collect()
            }
        }
    }
    fn rebase(&self, current: &CommittedSnapshot) -> Result<Self, Failure> {
        match self {
            Self::Construction(value) => Ok(Self::Construction(Box::new(value.rebase(current)?))),
            Self::Documents(value) => {
                let mut proposal = value.after.proposal();
                let mut documents = std::collections::BTreeSet::new();
                proposal.mutations.reverse();
                proposal.mutations.retain(|mutation| match mutation {
                    Mutation::PutDocument(document) => documents.insert(document.id),
                    _ => true,
                });
                proposal.mutations.reverse();
                for mutation in &mut proposal.mutations {
                    match mutation {
                        Mutation::PutDocument(document)
                            if current.state().documents.get(&document.id)
                                != value.before.state().documents.get(&document.id) =>
                        {
                            let before = value
                                .before
                                .state()
                                .documents
                                .get(&document.id)
                                .ok_or_else(|| {
                                    Failure::new(
                                        "conflict",
                                        "Created document identity already exists",
                                    )
                                })?;
                            let now =
                                current.state().documents.get(&document.id).ok_or_else(|| {
                                    Failure::new("conflict", "Edited document was removed")
                                })?;
                            if document.type_id == fold_compositor::COMPOSITE {
                                let decode = |d: &Document| {
                                    fold_compositor::Composite::from_document(d)
                                        .map_err(Failure::invalid)
                                };
                                let merged = fold_compositor::Composite::rebase(
                                    &decode(before)?,
                                    &decode(document)?,
                                    &decode(now)?,
                                )
                                .map_err(|e| Failure::new("conflict", e))?;
                                let extensions = fold_graph::reconcile::map(
                                    &before.extensions,
                                    &document.extensions,
                                    &now.extensions,
                                )
                                .map_err(|e| Failure::new("conflict", e))?;
                                *document =
                                    merged.document(document.id).map_err(Failure::invalid)?;
                                document.extensions = extensions;
                                continue;
                            }
                            if document.type_id != fold_motion::scene::TYPE {
                                return Err(Failure::new("conflict", "Edited document changed"));
                            }
                            let decode = |d: &Document| {
                                fold_motion::scene::Scene::from_document(d)
                                    .map_err(Failure::invalid)
                            };
                            let merged = fold_motion::scene::reconcile::merge(
                                &decode(before)?,
                                &decode(document)?,
                                &decode(now)?,
                            )
                            .map_err(|e| Failure::new("conflict", e))?;
                            *document = merged
                                .replace(now)
                                .map_err(|e| Failure::new("conflict", e))?;
                        }
                        Mutation::PutItem(item)
                            if current
                                .state()
                                .organization
                                .items
                                .iter()
                                .find(|i| i.id == item.id)
                                != value
                                    .before
                                    .state()
                                    .organization
                                    .items
                                    .iter()
                                    .find(|i| i.id == item.id) =>
                        {
                            return Err(Failure::new("conflict", "An edited project item changed"));
                        }
                        _ => {}
                    }
                }
                let after = fold_project::Candidate::new(current.clone())
                    .apply(EditBatch {
                        base: current.revision(),
                        mutations: proposal.mutations,
                    })
                    .map_err(Failure::invalid)?;
                let result = Self::Documents(Documents {
                    scope: value.scope,
                    before: current.clone(),
                    after,
                });
                result.validate_budget()?;
                Ok(result)
            }
        }
    }
    fn apply(self, project: &mut Project) -> Result<CommittedSnapshot, Failure> {
        match self {
            Self::Construction(value) => value.apply(project).map_err(Into::into),
            Self::Documents(value) => {
                if project.snapshot().revision() != value.before.revision() {
                    return Err(Failure::new("conflict", "Project changed"));
                }
                project
                    .commit_named(
                        value.after.proposal(),
                        "Edit Composition and scene graphs".into(),
                    )
                    .map_err(|e| Failure::invalid(e.to_string()))
            }
        }
    }
}
pub fn inspect(document: &Document) -> Result<Value, Failure> {
    let registry = crate::packages::builtins();
    let outputs = registry
        .document_outputs(document)
        .into_iter()
        .map(|output| json!({"reference":output.reference,"label":output.label,"info":output.info}))
        .collect::<Vec<_>>();
    let inputs =
        if registry.has_capability(document, fold_platform::packages::Capability::ImageInputs) {
            registry
                .document_image_inputs(document)
                .map_err(Failure::invalid)?
        } else {
            vec![]
        }
        .into_iter()
        .map(|port| json!({"id":port.id,"label":port.label,"source":port.source}))
        .collect::<Vec<_>>();
    let mut result = json!({"native":registry.inspect(document).map_err(Failure::invalid)?,"outputs":outputs,"image_inputs":inputs});
    if document.type_id == fold_compositor::COMPOSITE {
        result["graph_source"] = json!(
            fold_compositor::Composite::from_document(document)
                .map_err(Failure::invalid)?
                .source(None)
                .map_err(Failure::invalid)?
        );
    }
    if document.type_id == fold_motion::scene::TYPE {
        let scene = fold_motion::scene::Scene::from_document(document).map_err(Failure::invalid)?;
        result["object_sources"] = Value::Object(
            scene
                .objects
                .iter()
                .map(|object| {
                    Ok((
                        object.id.to_string(),
                        json!(
                            fold_motion::source::canonical(&object.graph)
                                .map_err(Failure::invalid)?
                        ),
                    ))
                })
                .collect::<Result<_, Failure>>()?,
        );
    }
    Ok(result)
}
pub(super) fn supported(kind: &str) -> bool {
    [
        fold_composition::COMPOSITION,
        fold_compositor::COMPOSITE,
        fold_motion::scene::TYPE,
    ]
    .contains(&kind)
}
pub fn prepare(
    snapshot: &CommittedSnapshot,
    scope: Scope,
    edits: &[Edit],
    cancel: &Cancel,
) -> Result<Candidate, Failure> {
    if scope.definition.is_some() || scope.object.is_some() || edits.is_empty() || edits.len() > 128
    {
        return Err(Failure::new(
            "type",
            "Project edits require document scope and 1..128 operations",
        ));
    }
    let registry = crate::packages::builtins();
    let mut after = fold_project::Candidate::new(snapshot.clone());
    for edit in edits {
        cancel.check().map_err(|e| Failure::new("cancelled", e))?;
        let current = after.snapshot();
        let document = |id: &DocumentId| -> Result<&Document, Failure> {
            let document = current
                .state()
                .documents
                .get(id)
                .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
            if !supported(&document.type_id) {
                return Err(Failure::new(
                    "unsupported",
                    "Unsupported project authoring provider",
                ));
            }
            Ok(document)
        };
        let mutations = match edit {
            Edit::Create {
                document: id,
                type_id,
                name,
            } => {
                if !supported(type_id) || current.state().documents.contains_key(id) {
                    return Err(Failure::new(
                        "conflict",
                        "Unsupported type or document identity already exists",
                    ));
                }
                if name.trim().is_empty() || name.len() > 256 {
                    return Err(Failure::new("type", "Document name requires 1..256 bytes"));
                }
                let created = registry
                    .create_with_defaults(current, type_id, *id)
                    .map_err(Failure::invalid)?;
                vec![
                    Mutation::PutDocument(created),
                    Mutation::PutItem(Item {
                        id: ItemId::Document(*id),
                        name: name.clone(),
                        parent: Default::default(),
                        order: current.state().organization.items.len() as u64,
                        extensions: Default::default(),
                    }),
                ]
            }
            Edit::AddLayer { composition, kind } => {
                let arguments = serde_json::to_vec(&crate::composition_layers::AddArgs {
                    document: *composition,
                    kind: kind.clone(),
                    layer: None,
                })
                .map_err(|e| Failure::invalid(e.to_string()))?;
                crate::composition_layers::add(current, &arguments, cancel)
                    .map_err(Failure::invalid)?
                    .mutations
            }
            Edit::ConnectLayers {
                composition,
                source,
                target,
                port,
            } => {
                let arguments = serde_json::to_vec(&crate::composition_layers::ConnectArgs {
                    document: *composition,
                    source: *source,
                    target: *target,
                    port: Some(port.clone()),
                })
                .map_err(|e| Failure::invalid(e.to_string()))?;
                crate::composition_layers::connect(current, &arguments, cancel)
                    .map_err(Failure::invalid)?
                    .mutations
            }
            Edit::Author {
                document: id,
                expected,
                edits,
            } => {
                let original = document(id)?;
                if inspect(original)? != *expected {
                    return Err(Failure::new("conflict", "Expected document differs"));
                }
                if edits
                    .iter()
                    .any(|edit| edit["op"] == "lock" && edit["locked"] == false)
                {
                    return Err(Failure::new("locked", "Agent edits cannot unlock controls"));
                }
                vec![Mutation::PutDocument(
                    registry
                        .author_cancellable(original, edits, cancel)
                        .map_err(Failure::invalid)?
                        .0,
                )]
            }
            Edit::Hierarchy {
                document: id,
                expected,
                edits,
            } => {
                let original = document(id)?;
                if inspect(original)? != *expected {
                    return Err(Failure::new("conflict", "Expected scene differs"));
                }
                if edits.iter().any(|edit| {
                    matches!(
                        edit,
                        fold_platform::hierarchy::Edit::Lock { value: false, .. }
                    )
                }) {
                    return Err(Failure::new("locked", "Agent edits cannot unlock objects"));
                }
                vec![Mutation::PutDocument(
                    registry
                        .edit_hierarchy(original, edits)
                        .map_err(Failure::invalid)?,
                )]
            }
            Edit::CompositorSource {
                document: id,
                nodes,
                expected,
                source,
            } => {
                let original = document(id)?;
                let mut graph = fold_compositor::Composite::from_document(original)
                    .map_err(Failure::invalid)?;
                if graph.source(nodes.as_deref()).map_err(Failure::invalid)? != *expected {
                    return Err(Failure::new("conflict", "Compositor source changed"));
                }
                graph
                    .replace_source(nodes.as_deref(), expected, source)
                    .map_err(Failure::invalid)?;
                let mut changed = graph.document(*id).map_err(Failure::invalid)?;
                changed.extensions = original.extensions.clone();
                vec![Mutation::PutDocument(changed)]
            }
            Edit::ObjectSource {
                document: id,
                object,
                expected,
                source,
            } => {
                if expected.len() > 65536 || source.len() > 65536 {
                    return Err(Failure::new("budget", "Object source exceeds 64 KiB"));
                }
                let original = document(id)?;
                let mut scene =
                    fold_motion::scene::Scene::from_document(original).map_err(Failure::invalid)?;
                let target = scene
                    .objects
                    .iter_mut()
                    .find(|o| o.id == *object)
                    .ok_or_else(|| Failure::new("missing_resource", "Missing scene object"))?;
                if fold_motion::source::canonical(&target.graph).map_err(Failure::invalid)?
                    != *expected
                {
                    return Err(Failure::new("conflict", "Object source changed"));
                }
                target.graph = fold_motion::source::parse(source)
                    .map_err(|e| Failure::invalid(e.message))?
                    .construction;
                vec![Mutation::PutDocument(
                    scene.replace(original).map_err(Failure::invalid)?,
                )]
            }
        };
        after = after
            .apply(EditBatch {
                base: current.revision(),
                mutations,
            })
            .map_err(Failure::invalid)?;
    }
    if !after
        .snapshot()
        .state()
        .documents
        .contains_key(&scope.document)
    {
        return Err(Failure::new(
            "missing_resource",
            "Candidate scope requires a resulting document",
        ));
    }
    let candidate = Candidate::Documents(Documents {
        scope,
        before: snapshot.clone(),
        after,
    });
    candidate.validate_budget()?;
    Ok(candidate)
}
