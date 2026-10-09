//! App request routing; creative payloads are interpreted only by providers.
use super::{Failure, native};
use crate::agent::wire::{Edit, Guard};
use fold_media::Cancel;
use fold_project::CommittedSnapshot;
pub(super) fn run(
    snapshot: &CommittedSnapshot,
    guard: Guard,
    edit: Edit,
    cancel: &Cancel,
) -> Result<native::Candidate, Failure> {
    match edit {
        Edit::Project { edits } => native::prepare(snapshot, guard.scope, &edits, cancel),
        edit => native::prepare_scoped(
            snapshot,
            guard.scope,
            serde_json::to_value(edit).map_err(|e| Failure::invalid(e.to_string()))?,
            cancel,
        ),
    }
}
