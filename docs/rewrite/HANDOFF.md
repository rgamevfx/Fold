# Motion rewrite checkpoint — 9 October 2026

## Resume here

### M5b continuation — geometry attributes and live Wrangle

The requested M5b implementation is present. Read [M5b evidence](evidence/m5b-result.md)
and the M5b phase before continuing. The requested recovery artwork is saved at
`demos/m5b-background-spheres.fold`; the independent programmable example is
`demos/m5b-wrangle.fold`. Both have public save/reopen source and pixel checks.
The running development session uses the edited temporary recovery copy under
`/tmp/fold-m5b-szf4gw17`; preserve its window and unsaved state. Original recovery
artwork under `/tmp/fold-freeze-diagnosis` was not overwritten; the old recovery
process had already exited normally before this work.

Grid and Copy to Points now use version 2 and Geometry ports. Old version-1
records stay unsupported; no runtime migration or aliases were added. Current
handoff/render tests use explicitly re-authored M5b fixture copies. Rhai 1.26.1
runs in-process with a closed host API; source edits require no build/restart.
See `skills/fold/references/wrangle.md` for public contracts and verified examples.

Physical artist acceptance and the pre-existing desktop-freeze investigation
remain open. Do not advance M6. The older process paths and status below are
historical; inspect running processes before assuming any session survives.

### Previous M5a report

**Latest report:** After commit `6db32c6`, the user reported UI corruption and a
freeze, but cannot recall the trigger. Desktop workflow acceptance is reopened.
Read [the incident record](evidence/m5a-desktop-freeze.md). A recovery copy is
running under `/tmp/fold-freeze-diagnosis/monitor.py`; preserve that process and
inspect its event/desktop logs if the user reports another freeze. No fix has
been established. The earlier passing evidence below does not dismiss this report.

The compositor cutover and mixed-scene implementation are complete in this
continuation. Read the [final cutover record](evidence/m5a-compositor-cutover/result.md)
and [28-operation audit](m5a-compositor-inventory.md) before starting new work.
The session-start uncommitted public-scene work is retained; the original
`demos/Make-it-move.fold` remains separate from `demos/m5a-mixed-scene.fold`.

Schema 8 is the only executable compositor model. Shared native operation,
parameter, connection, mask and override identities own persistence and edits;
`OperationConfig` is a temporary kernel/UI projection. Network Source and public
scoped edits use expected-value/protection gates. Inspector now follows nested
instance occurrence clocks and preserves navigation across edits. The final
fixture was authored and saved entirely through public v2 tools.

The historical “missing spheres” PNG was mislabeled: a reproducible pixel audit
shows its sphere region exactly matches the later frames. No renderer fix is
claimed. Native GPU public/preview agreement and source-edit cycles pass.

Automated workflow and normal/narrow native rendered review are recorded in the
cutover evidence. Physical artist acceptance remains unverified: live computer
use was screenshots-only, and the capture runtime could not start. Do not invent
human acceptance or waive earlier M0–M5 obligations. **Do not advance M6.**

The checkpoint passed: workspace 721 passed/116 ignored; final compositor safety
review 46 passed/1 ignored; 4 GPU and 8 public project tests passed separately.
Workspace check/Clippy, format and boundaries passed (existing lint warnings).
The final release is running on `/tmp/fold-m5a-delivery-354ewy2h/M5a mixed scene.fold`;
its held frame exactly matches reviewed evidence. The commit and normal launch
are recorded in the cutover record. Preserve existing application windows and their unsaved state. The older
sections below describe previous checkpoints, not remaining compositor work.

### Build and verification context

Use the current worktree `/home/rgame/.codex/worktrees/efc8/Fold`. The last disk
check showed roughly 40 GiB free. These settings avoid recreating the large
incremental/debug caches that were just removed:

```sh
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime
export FOLD_COLOR_ROOT=/tmp/fold-ocio-package
export FOLD_TEST_COLOR_ROOT=/tmp/fold-ocio-package
```

