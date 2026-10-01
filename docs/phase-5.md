# Phase 5 — Shared render graph

## Goal

Replace the solid-only execution contract with a feature-neutral image DAG used
by both preview and headless delivery (product sections 6–8 and 10).

## Tasks and acceptance criteria

- Define full-frame image operations for solid sources, affine transforms,
  opacity, and ordered source-over compositing; providers return this shared IR.
- Validate dimensions, parameters, output and edges before evaluation. Require
  topological ordering (inputs precede consumers), rejecting cycles and dangling
  references. Execute only the output dependency closure, once per node.
- Use scene-linear sRGB premultiplied RGBA32F throughout. Specify pixel-center,
  top-left/X-right/Y-down coordinates, nearest-neighbor sampling and transparent
  borders. Preview and PPM retain the same explicit output transform.
- Bound graph size and live CPU pixel storage, release intermediates after their
  last consumer, and fail explicitly before an over-budget pixel allocation.
- Test transform placement, scale/rotation, alpha/opacity/order, shared inputs,
  dead work, invalid graphs and budgets, and display/headless equivalence.
- Format touched files, check affected crates, run focused tests and boundaries.

## Scope

Deterministic CPU reference implementation, one output resolution per graph,
SDR colors, full-frame nodes and one image port type. Node indices are transient
IR handles, not persistent authoring IDs. No new authoring schema or UI controls;
compositor authoring is phase 8. Media adapters, cache, asynchronous scheduling,
cancellation, GPU passes, ROI, transform fusion, filtering choices and richer
color profiles are deferred. Existing worker-based preview remains nonblocking.

## Verification

Phase 5 acceptance criteria passed:

- `rustfmt --edition 2024` on all five touched/new Rust files — passed.
- `cargo test -p fold-render -p fold-app --no-default-features --locked` —
  passed: 2 existing render tests, 5 new graph tests, and 2 app checkpoint tests.
  Fixtures cover translation, identity, scale, rotation, clipping, transparent
  borders, opacity endpoints, partial alpha, merge order, duplicate edges,
  shared dependencies, invalid/dead nodes, node/resolution limits, intermediate
  release, over-budget rejection, and byte-exact display/PPM agreement.
- `cargo check -p fold-render -p fold-platform -p fold-motion -p fold-app --features desktop --all-targets --locked` — passed.
- `cargo test -p fold-app --bin fold-desktop --features desktop --locked` —
  passed: the registered snapshot provider still produces the expected demo
  through the new shared graph contract.
- `python3 scripts/check_boundaries.py` — passed, including headless isolation.
- `git diff --check` — passed; reviewed changed code for duplication and scope.

The provider contract now returns `RenderGraph`; `SolidPlan` remains only a
one-node convenience adapter, not a second evaluator. Both existing desktop and
CLI call the shared evaluator without changes to document persistence or UI.
The 64 MiB limit covers live evaluator pixel buffers per invocation, not caller-
retained results, display copies, allocator overhead, or concurrent renders.
Graphs are limited to 4096 nodes and 4,194,304 pixels per frame. Execution uses
supplied topological order, not a memory-optimizing scheduler; an otherwise
valid graph can fail the live-buffer budget and is never silently simplified.

No native UI rerun was needed or performed: presentation code and visible demo
are unchanged. GPU evaluation, media-backed sources, seeking and delivery remain
future work. Workspace/full-suite/Clippy checks are deferred to the media slice
checkpoint as required by the iteration policy.
