//! Common-layer proposal policy; application graph assembly remains in App.
use crate::Composition;
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
}
impl ScopedProposals for Provider {
    fn schema(&self) -> Value {
        json!({"version":1,"edits":schemars::schema_for!(Edit)})
    }
    fn inspect(
        &self,
        d: &Document,
        scope: Scope,
        cancel: &Cancel,
    ) -> Result<DocumentInspection, Failure> {
        check(cancel)?;
        Layers::decode(d)?.source(scope)
    }
    fn prepare(
        &self,
        base: &Snapshot,
        r: &ScopedRequest,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        (|| -> Result<Box<dyn PreparedProposal>, Failure> {
            request(r, cancel)?;
            document_scope(r.scope)?;
            let original = base
                .state()
                .documents
                .get(&r.scope.document)
                .ok_or_else(|| Failure::new(FailureKind::MissingResource, "Missing Composition"))?;
            let Edit::Author {
                document,
                expected,
                edits,
            } = serde_json::from_value(r.body.clone())
                .map_err(|e| Failure::invalid(e.to_string()))?;
            let before = Composition::from_document(original).map_err(Failure::invalid)?;
            if document != r.scope.document || expected["native"] != json!(before) {
                return Err(Failure::new(
                    FailureKind::Conflict,
                    "Expected document differs",
                ));
            }
            if edits
                .iter()
                .any(|e| e["op"] == "lock" && e["locked"] == false)
            {
                return Err(Failure::new(
                    FailureKind::ProtectedEdit,
                    "Agent edits cannot unlock controls",
                ));
            }
            let after = crate::automation::author(original, &edits).map_err(Failure::invalid)?;
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
            document_scope(scope)?;
            Ok(Box::new(Prepared::<Layers>::new(
                base, scope, after, cancel,
            )?))
        })()
        .map_err(|error: Failure| error.context(crate::PACKAGE, scope))
    }
}
struct Layers(Composition);
impl Records for Layers {
    fn undo_label(&self) -> &'static str {
        "Edit Composition and scene graphs"
    }
    fn decode(d: &Document) -> Result<Self, Failure> {
        Composition::from_document(d)
            .map(Self)
            .map_err(Failure::invalid)
    }
    fn validate_change(before: &Self, after: &Self, scope: Scope) -> Result<(), Failure> {
        document_scope(scope)?;
        for layer in &before.0.layers {
            if before.0.locked(layer.id)
                && after.0.layers.iter().find(|v| v.id == layer.id) != Some(layer)
            {
                return Err(Failure::new(
                    FailureKind::ProtectedEdit,
                    "Composition layer is locked",
                ));
            }
        }
        Ok(())
    }
    fn reconcile(
        _before: &Self,
        _after: &Self,
        _current: &Self,
        _scope: Scope,
    ) -> Result<Self, Failure> {
        Err(Failure::new(
            FailureKind::Conflict,
            "Edited Composition changed",
        ))
    }
    fn document(&self, original: &Document) -> Result<Document, Failure> {
        let mut d = self.0.document(original.id).map_err(Failure::invalid)?;
        d.extensions = original.extensions.clone();
        Ok(d)
    }
    fn source(&self, scope: Scope) -> Result<DocumentInspection, Failure> {
        document_scope(scope)?;
        Ok(DocumentInspection::Native(json!({"native":self.0})))
    }
    fn diff(before: Option<&Self>, after: &Self, scope: Scope) -> Result<Vec<Value>, Failure> {
        json_diff(before.map(|v| &v.0), &after.0, scope)
    }
    fn affected(
        before: Option<&Self>,
        after: &Self,
        _scope: Scope,
    ) -> Result<Vec<ObjectId>, Failure> {
        let ids = before
            .into_iter()
            .flat_map(|v| v.0.layers.iter())
            .chain(after.0.layers.iter())
            .map(|n| n.id)
            .collect::<std::collections::BTreeSet<_>>();
        Ok(ids
            .into_iter()
            .filter(|id| {
                before.and_then(|v| v.0.layers.iter().find(|n| n.id == *id))
                    != after.0.layers.iter().find(|n| n.id == *id)
            })
            .collect())
    }
}
