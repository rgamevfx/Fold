use fold_foundation::DocumentId;
use fold_motion::{
    candidates::{Cancellation, Error, IDLE_LIFETIME, MAX_LIVE, Store},
    editing,
    model::Construction,
};
use fold_project::{EditBatch, Mutation, Project};
use std::time::{Duration, Instant};

fn project() -> (Project, DocumentId) {
    let id = DocumentId::new();
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                fold_motion::package::create(id).unwrap(),
            )],
        })
        .unwrap();
    (project, id)
}
fn proposal(project: &Project, document: DocumentId, seed: u64) -> editing::Candidate {
    let snapshot = project.snapshot();
    let mut value = Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    value.seed = seed;
    editing::prepare(&snapshot, document, editing::Scope::Document, value).unwrap()
}

#[test]
fn generations_require_their_own_inspection_and_cancel_reports_committed() {
    let (mut project, id) = project();
    let base = project.snapshot().revision();
    let now = Instant::now();
    let mut store = Store::default();
    let first = store.insert(proposal(&project, id, 1), now).unwrap();
    let second = store.insert(proposal(&project, id, 2), now).unwrap();
    let receipt = store.inspect(first, now).unwrap().receipt;
    assert!(store.document(second, now).is_ok());
    assert_eq!(
        store.apply(second, receipt, base, &mut project, now),
        Err(Error::Uninspected)
    );
    assert_eq!(project.snapshot().revision(), base);
    assert_eq!(store.inspect(first, now).unwrap().receipt, receipt);
    let applied = store
        .apply(first, receipt, base, &mut project, now)
        .unwrap();
    assert_eq!(
        store.cancel(first, now),
        Ok(Cancellation::AlreadyCommitted(applied))
    );
    assert_eq!(
        store.apply(first, receipt, base, &mut project, now),
        Err(Error::AlreadyCommitted(applied))
    );
    assert_eq!(project.snapshot().revision(), applied);
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&id])
            .unwrap()
            .seed,
        0
    );
    assert_eq!(
        store.cancel(first, now),
        Ok(Cancellation::AlreadyCommitted(applied))
    );
}

#[test]
fn rebase_retains_old_generation_and_requires_new_inspection() {
    let (mut project, id) = project();
    let now = Instant::now();
    let mut store = Store::default();
    let handle = store.insert(proposal(&project, id, 7), now).unwrap();
    let original = store.inspect(handle, now).unwrap();
    let snapshot = project.snapshot();
    let mut artist = Construction::from_document(&snapshot.state().documents[&id]).unwrap();
    let node = artist.definitions[0].nodes[0].id;
    artist.presentation.positions.insert(node, [100., 200.]);
    editing::prepare(&snapshot, id, editing::Scope::Document, artist.clone())
        .unwrap()
        .apply(&mut project)
        .unwrap();
    assert_eq!(
        store.apply(handle, original.receipt, original.base, &mut project, now),
        Err(Error::Conflict)
    );
    let current = project.snapshot();
    let next = store.rebase(handle, &current, now).unwrap();
    assert_ne!(next, handle);
    assert_eq!(store.inspect(handle, now).unwrap().source, original.source);
    assert_eq!(
        store.apply(
            next,
            original.receipt,
            current.revision(),
            &mut project,
            now
        ),
        Err(Error::Uninspected)
    );
    let inspected = store.inspect(next, now).unwrap();
    assert_ne!(inspected.receipt, original.receipt);
    assert_eq!(inspected.changes.len(), 1);
    let value = fold_motion::source::parse(&inspected.source)
        .unwrap()
        .construction;
    assert_eq!(value.seed, 7);
    assert_eq!(value.presentation, artist.presentation);
    store
        .apply(next, inspected.receipt, inspected.base, &mut project, now)
        .unwrap();
    project.undo().unwrap();
    assert_eq!(
        Construction::from_document(&project.snapshot().state().documents[&id]).unwrap(),
        artist
    );
}

