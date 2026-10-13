//! Native candidates share typed authoring guards and the host publication lifecycle.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_motion_canvas::model::Canvas;
use fold_platform::{packages::Authoring, proposals::*};
use fold_project::{Document, Snapshot};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

pub(crate) static CAPABILITY: AuthoringProposals<Record> = AuthoringProposals::new(author, catalog);
fn author(document: &Document, edits: &[Value], cancel: &Cancel) -> Result<Document, String> {
    crate::package::Provider::default().author_cancellable(document, edits, cancel)
}
fn catalog() -> Value {
    crate::package::Provider::default().authoring_catalog()
}
#[derive(Default)]
pub(crate) struct Proposals {
    pub conversion: Option<Arc<crate::conversion::Service>>,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Author {
        document: DocumentId,
        expected: Value,
        edits: Vec<Value>,
    },
    Capture {
        document: DocumentId,
        expected: Value,
        object: ObjectId,
        time: Time,
    },
}
impl ScopedProposals for Proposals {
    fn schema(&self) -> Value {
        if self.conversion.is_none() {
            return CAPABILITY.schema();
        }
        json!({"version":1,"request":schemars::schema_for!(Request),"authoring":catalog(),"capture":"Convert local script output to editable objects at exact time. Inspect the candidate before Apply."})
    }
    fn inspect(
        &self,
        document: &Document,
        scope: Scope,
        cancel: &Cancel,
    ) -> Result<DocumentInspection, Failure> {
        CAPABILITY.inspect(document, scope, cancel)
    }
    fn prepare(
        &self,
        base: &Snapshot,
        input: &ScopedRequest,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        request(input, cancel)?;
        let parsed: Request = serde_json::from_value(input.body.clone())
            .map_err(|error| Failure::invalid(error.to_string()))?;
        match parsed {
            Request::Author {
                document,
                expected,
                edits,
            } => {
                let native = ScopedRequest {
                    scope: input.scope,
                    version: input.version,
                    body: json!({"op":"author","document":document,"expected":expected,"edits":edits}),
                };
                CAPABILITY.prepare(base, &native, cancel)
            }
            Request::Capture {
                document,
                expected,
                object,
                time,
            } => {
                let service = self.conversion.as_ref().ok_or_else(|| {
                    Failure::new(FailureKind::Unsupported, "Script capture is not configured")
                })?;
                let original = base
                    .state()
                    .documents
                    .get(&input.scope.document)
                    .ok_or_else(|| {
                        Failure::new(FailureKind::MissingResource, "Missing Motion document")
                    })?;
                let DocumentInspection::Native(native) =
                    Record::decode(original)?.source(input.scope)?
                else {
                    return Err(Failure::new(
                        FailureKind::Unsupported,
                        "Native inspection required",
                    ));
                };
                if document != input.scope.document || expected["native"] != native["native"] {
                    return Err(Failure::new(
                        FailureKind::Conflict,
                        "Expected native document differs",
                    ));
                }
                let after = service
                    .prepare(original, object, time, base, cancel)
                    .map_err(|error| {
                        use fold_foundation::FailureKind as Kind;
                        let kind = match error.kind {
                            Kind::Cancelled => FailureKind::Cancelled,
                            Kind::ResourcePressure => FailureKind::ResourcePressure,
                            Kind::RequestTooLarge => FailureKind::Oversized,
                            Kind::InvalidInput => FailureKind::InvalidRequest,
                            Kind::MissingResource => FailureKind::MissingResource,
                            Kind::UnsupportedCapability => FailureKind::Unsupported,
                            Kind::Execution => FailureKind::Execution,
                        };
                        Failure::new(kind, error.message)
                    })?;
                self.prepare_change(base, input.scope, &after, cancel)
            }
        }
    }
    fn prepare_change(
        &self,
        base: &Snapshot,
        scope: Scope,
        after: &Document,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        CAPABILITY.prepare_change(base, scope, after, cancel)
    }
}
pub(crate) struct Record(Canvas);
impl Records for Record {
    fn undo_label(&self) -> &'static str {
        "Edit Motion"
    }
    fn decode(document: &Document) -> Result<Self, Failure> {
        Canvas::from_document(document)
            .map(Self)
            .map_err(Failure::invalid)
    }
    fn validate_change(before: &Self, after: &Self, _scope: Scope) -> Result<(), Failure> {
        fold_motion_canvas::editing::check_change(&before.0, &after.0)
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
        self.0.replace_document(original).map_err(Failure::invalid)
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
