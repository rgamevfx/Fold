//! Scoped Compositor proposals retain native records and use C3 preparation.
use crate::Composite;
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::proposals::*;
use fold_project::{Document, Snapshot};
use serde::Deserialize;
use serde_json::{Value, json};
pub struct Provider;
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Author {
        document: DocumentId,
        expected: Value,
        edits: Vec<Value>,
    },
    CompositorSource {
        document: DocumentId,
        nodes: Option<Vec<ObjectId>>,
        expected: String,
        source: String,
    },
}
impl ScopedProposals for Provider {
    fn schema(&self) -> Value {
        json!({"version":1,"edits":schemars::schema_for!(Edit),"limits":{"source_bytes":1048576,"request_bytes":1048576}})
    }
    fn inspect(
        &self,
        d: &Document,
        scope: Scope,
        cancel: &Cancel,
    ) -> Result<DocumentInspection, Failure> {
        check(cancel)?;
        Graph::decode(d)?.source(scope)
    }
    fn prepare(
        &self,
        base: &Snapshot,
        r: &ScopedRequest,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        (|| -> Result<Box<dyn PreparedProposal>, Failure> {
            request(r, cancel)?;
            let original = base
                .state()
                .documents
                .get(&r.scope.document)
                .ok_or_else(|| {
                    Failure::new(FailureKind::MissingResource, "Missing Compositor document")
                })?;
            let edit: Edit = serde_json::from_value(r.body.clone())
                .map_err(|e| Failure::invalid(e.to_string()))?;
            let after = match edit {
                Edit::Author {
                    document,
                    expected,
                    edits,
                } => {
                    if document != r.scope.document
                        || expected["native"]
                            != crate::automation::inspect(original).map_err(Failure::invalid)?
                    {
                        return Err(Failure::new(
                            FailureKind::Conflict,
                            "Expected document differs",
                        ));
                    }
                    crate::automation::author(original, &edits).map_err(Failure::invalid)?
                }
                Edit::CompositorSource {
                    document,
                    nodes,
                    expected,
                    source,
                } => {
                    if document != r.scope.document {
                        return Err(Failure::new(
                            FailureKind::Conflict,
                            "Document scope differs",
                        ));
                    }
                    let mut graph = Composite::from_document(original).map_err(Failure::invalid)?;
                    if graph.source(nodes.as_deref()).map_err(Failure::invalid)? != expected {
                        return Err(Failure::new(
                            FailureKind::Conflict,
                            "Compositor source changed",
                        ));
                    }
                    graph
                        .replace_source(nodes.as_deref(), &expected, &source)
                        .map_err(Failure::invalid)?;
                    Graph(graph).document(original)?
                }
            };
            self.prepare_change(base, r.scope, &after, cancel)
        })()
        .map_err(|error: Failure| error.context(crate::PACKAGE, r.scope))
    }
    fn prepare_change(
        &self,
        base: &Snapshot,
        scope: Scope,
        after: &Document,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        (|| -> Result<Box<dyn PreparedProposal>, Failure> {
            Ok(Box::new(Prepared::<Graph>::new(
                base, scope, after, cancel,
            )?))
        })()
        .map_err(|error: Failure| error.context(crate::PACKAGE, scope))
    }
}
struct Graph(Composite);
impl Records for Graph {
    fn undo_label(&self) -> &'static str {
        "Edit Composition and scene graphs"
    }
    fn decode(document: &Document) -> Result<Self, Failure> {
        let value = Composite::from_document(document).map_err(Failure::invalid)?;
        Ok(Self(value))
    }
    fn validate_change(before: &Self, after: &Self, _scope: Scope) -> Result<(), Failure> {
        before
            .0
            .check_protection(&after.0)
            .map_err(|e| Failure::new(FailureKind::ProtectedEdit, e))
    }
    fn reconcile(
        before: &Self,
        after: &Self,
        current: &Self,
        _scope: Scope,
    ) -> Result<Self, Failure> {
        Composite::rebase(&before.0, &after.0, &current.0)
            .map(Self)
            .map_err(|e| Failure::new(FailureKind::Conflict, e))
    }
    fn document(&self, original: &Document) -> Result<Document, Failure> {
        let mut d = self.0.document(original.id).map_err(Failure::invalid)?;
        d.extensions = original.extensions.clone();
        Ok(d)
    }
    fn source(&self, _scope: Scope) -> Result<DocumentInspection, Failure> {
        Ok(DocumentInspection::Native(
            json!({"native":self.0,"graph_source":self.0.source(None).map_err(Failure::invalid)?}),
        ))
    }
    fn diff(before: Option<&Self>, after: &Self, scope: Scope) -> Result<Vec<Value>, Failure> {
        let ids = Self::affected(before, after, scope)?;
        let mut changes=ids.into_iter().map(|id|json!({"document":scope.document,"native_id":id,"before":before.and_then(|v|v.0.nodes.iter().find(|n|n.id==id)),"after":after.0.nodes.iter().find(|n|n.id==id)})).collect::<Vec<_>>();
        // Native node changes are reported once; keep document settings and
        // unknown extensions separate from their identity-bearing children.
        let metadata = |graph: &Self| -> Result<Value, Failure> {
            let mut value =
                serde_json::to_value(&graph.0).map_err(|e| Failure::invalid(e.to_string()))?;
            value
                .as_object_mut()
                .expect("Composite serializes as a record")
                .remove("nodes");
            Ok(value)
        };
        let old = before.map(metadata).transpose()?;
        let new = metadata(after)?;
        if old.as_ref() != Some(&new) {
            changes.push(json!({"document":scope.document,"before":old,"after":new}));
        }
        Ok(changes)
    }
    fn affected(
        before: Option<&Self>,
        after: &Self,
        _scope: Scope,
    ) -> Result<Vec<ObjectId>, Failure> {
        let ids = before
            .into_iter()
            .flat_map(|v| v.0.nodes.iter())
            .chain(after.0.nodes.iter())
            .map(|n| n.id)
            .collect::<std::collections::BTreeSet<_>>();
        Ok(ids
            .into_iter()
            .filter(|id| {
                before.and_then(|v| v.0.nodes.iter().find(|n| n.id == *id))
                    != after.0.nodes.iter().find(|n| n.id == *id)
            })
            .collect())
    }
}
