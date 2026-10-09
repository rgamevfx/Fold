# M5b — geometry attributes and live Rhai Wrangle

Implementation record, 9 October 2026. Implementation is present; this record
separates automated/public evidence from physical artist acceptance. M6 remains
blocked by the rewrite plan's open gates, including the earlier M5a desktop-freeze
report. No freeze fix is claimed here.

## Implemented contracts

- Geometry owns typed Point, Primitive, Corner, Edge, Detail and Instance
  attributes. Mesh `P`/`N` are intrinsic storage, not a second position/normal
  array. `Cd` is float3 scene-linear Rec.709 RGB; `orient` is a normalized
  x/y/z/w World quaternion. Attribute types and domains are checked.
- Grid is triangle mesh Geometry with stable cell point IDs. Copy to Points
  accepts mesh points, consumes `P`, `orient` and `pscale`, and copies a selected
  whitespace-separated attribute list onto shared instances. Instance `Cd`
  reaches rendered material RGB without duplicating the prototype mesh.
- Attribute Create, Attribute Randomize and Attribute Transfer use this model.
  Randomization is seeded by element identity. Transfer is radius-based nearest
  or radial-weighted blend in matching units/space; unmatched elements keep their
  destination value or report an error when no value exists. Categorical values
  require nearest. Topology edits preserve surviving domain values by identity.
- One general Wrangle embeds pinned Rhai **1.26.1** with `serde`, `sync`, and
  `metadata`. It creates/modifies geometry and attributes with optional inputs,
  Detail or element invocation, exact clock components, exposed uniform JSON
  parameters, source/host diagnostics and a closed native function registry.
  Rhai is dynamically typed. Host geometry/attribute contracts are validated.
- Native source/parameter controls use ordinary transactional edits. Source is
  compiled in memory off the UI thread; a bounded 32-entry AST cache is separate
  from time/parameter evaluation. Compilation disables constant folding of host
  geometry calls to avoid capturing another invocation's seed. Evaluation is
  stateless, cancellable and budgeted. No compiler process or application restart
  is involved in artist code edits. Failed output is not partially published.
- Actual registered function signatures are exposed through discovery at
  `catalog.wrangle.functions`. [Agent reference](../../../skills/fold/references/wrangle.md)
  documents the host API, examples, policies and current limits.

Removed the isolated `Type::Points`/`Value::Points` evaluation path. Grid and
Copy to Points now register **version 2 only**; their old version-1 records remain
unsupported and opaque. No runtime migration or executable compatibility alias
was added. Old integrated/control test projects were explicitly re-authored
through inspected public source edits and saved to separate M5b fixture copies;
historical M5 files remain unchanged.

## Public artwork and live editing

The original `/tmp/fold-freeze-diagnosis/M5a recovery copy.fold` was copied before
opening. Only the Background spheres object's graph was edited through public
v2 proposal, candidate inspection, Apply and save-copy operations:

```text
Sphere ───────────────────────────┐
Grid → Attribute Randomize (Cd) → Copy to Points → Soft material → Output
```

[Updated recovery artwork](../../../demos/m5b-background-spheres.fold) contains
44 shared spheres. Existing Sphere/Grid/material node identities and authored
parameters were retained where applicable; the title, backdrop and other scene
objects were outside the edit scope. The original recovery file was not overwritten.
Reviewed [the rendered sphere frame](m5b-background-1.png); frames at 0, 2 and 3
seconds and public candidate/source/save provenance are recorded in
[m5b-background-public.json](m5b-background-public.json).

[Independent Wrangle example](../../../demos/m5b-wrangle.fold) replaces only the
sphere object's construction with an input-free Wrangle and downstream material.
The node generates the grid and spheres, sets `Cd`/`orient`, and animates a wave
from `time`, `params.amplitude` and `params.speed`. Public exact-time frames at
0, 1/2, 2 and 0 seconds verify repeatable seeks. A live amplitude edit changes the
1/2-second frame; restoring it restores identical pixels. A source comment edit
is compiled live and preserves the frame. No app restart/rebuild occurs during
these edits. See [rendered wave](m5b-wrangle-1.png) and
[public evidence](m5b-wrangle-public.json).

Measured in that development session: initial author/inspect/apply 0.673 s;
parameter edit through returned 960×540 public frame 1.689 s; source edit through
inspection/apply 0.693 s. These include protocol round trips and are not a claim
about typical UI latency. An initial test animated depth under an orthographic
camera, producing identical images; it was corrected to animate in the image
plane. No evaluator bug was inferred from that test design error.

