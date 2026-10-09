//! C4 capability boundary: immutable providers cannot publish or mint receipts.
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::proposals::{FailureKind, InspectionRequest, Scope, ScopedRequest};
use fold_project::{EditBatch, Mutation, Project};
use serde_json::json;

#[test]
fn scoped_preparation_guards_identity_revision_cancellation_and_inspection_scope() {
    let registry = fold_app::packages::builtins();
    let id = DocumentId::new();
    let mut project = Project::new(8);
    let document = fold_motion::package::create(id).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let base = project.snapshot();
    let document = &base.state().documents[&id];
    let capability = registry.scoped_proposals(document).unwrap();
    let scope = Scope {
        document: id,
        definition: None,
        object: None,
    };
    let request = ScopedRequest {
        scope,
        version: 99,
        body: json!({}),
    };
    assert_eq!(
        capability
            .prepare(&base, &request, &Cancel::default())
            .err()
            .unwrap()
            .kind,
        FailureKind::Unsupported
    );
    let mut records = fold_motion::model::Construction::from_document(document).unwrap();
    records.seed += 1;
    let after = records.replace_document(document).unwrap();
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert_eq!(
        capability
            .prepare_change(&base, scope, &after, &cancelled)
            .err()
            .unwrap()
            .kind,
        FailureKind::Cancelled
    );
    let prepared = capability
        .prepare_change(&base, scope, &after, &Cancel::default())
        .unwrap();
    assert_eq!(project.snapshot().state(), base.state());
    assert_eq!(prepared.scope().writes, vec![id]);
    let inspection = prepared
        .inspect(&InspectionRequest::all(scope), &Cancel::default())
        .unwrap();
    assert_eq!(
        inspection.identity,
        prepared
            .publication(&base, &Cancel::default())
            .unwrap()
            .identity
    );
    assert_eq!(
        prepared
            .inspect(
                &InspectionRequest::all(Scope {
                    definition: Some(ObjectId::new()),
                    ..scope
                }),
                &Cancel::default()
            )
            .err()
            .unwrap()
            .kind,
        FailureKind::Conflict
    );
    let mut bounded = InspectionRequest::all(scope);
    bounded.max_bytes = 1;
    assert_eq!(
        prepared
            .inspect(&bounded, &Cancel::default())
            .err()
            .unwrap()
            .kind,
        FailureKind::Oversized
    );
    bounded = InspectionRequest::all(scope);
    bounded.selection = fold_platform::proposals::InspectionSelection::Diff;
    let diff = prepared.inspect(&bounded, &Cancel::default()).unwrap();
    assert!(diff.source.is_empty());
    assert!(!diff.changes.is_empty());
    assert_ne!(diff.identity, inspection.identity);
    let other = DocumentId::new();
    project
        .commit(EditBatch {
            base: base.revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::package::create(other).unwrap(),
            )],
        })
        .unwrap();
    let current = project.snapshot();
    assert_eq!(
        prepared
            .publication(&current, &Cancel::default())
            .err()
            .unwrap()
            .kind,
        FailureKind::Conflict
    );
    let rebased = prepared.reconcile(&current, &Cancel::default()).unwrap();
    assert_eq!(
        rebased
            .publication(&current, &Cancel::default())
            .unwrap()
            .base,
        current.revision()
    );
    assert_eq!(
        prepared
            .inspect(&InspectionRequest::all(scope), &Cancel::default())
            .unwrap()
            .identity,
        inspection.identity
    );
    let mut unsupported = document.as_ref().clone();
    unsupported.schema_version += 1;
    assert!(registry.scoped_proposals(&unsupported).is_err());
}