Verify that temporary runtime directories and any session sockets still exist.
Use offline Cargo while the dependency cache is available. Native Vulkan and
local-socket integration tests require execution permission beyond the sandbox.
The [current evidence](evidence/m5a-public-scene-result.md) records exact commands,
the durable workspace log, passed checks and remaining Clippy warnings. A normal
release build has not yet been qualified for this uncommitted continuation.

## Historical committed checkpoint — fcad1e9

The remainder records the earlier commit, not current capability status.

This is an **incomplete M5a checkpoint**, committed at the user's request. M3–M5
and M5a still have open acceptance gates; M6 must not start. The user asked to
finish all six remaining M5a areas and has not approved deferrals. They also
explicitly rejected legacy project shims: no old engine/protocol aliases,
migrations or execution fallbacks. Keep unsupported payloads opaque.

The latest requested deliverable is a **new Make it move project using a
Composition and Outliner**, with three separately editable layers/graphs:

1. A backdrop.
2. A text reveal reading exactly **Make it move**.
3. An array of randomly colored **spheres using Copy to Points**.

Keep each graph small and direct. Do not expose parameters, add demonstration
fixtures, or flatten all three into one graph. Preserve the existing demo.
This requested project **has not been created**. It is an additional immediate
artist fixture, not a waiver of the plan's mixed two-scene/compositor acceptance.

Live public discovery on 9 October confirmed two missing capabilities:

- Wire v2 can inspect/edit native Construction documents, but cannot create a
  Composition or author scene/Outliner objects through guarded public candidates.
- The catalog offers Grid, Cloner (points), Random and Box, but no Sphere primitive.

Implement and qualify these capabilities, update the public Fold skill/examples,
then author the new demo through public discovery → proposal → inspection → Apply
→ observation → save. Do not bypass the gap with raw `.fold` payload edits or
an alternate legacy protocol. Repository implementation may inspect Rust;
artwork authoring must use public tools. No artwork edit was applied during the
failed demo attempt; only discovery was performed.

### Current implementation and next seams

- `crates/graph`: shared typed graph records, bindings, tracks, expressions and
  curve projections; the superseded Motion copies and standalone timeline are removed.
- `crates/motion/src/scene*`, `ui/scene.rs`, shared `platform/hierarchy.rs`,
  `ui/outliner.rs`, and `composition/src/ui/hierarchy.rs`: native scene hierarchy,
  object graphs, shared selection/animation and Outliner/Composition rows.
- `crates/foundation/src/{affine,rotation}.rs`, Motion/render adapters: affine
  parenting and shared quaternion sampling. See the animation/parenting evidence.
- Compositor schema 7 stores shared native tracks and image-input edges. Schema
  1–6 readers are removed and opaque save/reopen is verified. Static `Parameters`,
  operation/document authoring, effect-mask and reusable instance-control storage,
  scoped source and public authoring still need the full replacement. Follow the
  [28-operation inventory](m5a-compositor-inventory.md); retain valid image kernels.
- `crates/app/src/agent/{wire,dispatch}.rs` and `dispatch/prepare.rs` remain tied
  to Construction candidates. Scene-provider authoring functions alone do not
  qualify scene creation, scoped candidates or atomic mixed-document public edits.
- Add and qualify Sphere through the native registry/evaluator and renderer for
  the new requested fixture; do not substitute boxes or raster images.

### Validation and known limits

Evidence is split by implementation scope:
[foundation](evidence/m5a-foundation-result.md),
[scene hierarchy](evidence/m5a-scene-result.md),
[animation/parenting](evidence/m5a-animation-parenting-result.md), and
[compositor records](evidence/m5a-compositor-native-records-result.md).
Latest compositor checks: 36 tests including an offscreen capture; 15 app
integration tests; subsequent protected-instance and opaque-payload regressions
passed. Workspace/all-target/all-feature checks and package boundaries passed;
Clippy completed with existing warnings. These are not a full green workspace
test gate or physical artist acceptance.

At this commit checkpoint, `cargo fmt --all -- --check`, `git diff --check`,
`python3 scripts/check_boundaries.py`, and the offline workspace/all-target/
all-feature Cargo check passed again. Full test suites were not repeated for
this documentation/commit-only step.