Both projects were opened in separate fresh desktop sessions. Canonical object
source and the selected public frame matched exactly after reopening:
[reopen evidence](m5b-reopen.json). The public reproduction script is
[`demos/m5b-background-spheres.py`](../../../demos/m5b-background-spheres.py);
append `wrangle` to author and exercise the programmable variant.

## UI and validation

Reviewed the [normal/narrow offscreen Wrangle inspector](m5b-wrangle-inspector.png):
Run Over, numeric parameters, advanced definitions and multiline source remain
readable without extra toolbar chrome. Capture asserts that drawing does not
mutate authored state. Native computer screenshot capture returned
`runtime_unavailable` because Orca was not running; no live clicks/typing/dragging
were performed. Physical artist interaction/undo/cancel and live Network visual
acceptance are not claimed by an offscreen capture.

Build/test environment:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime
FOLD_COLOR_ROOT=/tmp/fold-ocio-package
FOLD_TEST_COLOR_ROOT=/tmp/fold-ocio-package
```

Native tests/public workflows use the NVIDIA GTX 1070 Vulkan adapter and require
local socket/GPU permissions. Initial sandbox-only socket failures are environment
failures. Initial handoff failures exposed old fixture contracts; refreshed public
fixtures address them. A subsequent concurrent workspace run hit the existing
ImGui `ContextThreadConflict` in `project_panel::tests`; serial verification is
recorded below rather than hiding the failed attempt.

Validation completed:

- `cargo check --workspace --all-features --offline`: pass.
- `cargo test --workspace --all-features --offline --no-fail-fast -- --test-threads=1`:
  734 passed, 2 failed, 117 ignored. The two failures were an overlooked version-1
  Grid test constructor and an observation oracle that still assumed f64 Grid
  storage. Both were corrected, retaining the 1e-12 assertion against explicitly
  f32-rounded mesh coordinates. Their complete affected suites pass below. The
  entire workspace suite was not repeated after those focused fixes.
- Final `cargo test -p fold-motion --all-features --offline -- --test-threads=1 --nocapture`:
  **148 passed, 9 ignored**, including 12 procedural tests. These cover attribute
  schemas/domains, geometry/topology editing, input-free animation, original-input
  element sampling, parameter candidate/undo/redo/save/reopen, AST-cache isolation,
  error/budget behavior and cancellation. The owned-attribute copy regression
  verifies that repeated string payload copying consumes the geometry quota;
  radius transfer charges owned payloads too. Typed cancellation returned after
  **22.282 ms total** for a cancellation requested after 20 ms (test ceiling 2 s).
- Final app `agent_observe` and `agent_handoff`: **22 passed, 1 ignored**. This
  includes the corrected precision oracle and public CLI/MCP save/reopen/undo
  with the refreshed native fixtures.
- App `motion_integrated -- --ignored --test-threads=1`: **2 passed** on native
  Vulkan. Saved integrated frames and the control variation preserve held frames,
  change the expected rendered regions, and retain zero CPU image transfer.
- Wrangle inspector GPU capture: **1 passed**, reviewed above; no authored state
  mutation. Nine ignored Motion captures are not claimed by the default suite.
- Workspace Clippy/all-targets passed. New-code warnings were resolved; final
  Motion/app all-targets Clippy passed with existing warnings in Motion scene/UI,
  app and dependencies. Desktop/CLI binary build passed.
- `cargo fmt --all -- --check`, `git diff --check`, public-script syntax check,
  and `python3 scripts/check_boundaries.py`: pass.

Full workspace output is preserved in [m5b-workspace-tests.txt](m5b-workspace-tests.txt).
The [final validation log](m5b-validation.txt) includes focused test results,
commands and final check/build outcomes. Public workflow checks are independent
of the test counts. Physical acceptance and export delivery remain unverified.

## Remaining acceptance and explicit limits

Physical artist continuation, native source-edit cancellation/undo interaction,
last-valid-preview error presentation, and preview/export parity for the new
Wrangle workflow need dedicated acceptance. Public rendered frames exercise the
shared immutable rendering path, not export delivery or physical presentation.
Persistent simulation, external modules, multiple named Wrangle outputs,
individually keyed JSON parameters, curve topology scripting, mixed-prototype
copying and realization are not qualified by this phase. Normal and arbitrary
attribute transform semantics remain explicit: mesh P/N follow native Transform;
stored attributes are retained unchanged unless a node/program edits them.
Attribute Transfer currently uses bounded CPU radius scans, not a spatial index.
Broader foundational node families stay in their catalog phases.
