use fold_app::agent::jobs::*;
use fold_foundation::ObjectId;
use fold_project::Revision;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn response(bytes: Vec<u8>, committed: Option<Revision>) -> Completion {
    Completion::Response {
        bytes: bytes.into(),
        committed,
    }
}

#[test]
fn cancellation_is_cooperative_and_cannot_relabel_a_commit() {
    let now = Instant::now();
    let mut jobs = Jobs::new(ObjectId::new());
    let (handle, ticket) = jobs.start(Duration::from_secs(30), now).unwrap();
    let cancel = ticket.cancellation();
    assert!(cancel.check().is_ok());
    assert_eq!(jobs.cancel(handle, now), Ok(Cancellation::Requested));
    assert!(cancel.check().is_err());
    assert_eq!(
        jobs.status(handle, now),
        Ok(Status::Running {
            cancellation_requested: true
        })
    );
    let result = response(b"committed".to_vec(), Some(Revision(7)));
    jobs.complete(&ticket, result.clone(), now).unwrap();
    assert_eq!(jobs.status(handle, now), Ok(Status::Finished(result)));
    assert_eq!(
        jobs.cancel(handle, now),
        Ok(Cancellation::AlreadyCommitted(Revision(7)))
    );
    assert_eq!(
        jobs.complete(&ticket, Completion::Cancelled, now),
        Err(Error::InvalidTicket)
    );
    let (next, ticket) = jobs.start(Duration::from_secs(30), now).unwrap();
    jobs.cancel(next, now).unwrap();
    jobs.complete(&ticket, Completion::Cancelled, now).unwrap();
    assert_eq!(jobs.cancel(next, now), Ok(Cancellation::Cancelled));
}

#[test]
fn active_capacity_is_not_evicted_even_after_deadlines() {
    let now = Instant::now();
    let mut jobs = Jobs::new(ObjectId::new());
    let pending: Vec<_> = (0..MAX_JOBS)
        .map(|_| jobs.start(Duration::from_secs(1), now).unwrap())
        .collect();
    assert!(matches!(
        jobs.start(Duration::from_secs(1), now),
        Err(Error::Capacity)
    ));
    let later = now + Duration::from_secs(2);
    jobs.maintain(later);
    assert!(
        pending
            .iter()
            .all(|(_, ticket)| ticket.cancellation().check().is_err())
    );
    assert!(matches!(
        jobs.start(Duration::from_secs(1), later),
        Err(Error::Capacity)
    ));
    jobs.complete(&pending[0].1, Completion::Cancelled, later)
        .unwrap();
    let _replacement = jobs.start(Duration::from_secs(1), later).unwrap();
    assert_eq!(jobs.status(pending[0].0, later), Err(Error::Expired));
    assert!(matches!(
        jobs.status(pending[1].0, later),
        Ok(Status::Running { .. })
    ));
}

#[test]
fn retention_deadlines_epoch_isolation_and_session_drop() {
    let epoch = ObjectId::new();
    let now = Instant::now();
    let mut jobs = Jobs::new(epoch);
    assert!(matches!(
        jobs.start(Duration::ZERO, now),
        Err(Error::Budget)
    ));
    assert!(matches!(
        jobs.start(MAX_LIFETIME + Duration::from_secs(1), now),
        Err(Error::Budget)
    ));
    let (handle, ticket) = jobs.start(MAX_LIFETIME, now).unwrap();
    let mut other = Jobs::new(ObjectId::new());
    assert_eq!(other.cancel(handle, now), Err(Error::Epoch));
    assert_eq!(
        other.complete(&ticket, Completion::Cancelled, now),
        Err(Error::Epoch)
    );
    let mut same_epoch = Jobs::new(epoch);
    assert_eq!(
        same_epoch.complete(&ticket, Completion::Cancelled, now),
        Err(Error::InvalidTicket)
    );
    jobs.complete(&ticket, response(b"error response".to_vec(), None), now)
        .unwrap();
    assert_eq!(jobs.cancel(handle, now), Ok(Cancellation::Finished));
    assert_eq!(jobs.status(handle, now + RETENTION), Err(Error::Expired));
    let (_, ticket) = jobs.start(Duration::from_secs(1), now).unwrap();
    let token = ticket.cancellation();
    drop(jobs);
    assert!(token.check().is_err());
}

#[test]
fn bounded_result_storage_evicts_completed_results_but_not_active_jobs() {
    let now = Instant::now();
    let mut jobs = Jobs::new(ObjectId::new());
    let (active, _) = jobs.start(MAX_LIFETIME, now).unwrap();
    let (first, ticket) = jobs.start(MAX_LIFETIME, now).unwrap();
    let bytes: Arc<[u8]> = vec![0; 16 * 1024 * 1024].into();
    jobs.complete(
        &ticket,
        Completion::Response {
            bytes: bytes.clone(),
            committed: None,
        },
        now,
    )
    .unwrap();
    let (_, second) = jobs.start(MAX_LIFETIME, now).unwrap();
    jobs.complete(
        &second,
        Completion::Response {
            bytes,
            committed: None,
        },
        now,
    )
    .unwrap();
    let (_, third) = jobs.start(MAX_LIFETIME, now).unwrap();
    jobs.complete(&third, response(vec![1], None), now).unwrap();
    assert_eq!(jobs.status(first, now), Err(Error::Expired));
    assert_eq!(
        jobs.status(active, now),
        Ok(Status::Running {
            cancellation_requested: false
        })
    );
    let (oversized, ticket) = jobs.start(MAX_LIFETIME, now).unwrap();
    assert_eq!(
        jobs.complete(&ticket, response(vec![0; 16 * 1024 * 1024 + 1], None), now),
        Err(Error::Budget)
    );
    assert!(matches!(
        jobs.status(oversized, now),
        Ok(Status::Running { .. })
    ));
    jobs.complete(&ticket, response(b"budget error".to_vec(), None), now)
        .unwrap();
}
