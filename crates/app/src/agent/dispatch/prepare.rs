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
        Edit::Gallery { request } => {
            if request.document != guard.scope.document {
                return Err(Failure::invalid(
                    "Gallery target differs from proposal scope".into(),
                ));
            }
            let batch = crate::gallery::place(
                &crate::gallery::storage::Store::user(),
                snapshot,
                &request,
                cancel,
            )
            .map_err(Failure::invalid)?;
            native::Candidate::from_batch(snapshot, guard.scope, batch, cancel)
        }
        Edit::Project { edits } => native::prepare(snapshot, guard.scope, &edits, cancel),
        Edit::Native { request } => native::prepare_scoped(snapshot, guard.scope, request, cancel),
    }
}
