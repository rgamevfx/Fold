//! Atomic cross-document coordination. All creative policy belongs to providers.
use super::Failure;
use crate::agent::wire::Scope;
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::proposals::{InspectionRequest, PreparedProposal, ScopedRequest};
use fold_project::{
    CommittedSnapshot, Document, EditBatch, Item, ItemId, Mutation, Project, Revision, Snapshot,
    proposals::Proposal,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectEdit {
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
}
#[derive(Clone)]
pub struct Candidate {
    scope: Scope,
    before: CommittedSnapshot,
    after: fold_project::Candidate,
    prepared: Vec<Arc<dyn PreparedProposal>>,
    direct: bool,
}
fn check(cancel: &Cancel) -> Result<(), Failure> {
    cancel.check().map_err(|e| Failure::new("cancelled", e))
}
fn replacements(
    prepared: &dyn PreparedProposal,
    base: &Snapshot,
    cancel: &Cancel,
) -> Result<Vec<Mutation>, Failure> {
    let changes = prepared
        .publication(base, cancel)
        .map_err(|e| e.context(&prepared.scope().package, prepared.scope().target))?;
    if changes.base != base.revision()
        || changes
            .documents
            .iter()
            .any(|d| !prepared.scope().writes.contains(&d.id))
    {
        return Err(Failure::new(
            "conflict",
            "Provider publication exceeds scope or revision",
        ));
    }
    let inspection = prepared.inspect(&InspectionRequest::all(prepared.scope().target), cancel)?;
    if inspection.identity != changes.identity {
        return Err(Failure::new(
            "conflict",
            "Provider publication differs from inspected content",
        ));
    }
    Ok(changes
        .documents
        .into_iter()
        .map(Mutation::PutDocument)
        .collect())
}
impl Candidate {
    pub fn snapshot(&self) -> &Snapshot {
        self.after.snapshot()
    }
    #[cfg(test)]
    pub(super) fn from_document(
        base: &CommittedSnapshot,
        scope: Scope,
        document: &Document,
        cancel: &Cancel,
    ) -> Result<Self, Failure> {
        let registry = crate::packages::builtins();
        let prepared = registry
            .scoped_proposals(document)?
            .prepare_change(base, scope, document, cancel)?;
        Self::from_prepared(base, scope, prepared, cancel)
    }
    fn from_prepared(
        base: &CommittedSnapshot,
        scope: Scope,
        prepared: Box<dyn PreparedProposal>,
        cancel: &Cancel,
    ) -> Result<Self, Failure> {
        let after = fold_project::Candidate::new(base.clone())
            .apply(EditBatch {
                base: base.revision(),
                mutations: replacements(prepared.as_ref(), base, cancel)?,
            })
            .map_err(Failure::invalid)?;
        Ok(Self {
            scope,
            before: base.clone(),
            after,
            prepared: vec![prepared.into()],
            direct: true,
        })
    }
    pub(super) fn from_batch(
        base: &CommittedSnapshot,
        scope: Scope,
        batch: EditBatch,
        cancel: &Cancel,
    ) -> Result<Self, Failure> {
        let after = fold_project::Candidate::new(base.clone())
            .apply(batch)
            .map_err(Failure::invalid)?;
        Self::seal(base, scope, after, cancel)
    }
    fn seal(
        base: &CommittedSnapshot,
        scope: Scope,
        after: fold_project::Candidate,
        cancel: &Cancel,
    ) -> Result<Self, Failure> {
        let registry = crate::packages::builtins();
        let mut prepared = Vec::new();
        for (id, document) in &after.snapshot().state().documents {
            if base.state().documents.get(id) == Some(document) {
                continue;
            }
            prepared.push(Arc::from(
                registry.scoped_proposals(document)?.prepare_change(
                    base,
                    Scope { document: *id },
                    document,
                    cancel,
                )?,
            ));
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
        Ok(Self {
            scope,
            before: base.clone(),
            after,
            prepared,
            direct: false,
        })
    }
    pub fn validate_dependencies(&self, cancel: &Cancel) -> Result<(), Failure> {
        let changed: BTreeSet<_> = self
            .prepared
            .iter()
            .flat_map(|p| p.scope().writes.iter().copied())
            .collect();
        let registry = crate::packages::builtins();
        for document in self.snapshot().state().documents.values() {
            check(cancel)?;
            if !registry.supports(document)
                || !(changed.contains(&document.id)
                    || document
                        .dependencies
                        .iter()
                        .any(|d| changed.contains(&d.document)))
            {
                continue;
            }
            for output in registry.document_outputs(document) {
                registry
                    .validate_video_source(self.snapshot(), &output.reference, cancel)
                    .map_err(|e| Failure::invalid(e.to_string()))?;
            }
        }
        Ok(())
    }
    pub fn reconcile(&self, current: &CommittedSnapshot, cancel: &Cancel) -> Result<Self, Failure> {
        check(cancel)?;
        let mut prepared: Vec<Arc<dyn PreparedProposal>> = Vec::new();
        let mut mutations = Vec::new();
        for p in &self.prepared {
            let next = p
                .reconcile(current, cancel)
                .map_err(|e| e.context(&p.scope().package, p.scope().target))?;
            mutations.extend(replacements(next.as_ref(), current, cancel)?);
            prepared.push(next.into());
        }
        for mutation in self.after.proposal().mutations {
            if let Mutation::PutItem(item) = &mutation
                && current
                    .state()
                    .organization
                    .items
                    .iter()
                    .find(|i| i.id == item.id)
                    != self
                        .before
                        .state()
                        .organization
                        .items
                        .iter()
                        .find(|i| i.id == item.id)
            {
                return Err(Failure::new("conflict", "An edited project item changed"));
            }
            if let Mutation::PutAsset(asset) = &mutation
                && current.state().assets.get(&asset.id)
                    != self.before.state().assets.get(&asset.id)
            {
                return Err(Failure::new("conflict", "A component asset changed"));
            }
            if !matches!(mutation, Mutation::PutDocument(_)) {
                mutations.push(mutation);
            }
        }
        let after = fold_project::Candidate::new(current.clone())
            .apply(EditBatch {
                base: current.revision(),
                mutations,
            })
            .map_err(Failure::invalid)?;
        Ok(Self {
            scope: self.scope,
            before: current.clone(),
            after,
            prepared,
            direct: self.direct,
        })
    }
}
impl Proposal for Candidate {
    type Scope = Scope;
    type Error = Failure;
    type Change = Value;
    fn scope(&self) -> Scope {
        self.scope
    }
    fn base_revision(&self) -> Revision {
        self.before.revision()
    }
    fn document(&self) -> &Document {
        &self.after.snapshot().state().documents[&self.scope.document]
    }
    fn source(&self) -> Result<String, Failure> {
        let cancel = Cancel::default();
        if self.direct {
            return Ok(self.prepared[0]
                .inspect(&InspectionRequest::all(self.scope), &cancel)?
                .source);
        }
        let documents = self
            .prepared
            .iter()
            .map(|p| {
                let id = p.scope().target.document;
                let document = &self.snapshot().state().documents[&id];
                let inspection = p.inspect(&InspectionRequest::all(p.scope().target), &cancel)?;
                let mut value = inspection
                    .native
                    .unwrap_or_else(|| json!({"source":inspection.source}));
                decorate(&crate::packages::builtins(), document, &mut value)?;
                Ok(json!({"document":id,"type":document.type_id,"inspection":value}))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        serde_json::to_string_pretty(&json!({"documents":documents}))
            .map_err(|e| Failure::invalid(e.to_string()))
    }
    fn diff(&self) -> Result<Vec<Value>, Failure> {
        let mut changes = Vec::new();
        for p in &self.prepared {
            changes.extend(
                p.inspect(
                    &InspectionRequest::all(p.scope().target),
                    &Cancel::default(),
                )?
                .changes,
            );
        }
        Ok(changes)
    }
    fn rebase(&self, current: &CommittedSnapshot) -> Result<Self, Failure> {
        self.reconcile(current, &Cancel::default())
    }
    fn apply(self, project: &mut Project) -> Result<CommittedSnapshot, Failure> {
        let current = project.snapshot();
        let cancel = Cancel::default();
        if current.revision() != self.before.revision() {
            return Err(Failure::new("conflict", "Project changed"));
        }
        let mut mutations = Vec::new();
        for p in &self.prepared {
            mutations.extend(replacements(p.as_ref(), &current, &cancel)?);
        }
        mutations.extend(
            self.after
                .proposal()
                .mutations
                .into_iter()
                .filter(|m| !matches!(m, Mutation::PutDocument(_))),
        );
        self.validate_dependencies(&cancel)?;
        project
            .commit_named(
                EditBatch {
                    base: current.revision(),
                    mutations,
                },
                if self.direct {
                    self.prepared[0].undo_label()
                } else {
                    "Edit creative documents"
                }
                .into(),
            )
            .map_err(|e| Failure::invalid(e.to_string()))
    }
}
fn content(
    document: &Document,
    scope: Scope,
    cancel: &Cancel,
) -> Result<fold_platform::proposals::DocumentInspection, Failure> {
    let registry = crate::packages::builtins();
    let mut content = registry
        .scoped_proposals(document)?
        .inspect(document, scope, cancel)?;
    if let fold_platform::proposals::DocumentInspection::Native(value) = &mut content {
        decorate(&registry, document, value)?;
    }
    Ok(content)
}
pub fn source(document: &Document, scope: Scope, cancel: &Cancel) -> Result<String, Failure> {
    match content(document, scope, cancel)? {
        fold_platform::proposals::DocumentInspection::Native(value) => {
            serde_json::to_string_pretty(&value).map_err(|e| Failure::invalid(e.to_string()))
        }
        fold_platform::proposals::DocumentInspection::Source(text) => Ok(text),
    }
}
fn decorate(
    registry: &fold_platform::packages::PackageRegistry,
    document: &Document,
    value: &mut Value,
) -> Result<(), Failure> {
    value["outputs"] = json!(
        registry
            .document_outputs(document)
            .into_iter()
            .map(|o| json!({"reference":o.reference,"label":o.label,"info":o.info}))
            .collect::<Vec<_>>()
    );
    let inputs =
        if registry.has_capability(document, fold_platform::packages::Capability::ImageInputs) {
            registry
                .document_image_inputs(document)
                .map_err(Failure::invalid)?
        } else {
            vec![]
        };
    value["image_inputs"] = json!(
        inputs
            .into_iter()
            .map(|p| json!({"id":p.id,"label":p.label,"source":p.source}))
            .collect::<Vec<_>>()
    );
    Ok(())
}
pub fn inspect(document: &Document) -> Result<Value, Failure> {
    match content(
        document,
        Scope {
            document: document.id,
        },
        &Cancel::default(),
    )? {
        fold_platform::proposals::DocumentInspection::Native(value) => Ok(value),
        fold_platform::proposals::DocumentInspection::Source(text) => Ok(json!({"source":text})),
    }
}
pub fn prepare_scoped(
    base: &CommittedSnapshot,
    scope: Scope,
    body: Value,
    cancel: &Cancel,
) -> Result<Candidate, Failure> {
    let registry = crate::packages::builtins();
    let document = base
        .state()
        .documents
        .get(&scope.document)
        .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
    let prepared = registry.scoped_proposals(document)?.prepare(
        base,
        &ScopedRequest {
            scope,
            version: 1,
            body,
        },
        cancel,
    )?;
    Candidate::from_prepared(base, scope, prepared, cancel)
}
pub fn prepare(
    base: &CommittedSnapshot,
    scope: Scope,
    edits: &[Value],
    cancel: &Cancel,
) -> Result<Candidate, Failure> {
    if edits.is_empty() || edits.len() > 128 {
        return Err(Failure::new(
            "type",
            "Project edits require document scope and 1..128 operations",
        ));
    }
    let registry = crate::packages::builtins();
    let mut after = fold_project::Candidate::new(base.clone());
    for edit in edits {
        check(cancel)?;
        let current = after.snapshot();
        let mutations = if matches!(
            edit["op"].as_str(),
            Some("create" | "add_layer" | "connect_layers")
        ) {
            match serde_json::from_value::<ProjectEdit>(edit.clone())
                .map_err(|e| Failure::invalid(e.to_string()))?
            {
                ProjectEdit::Create {
                    document,
                    type_id,
                    name,
                } => {
                    if current.state().documents.contains_key(&document) {
                        return Err(Failure::new("conflict", "Document already exists"));
                    }
                    if name.trim().is_empty() || name.len() > 256 {
                        return Err(Failure::new("type", "Document name requires 1..256 bytes"));
                    }
                    let created = registry
                        .create_with_defaults(current, &type_id, document)
                        .map_err(Failure::invalid)?;
                    registry.scoped_proposals(&created)?;
                    vec![
                        Mutation::PutDocument(created),
                        Mutation::PutItem(Item {
                            id: ItemId::Document(document),
                            name,
                            parent: Default::default(),
                            order: current.state().organization.items.len() as u64,
                            extensions: Default::default(),
                        }),
                    ]
                }
                ProjectEdit::AddLayer { composition, kind } => {
                    crate::composition_layers::add(
                        current,
                        &serde_json::to_vec(&crate::composition_layers::AddArgs {
                            document: composition,
                            kind,
                            layer: None,
                        })
                        .map_err(|e| Failure::invalid(e.to_string()))?,
                        cancel,
                    )
                    .map_err(Failure::invalid)?
                    .mutations
                }
                ProjectEdit::ConnectLayers {
                    composition,
                    source,
                    target,
                    port,
                } => {
                    crate::composition_layers::connect(
                        current,
                        &serde_json::to_vec(&crate::composition_layers::ConnectArgs {
                            document: composition,
                            source,
                            target,
                            port: Some(port),
                        })
                        .map_err(|e| Failure::invalid(e.to_string()))?,
                        cancel,
                    )
                    .map_err(Failure::invalid)?
                    .mutations
                }
            }
        } else {
            let id: DocumentId = serde_json::from_value(
                edit.get("document")
                    .cloned()
                    .ok_or_else(|| Failure::new("type", "Provider edit requires document"))?,
            )
            .map_err(|e| Failure::invalid(e.to_string()))?;
            let original = current
                .state()
                .documents
                .get(&id)
                .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
            if let Some(expected) = edit.get("expected").filter(|v| v.is_object())
                && inspect(original)? != *expected
            {
                return Err(Failure::new("conflict", "Expected document differs"));
            }
            let prepared = registry.scoped_proposals(original)?.prepare(
                current,
                &ScopedRequest {
                    scope: Scope { document: id },
                    version: 1,
                    body: edit.clone(),
                },
                cancel,
            )?;
            replacements(prepared.as_ref(), current, cancel)?
        };
        after = after
            .apply(EditBatch {
                base: current.revision(),
                mutations,
            })
            .map_err(Failure::invalid)?;
    }
    Candidate::seal(base, scope, after, cancel)
}
