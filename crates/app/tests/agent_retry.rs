mod support;
use fold_app::agent::retry::*;
use fold_foundation::ObjectId;
use std::time::{Duration, Instant};

fn ticket(
    tracker: &mut Tracker,
    epoch: ObjectId,
    id: ObjectId,
    bytes: &[u8],
    now: Instant,
) -> Ticket {
    match tracker.begin(epoch, id, bytes, now).unwrap() {
        Admission::Execute(ticket) => ticket,
        _ => panic!("not a fresh request"),
    }
}
fn error(result: Result<Admission, Error>, expected: Error) {
    assert!(matches!(result, Err(actual) if actual == expected));
}

#[test]
fn identical_retries_share_one_execution_and_the_exact_response_bytes() {
    let epoch = ObjectId::new();
    let id = ObjectId::new();
    let now = Instant::now();
    let mut tracker = Tracker::new(epoch);
    let bytes = br#"{"edit":1}"#;
    let first = ticket(&mut tracker, epoch, id, bytes, now);
    error(
        tracker.begin(epoch, id, br#"{"edit":2}"#, now),
        Error::IdempotencyConflict,
    );
    assert!(matches!(
        tracker.begin(epoch, id, bytes, now),
        Ok(Admission::Pending)
    ));
    error(
        tracker.begin(epoch, id, br#"{ "edit":1}"#, now),
        Error::IdempotencyConflict,
    );
    let outcome = b"{\"ok\":true,\"revision\":7}\n".to_vec();
    tracker.complete(&first, outcome.clone(), now).unwrap();
    let Admission::Replay(replay) = tracker.begin(epoch, id, bytes, now).unwrap() else {
        panic!("missing replay")
    };
    assert_eq!(replay.as_ref(), outcome);
    assert!(matches!(
        tracker.complete(&first, b"different".to_vec(), now),
        Err(Error::InvalidTicket)
    ));
    error(
        tracker.begin(epoch, id, b"changed", now),
        Error::IdempotencyConflict,
    );
    error(tracker.begin(ObjectId::new(), id, bytes, now), Error::Epoch);
}

#[test]
fn response_eviction_retains_conflict_checks_and_never_reexecutes() {
    let epoch = ObjectId::new();
    let now = Instant::now();
    let mut tracker = Tracker::new(epoch);
    let old = ObjectId::new();
    let first = ticket(&mut tracker, epoch, old, b"original", now);
    tracker
        .complete(&first, b"committed".to_vec(), now)
        .unwrap();
    for _ in 0..MAX_CACHED_RESPONSES {
        let next = ticket(&mut tracker, epoch, ObjectId::new(), b"next", now);
        tracker.complete(&next, b"done".to_vec(), now).unwrap();
    }
    error(tracker.begin(epoch, old, b"original", now), Error::Expired);
    error(
        tracker.begin(epoch, old, b"changed", now),
        Error::IdempotencyConflict,
    );
    let timed = ObjectId::new();
    let next = ticket(&mut tracker, epoch, timed, b"timed", now);
    tracker
        .complete(&next, b"error outcome".to_vec(), now)
        .unwrap();
    error(
        tracker.begin(epoch, timed, b"timed", now + RESPONSE_LIFETIME),
        Error::Expired,
    );
    error(
        tracker.begin(epoch, timed, b"changed", now + RESPONSE_LIFETIME),
        Error::IdempotencyConflict,
    );
}

#[test]
fn active_requests_are_not_evicted_and_capacity_rejections_stay_rejected() {
    let epoch = ObjectId::new();
    let now = Instant::now();
    let mut tracker = Tracker::new(epoch);
    let ids: Vec<_> = (0..MAX_PENDING).map(|_| ObjectId::new()).collect();
    let tickets: Vec<_> = ids
        .iter()
        .map(|id| ticket(&mut tracker, epoch, *id, b"pending", now))
        .collect();
    let rejected = ObjectId::new();
    error(
        tracker.begin(epoch, rejected, b"rejected", now),
        Error::Capacity,
    );
    let later = now + Duration::from_secs(3600);
    assert!(matches!(
        tracker.begin(epoch, ids[0], b"pending", later),
        Ok(Admission::Pending)
    ));
    tracker.abandon(&tickets[0]).unwrap();
    error(
        tracker.begin(epoch, ids[0], b"pending", later),
        Error::Expired,
    );
    error(
        tracker.begin(epoch, rejected, b"rejected", later),
        Error::Capacity,
    );
    error(
        tracker.begin(epoch, rejected, b"changed", later),
        Error::IdempotencyConflict,
    );
    let _new = ticket(&mut tracker, epoch, ObjectId::new(), b"admitted", later);
}

#[test]
fn byte_budgets_and_session_tombstone_capacity_do_not_allow_uncertain_retries() {
    let epoch = ObjectId::new();
    let now = Instant::now();
    let mut tracker = Tracker::new(epoch);
    error(
        tracker.begin(epoch, ObjectId::new(), &vec![0; MAX_REQUEST_BYTES + 1], now),
        Error::Budget,
    );
    let id = ObjectId::new();
    let active = ticket(&mut tracker, epoch, id, b"active", now);
    assert!(matches!(
        tracker.complete(&active, vec![0; MAX_RESPONSE_BYTES + 1], now),
        Err(Error::Budget)
    ));
    assert!(matches!(
        tracker.begin(epoch, id, b"active", now),
        Ok(Admission::Pending)
    ));
    tracker
        .complete(&active, b"budget error".to_vec(), now)
        .unwrap();
    for _ in 1..MAX_REQUEST_IDS {
        let next = ticket(&mut tracker, epoch, ObjectId::new(), b"next", now);
        tracker.abandon(&next).unwrap();
    }
    error(
        tracker.begin(epoch, ObjectId::new(), b"new", now + RESPONSE_LIFETIME),
        Error::Capacity,
    );
    error(
        tracker.begin(epoch, id, b"active", now + RESPONSE_LIFETIME),
        Error::Expired,
    );
    error(
        tracker.begin(epoch, id, b"changed", now + RESPONSE_LIFETIME),
        Error::IdempotencyConflict,
    );
}

#[test]
fn response_memory_budget_evicts_oldest_and_tickets_cannot_cross_trackers() {
    let epoch = ObjectId::new();
    let now = Instant::now();
    let mut tracker = Tracker::new(epoch);
    let first_id = ObjectId::new();
    let first = ticket(&mut tracker, epoch, first_id, b"first", now);
    let mut other = Tracker::new(epoch);
    let _other_ticket = ticket(&mut other, epoch, first_id, b"first", now);
    assert!(matches!(
        other.complete(&first, vec![], now),
        Err(Error::InvalidTicket)
    ));
    tracker
        .complete(&first, vec![1; MAX_RESPONSE_BYTES], now)
        .unwrap();
    let next = ticket(&mut tracker, epoch, ObjectId::new(), b"second", now);
    tracker
        .complete(&next, vec![2; MAX_RESPONSE_BYTES], now)
        .unwrap();
    let last = ticket(&mut tracker, epoch, ObjectId::new(), b"third", now);
    tracker.complete(&last, vec![3], now).unwrap();
    error(
        tracker.begin(epoch, first_id, b"first", now),
        Error::Expired,
    );
}

#[test]
fn retried_candidate_apply_commits_once_even_after_artist_undo() {
    use fold_foundation::DocumentId;
    use fold_motion_canvas::model::Canvas;
    use fold_project::proposals::Store;
    use fold_project::{EditBatch, Mutation, Project};
    let now = Instant::now();
    let id = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                support::motion().document(id).unwrap(),
            )],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let original = Canvas::from_document(&snapshot.state().documents[&id]).unwrap();
    let candidate = fold_app::agent::dispatch::native::prepare_scoped(
        &snapshot,
        fold_platform::proposals::Scope { document: id },
        support::motion_request(id, serde_json::json!({"native":original}), 123.),
        &fold_media::Cancel::default(),
    )
    .unwrap();
    let mut candidates = Store::default();
    let handle = candidates.insert(candidate, now).unwrap();
    let inspected = candidates.inspect(handle, now).unwrap();
    let epoch = candidates.epoch();
    let mut tracker = Tracker::new(epoch);
    let request = ObjectId::new();
    let bytes = serde_json::to_vec(&(handle, inspected.receipt, snapshot.revision())).unwrap();
    let admitted = ticket(&mut tracker, epoch, request, &bytes, now);
    let applied = candidates
        .apply(
            handle,
            inspected.receipt,
            snapshot.revision(),
            &mut project,
            now,
        )
        .unwrap();
    let response = serde_json::to_vec(&applied).unwrap();
    tracker.complete(&admitted, response.clone(), now).unwrap();
    project.undo().unwrap();
    let undone = project.snapshot().revision();
    let Admission::Replay(replay) = tracker.begin(epoch, request, &bytes, now).unwrap() else {
        panic!("apply replay must not execute")
    };
    assert_eq!(replay.as_ref(), response);
    assert_eq!(project.snapshot().revision(), undone);
    assert_eq!(
        Canvas::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        original
    );
    error(
        tracker.begin(epoch, request, &bytes, now + RESPONSE_LIFETIME),
        Error::Expired,
    );
    assert_eq!(project.snapshot().revision(), undone);
}
