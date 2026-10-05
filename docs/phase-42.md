# Phase 42 — Editable motion solvers and typography study

Status: implemented and verified; native interaction acceptance remains unverified.
Authorized by the request to implement the [Cavalry reference study](research/cavalry-solver-workflows.md)
using reusable, customizable tools. This is a visual reconstruction, not the
creator's recovered Cavalry graph or a frame-identical match.

## Delivered construction

The Motion package now owns a **Solver** node with an editable update group.
**Previous State** supplies materialized geometry, delta time in seconds and
step index. Group inputs are live inputs, independently sampled at each step;
the group's State output becomes the next state. Initial geometry is sampled
at the start time. Content and Points are supported, with typed per-element
attributes. The solver has no effect-specific movement, collision or trail mode.

New ordinary nodes are **Read Attribute**, **Store Attribute**, **Geometry
History**, **Convex Sweep**, **Shape Grid**, **Direction**, and **Set Opacity**.
Math gains Modulo. Attribute lookup joins by stable element ID, with an explicit
fallback and type check; writes materialize field values on top-level elements.
Existing field, instancing, group, glyph-animation and transaction contracts are
reused. Project and renderer models have no simulation-specific additions.

The **Animate → Drifting Shapes** action takes a selected scene object, keeps
it as an editable hidden source, and creates a group with published distribution,
motion, appearance and trail controls. Edit Graph exposes the distribution,
solver, attribute write/read, convex sweep and history operations; the solver's
own Edit Graph opens the update. Convex sweep is opt-in, so the default action
retains arbitrary source silhouettes; the portrait fixture enables it for its
rectangular profile. Normal inspector edits and nested graph edits
use the existing preview/commit/undo lifecycle. New Solver nodes are also
available through graph search, with an initial identity update. State type can
be changed on an unmodified identity update; existing incompatible connections
must be disconnected first. Edited update graphs are never automatically retyped.

The typography study has a white portrait composition, small signature, four
lines of live text, an animated Shape Grid treatment and per-glyph displacement,
and drifting orange profiles with short sweeps and fading history. It is saved
as Motion → Composite → Sequence, with embedded font bytes. The fixture accepts
an optional font path; the reviewed artifact uses Liberation Sans Bold as a
substitute for the unidentified reference font. No soundtrack is included.

## Execution and resource contract

- Exact rational start and step durations. At/before start, return initial state.
  Later requests replay to `floor((time-start)/step)`; between steps, hold the
  completed state. Updates sample live inputs at the step's ending time. Pure
  nodes continue to evaluate directly at their requested time.
- Re-enter ancestor group bindings at each historical time, including animated
  ports. Never cache lazy field closures as state. Scope, solver node and scene
  object identity distinguish instances sharing an update graph.
- One thread-safe runtime per Motion video provider, shared by preview/export
  requests. Headless callers can supply an explicit runtime. Locks cover cache
  lookup/publication only; update graphs evaluate outside locks.
- Keep up to 128 checkpoints within a conservative 64 MiB retained payload
  budget. Retain every 16th step and requested completed states, evict by use,
  replay from a matching earlier checkpoint or initial conditions. Cached data
  is disposable and never saved into authoritative project content.
- Keys use canonical authored document content (excluding node positions),
  instance scope and object identity. Provider compilation reuses its prepared
  immutable root and identity; control overrides have distinct identities.
  This first implementation conservatively invalidates for unrelated semantic
  document edits too; dependency-specific invalidation is a future optimization.
- Work shares the existing cancellable evaluation budget. Bound each state to
  64 MiB of estimated payload, depth 32 and 16384 elements per collection;
  require unique sibling IDs. Limit requested integration to 100000 steps and
  step duration to at least 1 ms. Reject image effects inside solver state.
  These payload bounds are not measurements of allocator or process RSS.
- Geometry History samples up to 128 upstream states at exact intervals and
  exposes `history_age` and `history_index`. It supports pure animation as well
  as solver output. Fade applies to sampled geometry, separately from movement.
- Convex Sweep constructs the convex envelope of a uniformly styled profile and
  its translated copy. It intentionally fills concavities/holes; it is not a
  general nonconvex extrusion or collision operation. Samples are bounded and
  cancellable. Shape Grid uses local-space cell-center occupancy with winding
  rules, returns native paths, and preserves original geometry at size ≤1 px.

Current deliberate limits: no nested Solver/History sampling inside an update,
no disk bake/cache format, no automatic substep interpolation/motion-blur contract,
and no rigid-body collision library. Explicit attributes can retain numeric
history. Content-changing graphs must author their own stable IDs/topology
correspondence. These limits fail explicitly rather than depend on play order.