The earlier full workspace run had failures involving media dimensions, untagged
video profile expectations and media process capacity. A handoff deadline failed
in the broad run but passed alone. Reconcile those recorded failures at the final
checkpoint; do not erase or relabel them as passing. No profiling proves interactive
performance yet. Current desktop builds are unoptimized dev builds using shared
Vulkan rendering on the NVIDIA GTX 1070, not CPU-only 3D rendering.

The user permits **screenshots only**, no computer-use clicks, typing or dragging.
No subagents were authorized. Leave any existing live windows/unsaved work intact.

### Last launched sessions (temporary, rediscover before use)

- Latest built desktop opened an editable copy at
  `/tmp/fold-m5a-review-yglgvbtu/Make It Move.fold`.
- A separate authoring window opened the same saved copy with a private v2 endpoint
  `/tmp/fold-simple-demo-protocol/fold-session-OeDoE2/protocol.sock`.
- Discovery was saved to `/tmp/fold-simple-demo-discovery.json`; it reported only
  the existing native Construction document. Epochs/revisions and socket paths are
  session-specific and must never be reused without discovery.
- The checked-in `demos/m5-integrated.fold` remains the older flat 35-node demo.
  Neither launched window contains the newly requested three-layer artwork.
- `target/debug/fold-desktop` and `fold-cli` were rebuilt from this checkout.
  Build environment and launcher are below. `/tmp` artifacts are not durable evidence.

