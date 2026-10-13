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
        Edit::Native { request } => native::prepare_scoped(snapshot, guard.scope, request, cancel),
    }
}
