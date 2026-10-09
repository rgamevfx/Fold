//! Bounded preparation on retained snapshots. No worker can commit a project.
use super::{Failure, operation_budget, projection};
use crate::agent::wire::{Edit, Guard};
use fold_media::Cancel;
use fold_motion::{editing, model::Construction};
use fold_project::CommittedSnapshot;
use serde_json::json;
pub(super) fn run(
    snapshot: &CommittedSnapshot,
    guard: Guard,
    edit: Edit,
    cancel: &Cancel,
) -> Result<super::native::Candidate, Failure> {
    if guard.scope.object.is_some() {
        return object(snapshot, guard, edit, cancel);
    }
    if let Edit::Project { edits } = &edit {
        return super::native::prepare(snapshot, guard.scope, edits, cancel);
    }
    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
    let original = snapshot
        .state()
        .documents
        .get(&guard.scope.document)
        .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
    let before = Construction::from_document(original).map_err(Failure::invalid)?;
    operation_budget(&before)?;
    let candidate = match edit {
        Edit::Project { .. } => unreachable!("project proposal handled above"),
        Edit::Patches { patches } => editing::prepare_patches(
            snapshot,
            guard.scope.document,
            guard.scope.native(),
            &patches,
        )?,
        Edit::Source {
            expected,
            text,
            mapping,
        } => {
            if expected.len() > 65536 || text.len() > 65536 {
                return Err(Failure::new("budget", "Source exceeds 64 KiB"));
            }
            let actual = projection(&before, guard.scope)?;
            if expected != actual {
                return Err(Failure::conflict(
                    json!({"expected":expected,"actual":actual}),
                ));
            }
            let mut buffer = editing::SourceBuffer::default();
            let proposed = if let Some(target) = guard.scope.definition {
                buffer.propose_fragment(text, snapshot, guard.scope.document, target, &mapping)
            } else {
                if !mapping.is_empty() {
                    return Err(Failure::new(
                        "unsupported",
                        "Identity mapping requires an asset scope",
                    ));
                }
                buffer.propose(text, snapshot, guard.scope.document, guard.scope.native())
            };
            match proposed {
                Ok(candidate) => candidate,
                Err(error) if !buffer.diagnostics().is_empty() => {
                    let mut failure = match error {
                        editing::Error::Validation(diagnostic) => Failure::from(diagnostic),
                        error => Failure::new("invalid_source", format!("{error:?}")),
                    };
                    failure.details["diagnostics"] = json!(buffer.diagnostics().iter().map(|d| json!({"message":d.message,"start":d.span.start,"end":d.span.end,"native_id":d.native_id})).collect::<Vec<_>>());
                    return Err(failure);
                }
                Err(error) => return Err(error.into()),
            }
        }
    };
    operation_budget(
        &Construction::from_document(candidate.document()).map_err(Failure::invalid)?,
    )?;
    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
    Ok(super::native::Candidate::Construction(Box::new(candidate)))
}

/// Reuse the native graph edit gate on an isolated object graph, then publish
/// through the scene owner so hierarchy and inherited protection remain intact.
fn object(
    snapshot: &CommittedSnapshot,
    guard: Guard,
    edit: Edit,
    cancel: &Cancel,
) -> Result<super::native::Candidate, Failure> {
    let scope = guard.scope;
    let document = snapshot
        .state()
        .documents
        .get(&scope.document)
        .ok_or_else(|| Failure::new("missing_resource", "Missing scene"))?;
    let scene = fold_motion::scene::Scene::from_document(document).map_err(Failure::invalid)?;
    let object = scene
        .object(scope.object.unwrap())
        .map_err(Failure::invalid)?;
    if matches!(edit, Edit::Project { .. }) {
        return Err(Failure::new("type", "Object scope requires a graph edit"));
    }
    let expected = fold_motion::source::canonical(&object.graph).map_err(Failure::invalid)?;
    let graph = object
        .graph
        .document(scope.document)
        .map_err(Failure::invalid)?;
    let mut isolated = fold_project::Project::new(0);
    isolated
        .commit(fold_project::EditBatch {
            base: isolated.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(graph)],
        })
        .map_err(|e| Failure::invalid(e.to_string()))?;
    let mut graph_guard = guard;
    graph_guard.scope.object = None;
    let prepared = run(&isolated.snapshot(), graph_guard, edit, cancel)?;
    use fold_project::proposals::Proposal;
    let graph = Construction::from_document(prepared.document()).map_err(Failure::invalid)?;
    let source = fold_motion::source::canonical(&graph).map_err(Failure::invalid)?;
    let mut root = scope;
    root.object = None;
    root.definition = None;
    let mut candidate = super::native::prepare(
        snapshot,
        root,
        &[super::native::Edit::ObjectSource {
            document: scope.document,
            object: scope.object.unwrap(),
            expected,
            source,
        }],
        cancel,
    )?;
    if let super::native::Candidate::Documents(value) = &mut candidate {
        value.scope = scope;
    }
    Ok(candidate)
}