#[test]
fn live_capacity_expiry_epochs_and_terminal_retention_are_bounded() {
    let (project, id) = project();
    let now = Instant::now();
    let mut store = Store::default();
    let candidate = proposal(&project, id, 1);
    let handles: Vec<_> = (0..MAX_LIVE)
        .map(|_| store.insert(candidate.clone(), now).unwrap())
        .collect();
    assert_eq!(store.insert(candidate.clone(), now), Err(Error::Capacity));
    assert_eq!(
        store.rebase(handles[0], &project.snapshot(), now),
        Err(Error::Capacity)
    );
    assert_eq!(Store::default().cancel(handles[0], now), Err(Error::Epoch));
    store
        .inspect(handles[0], now + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        store.cancel(handles[1], now + IDLE_LIFETIME),
        Err(Error::Expired)
    );
    assert_eq!(
        store.cancel(handles[0], now + IDLE_LIFETIME),
        Ok(Cancellation::Cancelled)
    );
    assert_eq!(
        store.cancel(handles[0], now + IDLE_LIFETIME),
        Ok(Cancellation::Cancelled)
    );
    assert!(store.insert(candidate, now + IDLE_LIFETIME).is_ok());
    assert_eq!(
        store.cancel(handles[0], now + IDLE_LIFETIME + Duration::from_secs(600)),
        Err(Error::Expired)
    );
}

#[test]
fn conflicting_rebase_does_not_consume_or_modify_the_generation() {
    let (mut project, id) = project();
    let now = Instant::now();
    let mut store = Store::default();
    let handle = store.insert(proposal(&project, id, 1), now).unwrap();
    let original = store.inspect(handle, now).unwrap();
    proposal(&project, id, 2).apply(&mut project).unwrap();
    let revision = project.snapshot().revision();
    assert!(matches!(
        store.rebase(handle, &project.snapshot(), now),
        Err(Error::Edit(editing::Error::RebaseConflict(_)))
    ));
    assert_eq!(store.inspect(handle, now).unwrap().source, original.source);
    assert_eq!(project.snapshot().revision(), revision);
    assert_eq!(store.cancel(handle, now), Ok(Cancellation::Cancelled));
    assert!(matches!(store.inspect(handle, now), Err(Error::Cancelled)));
}

#[test]
fn inspection_authority_requires_publication_and_rejects_retired_generations() {
    let (mut project, id) = project();
    let base = project.snapshot().revision();
    let now = Instant::now();
    let mut store = Store::default();
    let handle = store.insert(proposal(&project, id, 42), now).unwrap();
    let first = store.prepare_inspection(handle, now).unwrap();
    let concurrent = store.prepare_inspection(handle, now).unwrap();
    let (inspection, proof) = first.run().unwrap();
    assert_eq!(
        store.apply(handle, inspection.receipt, base, &mut project, now),
        Err(Error::Uninspected)
    );
    store.publish_inspection(proof, now).unwrap();
    let (second, proof) = concurrent.run().unwrap();
    assert_eq!(second.receipt, inspection.receipt);
    store.publish_inspection(proof, now).unwrap();
    let pending = store.prepare_inspection(handle, now).unwrap();
    store
        .apply(handle, inspection.receipt, base, &mut project, now)
        .unwrap();
    let (_, proof) = pending.run().unwrap();
    assert!(matches!(
        store.publish_inspection(proof, now),
        Err(Error::AlreadyCommitted(_))
    ));

    let cancelled = store.insert(proposal(&project, id, 99), now).unwrap();
    let pending = store.prepare_inspection(cancelled, now).unwrap();
    store.cancel(cancelled, now).unwrap();
    let (_, proof) = pending.run().unwrap();
    assert_eq!(store.publish_inspection(proof, now), Err(Error::Cancelled));

    let expiring = store.insert(proposal(&project, id, 101), now).unwrap();
    let (_, proof) = store
        .prepare_inspection(expiring, now)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(
        store.publish_inspection(proof, now + IDLE_LIFETIME + Duration::from_secs(1)),
        Err(Error::Expired)
    );
}