## Reproduction

```sh
cargo run -p fold-app --release --example cavalry_recreation -- \
  /tmp/fold-typography-demo.fold /tmp/fold-typography-demo-preview \
  /usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf
```

Destinations must not exist. Omit the font argument to use the bundled Noto font.
The source is 360×640, 25 fps, 268 frames; the preview samples every other frame.
The shared example writer now derives dimensions/duration/frame range from the
Motion document instead of assuming a 960×540 ten-second demo.

```sh
ffmpeg -framerate 25/2 -i /tmp/fold-typography-demo-preview/%04d.ppm \
  -c:v libx264 -pix_fmt yuv420p /tmp/fold-typography-demo.mp4
cargo run -p fold-app --release --features desktop --bin fold-desktop -- \
  --project /tmp/fold-typography-demo.fold
```

## Verification

- `cargo test -p fold-motion --features ui --quiet -- --test-threads=1`: 86
  tests passed; one native rendering test is ignored in the ordinary suite.
- Explicit native rendering test run separately for Solver and Drifting Shapes:
  both passed. Normal and narrow inspector captures reviewed on Vulkan llvmpipe.
- `cargo test -p fold-app --test solver_workflow --test motion_workflow --quiet`:
  four integration tests passed, including provider/headless pixel parity with
  overrides and undo snapshots.
- Release desktop build, touched-file `rustfmt --check`, and `git diff --check`
  passed. Workspace-wide tests and Clippy were deferred per iteration policy.
- Delivered `/tmp/fold-typography-demo.fold` and
  `/tmp/fold-typography-demo.mp4` (silent, 10.72 seconds, every-other-frame preview).
  All 134 preview frames rendered. This is a visual reconstruction with a
  substitute font and independently generated trajectories.
- Live desktop gestures, reference-hardware latency and real-time playback are
  unverified. Captures exercise the actual ImGui/backend layout, not OS input.

[Animation contact sheet](media/phase-42-typography.png) ·
[Solver inspector](media/phase-42-solver-narrow.png) ·
[Drifting Shapes inspector](media/phase-42-drift-narrow.png)

Focused tests cover random/reverse/cold seeks, cache eviction, live inputs through
nested editable groups, instance separation, exact fractional timing, cancellation,
provider/headless parity with control overrides, document roundtrip and undo.
A second update graph accumulates/decays a `heat` attribute without moving any
geometry, using the same Solver and ordinary Math/attribute nodes.
Geometry checks cover filled-cell holes, original-curve bypass, swept silhouettes,
alpha preservation and resource-limit errors. Normal/narrow native-backend
captures inspect the actual inspector rather than a UI mockup.

## Follow-up — edit command payload regression

The launched reference project exposed a gap in the original acceptance checks:
Motion UI edits serialized the entire document into each command. The embedded
font contributed 1197500 of 1227857 JSON bytes, so even moving one network node
hit the host's 1 MiB command limit. The original tests used small documents and
direct document commits, which did not catch this command transport failure.

Existing-document UI edits now send `fold.motion.change.v1`, a sparse set of
changed authored values against the host-checked base revision. Changed objects
and equal-length arrays are compared recursively; resized arrays are replaced.
Only changed top-level fields become JSON during construction/staging, keeping
unchanged font resources as typed bytes. The owning Motion package validates the
result and returns the same atomic document transaction. Unknown extension data,
preview/cancel and undo/redo are preserved. Initial document creation and the
legacy full-replacement API remain available. The host command budget is unchanged;
a single operation introducing more than 1 MiB of new data still exceeds it.

Reproduction/regression checks:

```sh
cargo test -p fold-motion --features ui --quiet -- --test-threads=1
FOLD_EDIT_PROJECT=/tmp/fold-typography-demo.fold \
  cargo test -p fold-app --test motion_large_edit --quiet -- --ignored
```

The embedded-font UI regression first failed with the exact reported error.
The reference-project replay also first failed and now asserts its node-move
command is below 1 KiB. Additional coverage checks changed font metadata,
optional fields, node/group creation, extension insertion/removal, invalid paths,
stale revisions, cancellation and atomic rejection. No panel layout changed.

Follow-up verification: all 88 Motion tests passed (one native layout capture
remains ignored in the ordinary suite), and the explicit saved-project replay
passed. Release desktop build, touched-file formatting and diff checks passed.
Opened the rebuilt demo on the shared GTX 1070 backend in a new window; the
previous process was left intact to preserve any user state. Native mouse/keyboard
acceptance of this fix remains unverified; the regression exercises the same
Motion UI command generation and host staging path programmatically.