**Course correction:** The user subsequently requested the shared typed Compositing
rewrite and unified scene/object authoring workflow recorded in
[M5a](../Motion_System_Rewrite_Plan.md#m5a--shared-typed-graphs-scene-hierarchy-and-unified-authoring).
Freeze those contracts during current M0–M5 work. The standalone Motion panel and
flat whole-scene graph are not the target artist workflow. M6 now requires M5a
acceptance as well as M5; the demo instructions below describe the current build,
not acceptance of the corrected workflow.

The user resumed M0–M5 implementation and requested a demo to test personally.
**M0–M2 retain their recorded gates; M3–M5 are still not fully accepted.** Do not
mark them complete from the working demo or the new catalog tests.

## M5a continuation

Shared typed primitives live in `fold-graph`. Scene/object documents, Outliner,
per-object Network/source scopes and nested Composition rows are implemented.
**Add Motion layer** and its creation command are removed; Project/Composition
create **Scene**. Existing native Construction resources remain editable. The
flat M5 demo is still an earlier fixture, not the final M5a artwork.

The 9 October continuation adds affine nonuniform parenting, shared typed
Bézier/quaternion curve projection, reverse and paused clock views, reusable
occurrence/override tracks, and compact Inspector key controls. See
[evidence](evidence/m5a-animation-parenting-result.md) for verification and limits.
The earlier [scene evidence](evidence/m5a-scene-result.md) records its original scope.

The next 9 October continuation replaces compositor animation storage with shared
native parameter/track/key records and image inputs with shared edges, using
schema 7 (schema 1–6 readers removed). See [native compositor record evidence](evidence/m5a-compositor-native-records-result.md).
The static Parameters/operation model, scoped source and public protocol are
still pending. Do not claim a complete compositor rewrite from shared records.

**The user requested ALL six remaining M5a work areas. That request is not yet
fulfilled.** Full compositor document/authoring/source replacement, version-2
public scene/object and mixed-document candidates, the public two-scene demo,
complete Inspector/curve workflow acceptance and the workspace gate remain open.
No deferrals were approved. Do not claim this continuation completed M5a, and do
not advance M6. The [28-entry inventory](m5a-compositor-inventory.md) remains binding.

The user permits **screenshots only**, no computer-use clicks, typing or drags.
Offscreen GPU captures do not establish physical interaction acceptance. Existing desktop windows were left intact; later launches are recorded above.

## Run the demo

```sh
./demos/run-m5-demo.sh
```

This builds the desktop with spatial rendering and opens a fresh editable copy
of `demos/m5-integrated.fold`. The printed temporary path is the working project;
Save As retains it elsewhere. [Demo guide](../../demos/README.md) lists controls,
undo/redo, graph editing and save/reopen steps. Run in a graphical desktop terminal
with Vulkan access. The launcher itself passes shell syntax validation; physical
window interaction is not claimed.

## Read before continuing

1. [Rewrite plan](../Motion_System_Rewrite_Plan.md), which owns gates and scope.
2. [Completion audit](m0-m5-completion-audit.md) and the new
   [catalog continuation evidence](evidence/m5-catalog-completion-result.md).
3. [Catalog](catalog.md), [acceptance specification](acceptance.md), relevant
   proposal sections, root `AGENTS.md` and Fold UI skill before UI changes.

The user did not authorize subagents. Artwork authoring must use public tools
without reading Rust crates. Repository implementation follows root instructions.
Legacy deletions remain intentional; restore historical source through Git only.

## Work added in this continuation

- Four native centered Stroke combinations, retained through observations and
  Screen/Camera Plane rendering. The authored butt/miter mode remains valid.
- Transform with explicit pivot: Screen contours/Outline Text, quaternion metric
  meshes, and uniform-scale/Z-rotation shared instances. The registry and demo
  guide state these boundaries. Screen path coordinates change while Stroke
  width stays in canvas units. Mesh normals use inverse transpose.
- Public Tessellate `planar` and Extrude `closed_profile`, reusing shared
  tessellation/extrusion. Explicit fill rules, metric conversion, normals/UVs,
  named regions and output-vertex-to-cap-vertex correspondence. Generic prepared
  mesh records replace the native box-only wrapper. Converted meshes retain source
  curves/font provenance. Outlined glyphs are separate profiles; this avoids the
  nonconforming combined-glyph cap exposed by the new default-title regression.
- Tests for holes, fill rules, topology/depth correspondence, pivots, upstream
  field sampling, cap/join coverage, native graph starters and rejection/budgets.
- Eight additional public demo catalog trials through actual Source proposals,
  inspection, Apply, observations and optional GPU frames. All restore baseline
  Source. Public word checks cover glyph membership and translated group pivots.
- Current desktop launcher, hands-on guide and updated Fold public workflow.

Primary seams:
`crates/motion/src/{registry,evaluation}/{profile,transform,style}.rs`, native
`evaluation/mesh.rs`, Screen/plane rendering, graph templates,
`crates/app/src/agent/dispatch/observe.rs`, and `demos/m5-integrated.py`.

## Remaining independent implementation and acceptance

M4 still requires picking, aggregate Bevy allocation admission/accounting,
general spatial transparency, rapid native edit presentation, and complete
resource/recovery acceptance. `i_overlay` 4.4.1 still has no cooperative
cancellation inside synchronous Boolean graph operations. Input/output/cache
caps and surrounding checkpoints do not prove intermediate memory or worst-case
cancellation latency. Do not waive that gap or silently move it to M6/M7.

Physical artist continuation, one rendered native/public handoff, and normal/
narrow graph review remain open. The prescribed desktop automation retry again
returned `runtime_unavailable`, `runtime_open_timeout`, then
`runtime_unavailable`. Continue independent implementation if that route remains
unavailable; a host compositor alone does not establish usable automation.

The original catalog continuation rejected direct retained Text and `straight_text`
solids. Its original uniform-instance limitation is superseded by the later affine
parenting/instance evidence; use that evidence for current transform support.

Default Motion conversion budget remains 2,000,000 checkpoints per frame from
prior user authorization. Geometry limits were not raised. Public frame time
remains half-open: last demo frame **95/24**, separate exact-time CPU/native sample
**4**. Never assign older captures to regenerated artifacts without checking.

## Validation

Use:

```sh
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

The [new evidence record](evidence/m5-catalog-completion-result.md) contains the
fresh results, commands, hardware, final checkpoint classification and captures.
Do not infer green workspace status from the earlier checkpoint's three shared
media failures. Native Vulkan and private Unix sockets need access outside this
session's sandbox. Hardware for public frame runs: NVIDIA GTX 1070, driver
580.173.02, Vulkan. Sandbox-only runs may select llvmpipe.

Full raw logs/reports under `/tmp` may disappear; executable tests, the public
authoring script, saved demo and committed evidence summaries remain reproducible.
No phase-completion claim is made by this handoff.
