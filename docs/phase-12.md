# Phase 12 — Persistent organization and asset-only ingest

## Goal and boundaries
Implement the phase-11 organization/ingest contracts without adding the phase-13 Project UI or changing placement, viewer bindings, transport, delivery selection, or color rendering. Project core remains feature-neutral. Linked originals are never copied into managed project storage or deleted.

## Tasks and acceptance
- [x] Stable bins/items, explicit root/order, transactional parent/cycle/target validation and history.
- [x] Archive-v1 migration with deterministic root items; opaque data and resources survive save/reopen.
- [x] Media-owned verified stream facts and separate input interpretation.
- [x] Bounded cancellable asset-only worker proposals, duplicate detection and atomic relink.
- [x] Empty-bin deletion, conservative target deletion and provider-validated relink.
- [x] Focused regression tests, touched-file formatting, headless and desktop build checks.

## Implementation

### Organization and persistence
`crates/project/src/organization.rs` owns `Organization`, `Bin`, `Item`, and explicit `Parent::Root` / `Parent::Bin`. Bins use foundation `BinId`s. With aliases deliberately excluded, `ItemId` is a typed `Asset(AssetId)` / `Document(DocumentId)` identity and target reference together. This makes one item per target structural, gives legacy migration deterministic identities without a new UUID namespace/dependency, and keeps asset/document identity unchanged by rename/move. Items and bins persist names, order, and extension fields; equal order values can be disambiguated by stable identity.

Coordinator `PutBin` / `PutItem` mutations create, rename, move and reorder; `RemoveBin` requires an empty bin at that point in the batch. Move contents explicitly before deleting their bin. Target removal also removes its item in the same transaction. Existing asset/document creation commands automatically discover new root items, so old creation paths remain usable until phase 13 replaces them. These operations participate in existing atomic validation, bounded history, fresh-revision undo/redo and transient edit infrastructure. Invalid cycles, parents, targets, duplicate serialized items and shadowing extension fields are rejected.

Archives now save as version 2 and load versions 1–2. Version 1 discovers missing organization records on the unpublished load copy; version 2 validates complete organization instead of silently repairing it. Migration does not parse provider payloads, change authored color, inspect media, or prune resources. Unknown payload bytes, extension data, dependency records and retained assets remain intact. Older binaries that only support archive v1 cannot open a newly saved v2 archive.

### Media facts
`crates/media/src/ingest.rs` adds worker-only `inspect_linked` and versioned asset extension contracts:
- `fold.media.source.v1`: validated MP4/H.264 `VideoInfo`, PCM16 WAVE `WaveInfo`, or P6 PPM dimensions; stream index/kind; native bit depth/plane/alpha facts where known; exact timing policy; verbatim bounded FFprobe stream/format metadata, including color, codec/profile, aspect/orientation side data, time base, durations and audio layout.
- `fold.media.input.v1`: separate optional color-space/alpha assignment with provenance. New links are explicitly unassigned rather than guessing an OCIO color space. Phase 16 supplies config-driven assignment UI and production transforms. Existing adapter/render color semantics are unchanged.

Inspection fingerprints before and after all probes, retains existing narrow format/profile validation, limits raw FFprobe metadata to 1 MiB and 32 streams, and checks cancellation through fingerprint/probe work. It never pins/copies original media into managed project storage. The renderer's existing temporary pinning remains separate.

