//! Immutable provider proposals. Project owns candidate lifetime and publication.
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_project::{Document, Revision, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub document: DocumentId,
    pub definition: Option<ObjectId>,
    #[serde(default)]
    pub object: Option<ObjectId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    InvalidRequest,
    MissingResource,
    Unsupported,
    Cancelled,
    ResourcePressure,
    Oversized,
    Execution,
    Conflict,
    ProtectedEdit,
    Cycle,
    InvalidSource,
}
impl FailureKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "type",
            Self::MissingResource => "missing_resource",
            Self::Unsupported => "unsupported",
            Self::Cancelled => "cancelled",
            Self::ResourcePressure => "capacity",
            Self::Oversized => "budget",
            Self::Execution => "execution",
            Self::Conflict => "conflict",
            Self::ProtectedEdit => "locked",
            Self::Cycle => "cycle",
            Self::InvalidSource => "invalid_source",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    pub details: Value,
}
impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: Value::Null,
        }
    }
    pub fn context(mut self, provider: &str, scope: Scope) -> Self {
        self.details["provider"] = json!(provider);
        self.details["document"] = json!(scope.document);
        if let Some(id) = scope.object {
            self.details["object"] = json!(id);
        }
        if let Some(id) = scope.definition {
            self.details["definition"] = json!(id);
        }
        self
    }
    pub fn invalid(message: String) -> Self {
        Self::new(FailureKind::InvalidRequest, message)
    }
    pub fn conflict(details: Value) -> Self {
        Self {
            kind: FailureKind::Conflict,
            message: "Expected state differs".into(),
            details,
        }
    }
}
pub fn check(cancel: &Cancel) -> Result<(), Failure> {
    cancel
        .check()
        .map_err(|e| Failure::new(FailureKind::Cancelled, e))
}
pub struct ScopedRequest {
    pub scope: Scope,
    /// Version of the provider request, independent of transport version.
    pub version: u32,
    pub body: Value,
}
pub struct ProposalScope {
    pub target: Scope,
    pub package: String,
    pub type_id: String,
    pub schema: u32,
    pub writes: Vec<DocumentId>,
    pub objects: Vec<ObjectId>,
    pub dependencies: BTreeMap<DocumentId, Option<Document>>,
}
pub const MAX_INSPECTION_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InspectionSelection {
    All,
    Source,
    Diff,
    Diagnostics,
}
pub struct InspectionRequest {
    pub scope: Scope,
    pub selection: InspectionSelection,
    pub max_bytes: usize,
}
impl InspectionRequest {
    pub fn all(scope: Scope) -> Self {
        Self {
            scope,
            selection: InspectionSelection::All,
            max_bytes: MAX_INSPECTION_BYTES,
        }
    }
}
#[derive(Clone)]
pub struct Inspection {
    pub source: String,
    pub native: Option<Value>,
    pub changes: Vec<Value>,
    pub diagnostics: Vec<Value>,
    /// Exact content, rather than a lossy digest, binds publication to inspection.
    pub identity: Vec<u8>,
}
pub struct ValidatedChanges {
    pub base: Revision,
    pub documents: Vec<Document>,
    pub identity: Vec<u8>,
}
pub enum DocumentInspection {
    Source(String),
    Native(Value),
}
pub trait ScopedProposals: Send + Sync {
    fn schema(&self) -> Value;
    fn inspect(
        &self,
        document: &Document,
        scope: Scope,
        cancel: &Cancel,
    ) -> Result<DocumentInspection, Failure>;
    fn prepare(
        &self,
        base: &Snapshot,
        request: &ScopedRequest,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure>;
    /// Seal cross-document orchestration using exactly the same provider gate.
    fn prepare_change(
        &self,
        base: &Snapshot,
        scope: Scope,
        after: &Document,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure>;
}
pub trait PreparedProposal: Send + Sync {
    fn undo_label(&self) -> &'static str;
    fn scope(&self) -> &ProposalScope;
    fn inspect(&self, request: &InspectionRequest, cancel: &Cancel) -> Result<Inspection, Failure>;
    fn reconcile(
        &self,
        current: &Snapshot,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure>;
    fn publication(&self, current: &Snapshot, cancel: &Cancel)
    -> Result<ValidatedChanges, Failure>;
}
/// Providers implement these operations over their own decoded records. This
/// shared value handles only envelope identity, dependency guards and revision.
pub trait Records: Send + Sync + Sized + 'static {
    fn undo_label(&self) -> &'static str;
    fn decode(document: &Document) -> Result<Self, Failure>;
    fn validate_change(before: &Self, after: &Self, scope: Scope) -> Result<(), Failure>;
    fn reconcile(
        before: &Self,
        after: &Self,
        current: &Self,
        scope: Scope,
    ) -> Result<Self, Failure>;
    fn document(&self, original: &Document) -> Result<Document, Failure>;
    fn source(&self, scope: Scope) -> Result<DocumentInspection, Failure>;
    fn diff(before: Option<&Self>, after: &Self, scope: Scope) -> Result<Vec<Value>, Failure>;
    fn affected(
        before: Option<&Self>,
        after: &Self,
        scope: Scope,
    ) -> Result<Vec<ObjectId>, Failure>;
}
pub struct Prepared<R: Records> {
    scope: ProposalScope,
    revision: Revision,
    before: Option<Document>,
    original: Option<R>,
    after: Document,
    records: R,
    inspection: Inspection,
}
impl<R: Records> Prepared<R> {
    pub fn new(
        base: &Snapshot,
        scope: Scope,
        after: &Document,
        cancel: &Cancel,
    ) -> Result<Self, Failure> {
        check(cancel)?;
        if after.id != scope.document {
            return Err(Failure::new(
                FailureKind::Conflict,
                "Document scope differs",
            ));
        }
        let before = base
            .state()
            .documents
            .get(&scope.document)
            .map(|d| d.as_ref().clone());
        if let Some(before) = &before
            && (before.package_id != after.package_id
                || before.type_id != after.type_id
                || before.schema_version != after.schema_version)
        {
            return Err(Failure::new(
                FailureKind::Conflict,
                "Document identity changed",
            ));
        }
        let original = before.as_ref().map(R::decode).transpose()?;
        let records = R::decode(after)?;
        if let Some(before) = &original {
            R::validate_change(before, &records, scope)?;
        }
        let dependencies = after
            .dependencies
            .iter()
            .chain(before.iter().flat_map(|d| d.dependencies.iter()))
            .filter(|d| d.document != after.id)
            .map(|d| {
                (
                    d.document,
                    base.state()
                        .documents
                        .get(&d.document)
                        .map(|v| v.as_ref().clone()),
                )
            })
            .collect();
        let objects = R::affected(original.as_ref(), &records, scope)?;
        let (source, native) = match records.source(scope)? {
            DocumentInspection::Source(source) => (source, None),
            DocumentInspection::Native(native) => (
                serde_json::to_string_pretty(&native)
                    .map_err(|e| Failure::invalid(e.to_string()))?,
                Some(native),
            ),
        };
        let mut inspection = Inspection {
            source,
            native,
            changes: R::diff(original.as_ref(), &records, scope)?,
            diagnostics: vec![],
            identity: vec![],
        };
        inspection.identity = inspection_identity(after, &inspection)?;
        if inspection.identity.len() > MAX_INSPECTION_BYTES {
            return Err(Failure::new(
                FailureKind::Oversized,
                "Prepared inspection exceeds 16 MiB",
            ));
        }
        check(cancel)?;
        Ok(Self {
            scope: ProposalScope {
                target: scope,
                package: after.package_id.clone(),
                type_id: after.type_id.clone(),
                schema: after.schema_version,
                writes: vec![after.id],
                objects,
                dependencies,
            },
            revision: base.revision(),
            before,
            original,
            after: after.clone(),
            records,
            inspection,
        })
    }
    fn dependencies(&self, current: &Snapshot) -> Result<(), Failure> {
        for (id, expected) in &self.scope.dependencies {
            if current.state().documents.get(id).map(|d| d.as_ref()) != expected.as_ref() {
                return Err(Failure::new(
                    FailureKind::Conflict,
                    "Proposal dependency changed",
                ));
            }
        }
        Ok(())
    }
}
impl<R: Records> PreparedProposal for Prepared<R> {
    fn undo_label(&self) -> &'static str {
        self.records.undo_label()
    }
    fn scope(&self) -> &ProposalScope {
        &self.scope
    }
    fn inspect(&self, request: &InspectionRequest, cancel: &Cancel) -> Result<Inspection, Failure> {
        check(cancel)?;
        if request.scope != self.scope.target {
            return Err(Failure::new(
                FailureKind::Conflict,
                "Inspection scope differs",
            ));
        }
        if request.max_bytes == 0 || request.max_bytes > MAX_INSPECTION_BYTES {
            return Err(Failure::new(
                FailureKind::Oversized,
                "Invalid inspection budget",
            ));
        }
        let mut result = self.inspection.clone();
        if request.selection != InspectionSelection::All {
            if request.selection != InspectionSelection::Source {
                result.source.clear();
                result.native = None;
            }
            if request.selection != InspectionSelection::Diff {
                result.changes.clear();
            }
            if request.selection != InspectionSelection::Diagnostics {
                result.diagnostics.clear();
            }
            result.identity = inspection_identity(&self.after, &result)?;
        }
        if result.identity.len() > request.max_bytes {
            return Err(Failure::new(
                FailureKind::Oversized,
                "Inspection exceeds requested budget",
            ));
        }
        check(cancel)?;
        Ok(result)
    }

    fn reconcile(
        &self,
        current: &Snapshot,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        check(cancel)?;
        self.dependencies(current)?;
        let now = current
            .state()
            .documents
            .get(&self.after.id)
            .map(|d| d.as_ref());
        let after = if now == self.before.as_ref() {
            self.after.clone()
        } else {
            let original = self.original.as_ref().ok_or_else(|| {
                Failure::new(
                    FailureKind::Conflict,
                    "Created document identity already exists",
                )
            })?;
            let now = now.ok_or_else(|| {
                Failure::new(FailureKind::Conflict, "Edited document was removed")
            })?;
            let records = R::decode(now)?;
            let merged = R::reconcile(original, &self.records, &records, self.scope.target)?;
            R::validate_change(&records, &merged, self.scope.target).map_err(|mut e| {
                if e.kind == FailureKind::ProtectedEdit {
                    e.details["cause"] = json!(e.kind.code());
                    e.kind = FailureKind::Conflict;
                }
                e
            })?;
            let mut document = merged.document(now)?;
            document.extensions = fold_graph::reconcile::map(
                &self.before.as_ref().unwrap().extensions,
                &self.after.extensions,
                &now.extensions,
            )
            .map_err(|e| Failure::new(FailureKind::Conflict, e))?;
            document
        };
        Ok(Box::new(Self::new(
            current,
            self.scope.target,
            &after,
            cancel,
        )?))
    }
    fn publication(
        &self,
        current: &Snapshot,
        cancel: &Cancel,
    ) -> Result<ValidatedChanges, Failure> {
        check(cancel)?;
        if current.revision() != self.revision
            || current
                .state()
                .documents
                .get(&self.after.id)
                .map(|d| d.as_ref())
                != self.before.as_ref()
        {
            return Err(Failure::new(FailureKind::Conflict, "Proposal base changed"));
        }
        self.dependencies(current)?;
        if let Some(before) = &self.original {
            R::validate_change(before, &self.records, self.scope.target)?;
        }
        Ok(ValidatedChanges {
            base: self.revision,
            documents: vec![self.after.clone()],
            identity: self.inspection.identity.clone(),
        })
    }
}
fn inspection_identity(document: &Document, inspection: &Inspection) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec(&(
        document,
        &inspection.source,
        &inspection.native,
        &inspection.changes,
        &inspection.diagnostics,
    ))
    .map_err(|e| Failure::invalid(e.to_string()))
}
pub fn request(request: &ScopedRequest, cancel: &Cancel) -> Result<(), Failure> {
    check(cancel)?;
    if request.version != 1 {
        return Err(Failure::new(
            FailureKind::Unsupported,
            "Provider proposal version 1 required",
        ));
    }
    if serde_json::to_vec(&request.body)
        .map_err(|e| Failure::invalid(e.to_string()))?
        .len()
        > 1024 * 1024
    {
        return Err(Failure::new(
            FailureKind::Oversized,
            "Provider request exceeds 1 MiB",
        ));
    }
    Ok(())
}
pub fn document_scope(scope: Scope) -> Result<(), Failure> {
    if scope.definition.is_some() || scope.object.is_some() {
        return Err(Failure::new(
            FailureKind::InvalidRequest,
            "This operation requires document scope",
        ));
    }
    Ok(())
}
pub fn json_diff<R: Serialize>(
    before: Option<&R>,
    after: &R,
    scope: Scope,
) -> Result<Vec<Value>, Failure> {
    Ok(vec![
        json!({"document":scope.document,"before":before,"after":after}),
    ])
}

impl From<fold_graph::validation::Diagnostic> for Failure {
    fn from(diagnostic: fold_graph::validation::Diagnostic) -> Self {
        use fold_graph::validation::Kind;
        Self {
            kind: match diagnostic.kind {
                Kind::Type => FailureKind::InvalidRequest,
                Kind::Cycle => FailureKind::Cycle,
                Kind::MissingReference => FailureKind::MissingResource,
                Kind::Unsupported => FailureKind::Unsupported,
                Kind::Budget => FailureKind::Oversized,
            },
            message: diagnostic.message,
            details: json!({"native_ids": diagnostic.native_ids}),
        }
    }
}
