# Phase 2 — Project state, exact time, transactions, and persistence

## Goal

Implement the feature-neutral foundation and project coordinator from product
specification sections 4, 5, and 9. Creative payloads remain opaque bytes; workers
retain immutable snapshots rather than borrowing mutable project state.

## Tasks

- Add stable document/asset/object IDs and normalized rational time with checked
  arithmetic, explicit boundary rounding, and half-open ranges.
- Add document envelopes, declared references, assets, settings/environment data,
  and immutable project snapshots.
- Stage edits against an expected revision; validate the entire candidate before
  publishing. Provide bounded global undo/redo and transient preview sessions.
- Save a versioned JSON archive with independent opaque document payloads using
  flushed temporary files and atomic replacement. Load without providers and
  preserve unknown envelope/manifest fields and payload bytes.
- Test failure atomicity, cross-document history, snapshot isolation, time
  boundaries, unknown-data round trips, and failed/interrupted saves.

## Acceptance criteria

- Exact fractional-rate and negative-time arithmetic/rounding is tested, including
  zero denominators, overflow, and adjacent half-open boundaries.
- A multi-document edit is one undo entry; invalid/stale edits publish nothing;
  retained snapshots remain unchanged. History is bounded; redo is invalidated by
  a new edit. Preview changes never alter committed state and commit once or cancel.
- Missing/cyclic declared document references and missing declared assets are
  rejected. Unavailable document providers do not prevent preservation of data.
- Save/reopen preserves IDs, revisions, opaque bytes, declared dependencies,
  assets, and unknown metadata. Unsupported archive versions fail explicitly.
- Failed saves do not replace the last valid archive; an abandoned temporary
  write does not prevent reopening that archive.
- Focused foundation/project tests and checks and the package boundary check pass.

## Scope and deferred work

The initial archive is one versioned JSON container, not a compressed ZIP; opaque
payloads are byte arrays. Assets are external manifest records, not embedded media.
Persistence is synchronous worker-side I/O; no UI is introduced. The initial
atomic-replacement behavior is verified on Linux. Recovery snapshots, provider
migration/typed validation, port/capability validation, and export dependency
reports await their providers/hardening. Unknown fields are preserved semantically;
opaque payload bytes are preserved exactly. No silent migrations or asset pruning.
The generated-image headless checkpoint remains a separate next task.

## Verification

Acceptance criteria passed locally on Linux:

- `cargo fmt -p fold-foundation -p fold-project` — touched crates formatted.
- `cargo check -p fold-foundation -p fold-project --all-targets --locked` — passed.
- `cargo test -p fold-foundation -p fold-project --locked` — all 13 tests passed
  (3 exact-time unit tests, 1 persistence fault-injection test, 7 project behavior
  tests, and 2 serialization/revision-limit tests); doc tests also passed.
- `python3 scripts/check_boundaries.py` — package boundaries and headless UI
  isolation passed with the new serialization/UUID/tempfile dependencies.
- `git diff --check` — passed. Reviewed changes for scope and duplication.

Tests cover cross-document create/remove atomicity, unchanged-root sharing,
retained snapshots, monotonic undo/redo revisions, stale proposals, cyclic and
missing references, missing assets, bounded/disabled history, redo invalidation,
preview commit/cancel, unknown payload and metadata round trips, stable IDs,
malformed/unsupported archives, reserved-field collisions, revision exhaustion,
replacement of existing archives, pinned older saves, failed replacement, injected
partial-write failure, and reopening with an abandoned temporary file present.

The phase tracker is complete. No full workspace suite or Clippy was run: those
are deferred to the foundation slice checkpoint, which still needs static provider
assembly and generated-image headless rendering. Crash/power-loss durability was
not tested by killing a process or machine; interruption coverage uses fault
injection and an abandoned-file fixture. Recovery snapshot retention, non-Linux
atomic replacement, provider-specific validation/migration, and byte-budgeted
history remain deferred as described above. No UI/render behavior is claimed.
