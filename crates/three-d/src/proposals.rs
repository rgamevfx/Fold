//! Native candidates share typed authoring guards and the host publication lifecycle.
use crate::model::Scene;
use fold_foundation::ObjectId;
use fold_media::Cancel;
use fold_platform::{packages::Authoring, proposals::*};
use fold_project::Document;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) static CAPABILITY: AuthoringProposals<Record> = AuthoringProposals::new(author, catalog);
fn author(document: &Document, edits: &[Value], cancel: &Cancel) -> Result<Document, String> {
    crate::package::Provider.author_cancellable(document, edits, cancel)
}
fn catalog() -> Value {
    crate::package::Provider.authoring_catalog()
}
pub(crate) struct Record(Scene);
impl Records for Record {
    fn undo_label(&self) -> &'static str {
        "Edit 3D"
    }
    fn decode(document: &Document) -> Result<Self, Failure> {
        Scene::from_document(document)
            .map(Self)
            .map_err(Failure::invalid)
    }
    fn validate_change(before: &Self, after: &Self, _scope: Scope) -> Result<(), Failure> {
        crate::editing::check_change(&before.0, &after.0)
            .map_err(|e| Failure::new(FailureKind::ProtectedEdit, e))
    }
    fn reconcile(
        before: &Self,
        after: &Self,
        current: &Self,
        _scope: Scope,
    ) -> Result<Self, Failure> {
        if current.0 == before.0 || current.0 == after.0 {
            Ok(Self(after.0.clone()))
        } else {
            Err(Failure::new(
                FailureKind::Conflict,
                "Document changed; inspect and prepare again",
            ))
        }
    }
    fn document(&self, original: &Document) -> Result<Document, Failure> {
        let mut document = self.0.document(original.id).map_err(Failure::invalid)?;
        document.extensions = original.extensions.clone();
        document.revision = original.revision;
        Ok(document)
    }
    fn source(&self, _scope: Scope) -> Result<DocumentInspection, Failure> {
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
        let ids: BTreeSet<_> = before
            .into_iter()
            .flat_map(|v| v.0.objects.keys())
            .chain(after.0.objects.keys())
            .copied()
            .collect();
        Ok(ids
            .into_iter()
            .filter(|id| before.and_then(|v| v.0.objects.get(id)) != after.0.objects.get(id))
            .collect())
    }
}