### Services and policies
`crates/app/src/ingest.rs` exposes `ImportRequest`, `RelinkRequest`, `IngestWorker`, `IngestProposal`, and `delete_item`:
- One in-flight request per worker, a one-result channel, at most 32 import paths; busy requests fail rather than queue unbounded work. Polling is nonblocking. Dropping/cancelling the service signals its worker; shutdown does not join/block the UI.
- The worker retains an immutable snapshot. A proposal commits through `Project` in one history entry and rechecks cancellation plus expected revision at publication. Failed/cancelled/stale work never partially publishes. Duplicate-only import does not create an undo entry.
- Canonical location **and** verified fingerprint reuse an existing item without moving it. Changed bytes at the same location require relink. Same bytes at a different location remain distinct assets. Legacy relative/symlink locations are resolved on the worker without rewriting existing links.
- Relink preserves AssetId/item organization, user interpretation and unrelated asset extensions, while replacing verified location/fingerprint/probed metadata atomically. Destination collision with another asset is rejected rather than merging identities.
- Registered providers validate documents and affected uses through the explicit `DocumentProvider::validate_relink` capability. Timeline and compositor validate their own embedded source contracts. Referenced relinks require matching duration, stream selectors and native decode/color facts; incompatible replacements fail, never trim or rewrite clips. Missing providers/invalid documents fail closed, including potentially undeclared uses.
- `delete_item` returns a usage report for authored references, explicit delivery selection, or unavailable/invalid providers. Otherwise it proposes removal of the item and target together. It never deletes the linked file. Existing low-level coordinator mutations remain infrastructure; callers of the Project workflow must use this guarded deletion service.
- Existing content keys already include asset location/fingerprint, so dependent results change on relink without rewriting documents. Organization/import-only changes leave existing output content keys unchanged. Old snapshots and decoder-owned pinned source handles retain the original identity/content.

## Verification evidence

Passed:

```text
cargo test -p fold-project -p fold-media -p fold-platform -p fold-timeline -p fold-compositor
  35 tests including the project compile-fail doctest
cargo test -p fold-app --test project_ingest --test delivery_selection --test compositor_media --test media_sequence
  11 tests
cargo check -p fold-app
PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig cargo check -p fold-app --features desktop
rustfmt --edition 2024 --config skip_children=true <touched Rust files>
git diff --check
```

After final safety-validation changes, reran `organization` (2), platform `packages` (3), and app `project_ingest` (5), plus the desktop check: all passed. These reruns are not additional unique test counts.

New focused fixtures:
- `crates/project/tests/organization.rs`: bin create/rename/move/delete, item rename/move, duplicate names, cycle/dangling rejection with rollback, undo/redo, deterministic v1 migration, unknown payload/resource retention, duplicate-item archive rejection and v2 save/reopen.
- `crates/app/tests/project_ingest.rs`: actual worker import and relink, MP4/WAVE/PPM metadata, asset-only isolation with authored timeline/compositor documents and explicit delivery output, duplicate/no-op semantics, distinct-location reuse policy, changed-source rejection, cancellation before/after preparation, busy/stale requests, unsupported/missing files with all-or-nothing behavior, safe deletion/usage reports, unknown-provider rejection, interpretation retention, undo/redo and save/reopen.
- Relink fixture changes a red video to a compatible blue video, rejects a shorter video, and verifies timeline/compositor content invalidation. After removing the original file it forces a decoder presentation-resolution cache miss and successfully decodes from the retained original source handle. This is headless pinned-source evidence, not a claim of native concurrent export acceptance.

The first desktop check without the documented environment failed because `alsa.pc` was not in the default pkg-config path. Repeating with the existing phase-11 local ALSA build environment passed; no dependency installation or repository workaround was needed.

## Deliberate limits and next phase
- No Project UI, placement replacement, browser thumbnails, or new CLI command is included. The new service is ready for phase-13 wiring; legacy placement/import UI stays functional until that replacement passes. Viewer bindings/transports cannot be touched by these services, which have no session/UI handles. No native UI acceptance is claimed.
- Referenced legacy assets with no persisted verified source metadata fail relink conservatively: replacement inspection alone cannot prove that native streams/color match the missing original. They remain discoverable, loadable and preserved; unreferenced legacy assets can relink. This is an explicit safety rejection, not silent reinterpretation or payload migration.
- Compatibility is intentionally stricter than merely fitting a shorter clip: referenced media must retain the source contract. Broader stream/duration remapping requires an explicit future operation, not automatic relink.
- Ingest concurrency is bounded per service. The shared multi-consumer scheduler/aggregate budgets remain phases 18–19. Whole-source fingerprint costs and the existing decoder pinning strategy are not optimized here.
- Workspace suites, Clippy, package-boundary checkpoint checks and native Project workflow acceptance remain at the phase-13 slice checkpoint. No performance target or broad codec support is asserted.

Phase 12 acceptance is complete for this non-UI organization/ingest slice.
