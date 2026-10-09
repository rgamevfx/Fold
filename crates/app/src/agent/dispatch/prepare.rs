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
) -> Result<editing::Candidate, Failure> {
    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
    let original = snapshot
        .state()
        .documents
        .get(&guard.scope.document)
        .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
    let before = Construction::from_document(original).map_err(Failure::invalid)?;
    operation_budget(&before)?;
    let candidate = match edit {
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
    Ok(candidate)
}
