# M5a animation and parenting continuation — 9 October 2026

Status: implemented continuation, **not complete M5a acceptance**. The user requested
all six remaining areas. Compositor replacement, public scene/object candidates,
the final public demo and the full acceptance gate remain required, not deferred.

## Implemented

- One visible creation path: **Composition → Add Scene** and Project **Scene**.
  Removed the Add Motion entry, the `kind: motion` application creation branch,
  and the standalone Construction browser-creation entry. Existing native
  Construction resources remain editable through their original typed evaluator;
  no document conversion or new legacy execution path was added. The flat M5
  demo remains an earlier fixture and the demo guide explicitly says so.
- Scene objects accept XYZ dimensionless scale as well as an explicit uniform
  scalar. Composition of parent/child transforms retains a double-precision
  affine matrix, including shear. The rendering adapter supplies the complete
  matrix to Bevy after transform propagation and before visibility checks.
  Ordinary TRS instances retain their TRS representation. Matrix-only edits retain
  prototype meshes. Finite, bounded, orientation-preserving, sufficiently
  conditioned matrices are required; unsupported singular transforms error.
- Moved the coupled-component track adapter from Motion into `fold-graph`.
  Typed keys now retain component Bézier handles. CPU sampling uses the same
  scalar curve semantics as the shared Curve Editor. Key identities, opaque
  metadata and Add/Multiply contributions are retained.
- Quaternion component curves sample spherical interpolation using a shared
  foundation implementation. Edits normalize the coupled quaternion. The
  projection preserves the quaternion branch under reversed time. Quaternion
  tangent/Bezier edits are explicitly unsupported; Linear means spherical
  interpolation, not independent linear interpolation of four components.
- Reversed clocks preserve exact rational key times, handle orientation and both
  sides of hold discontinuities. A zero-rate clock projects a labeled **Paused**
  constant sample. Value edits insert/update the frozen local key without deleting
  other authored keys. There is no inverse clock for timing edits: these explicitly
  require resuming the clock. No approximate timing inversion was introduced.
- Scene curve projections traverse reusable graph occurrences and instance
  overrides, preserving parent-time override sampling and occurrence clocks.
  Conflicting edits to one shared track fail; equal occurrence proposals deduplicate.
- Removed the Inspector's duplicated per-key tables. It shows the sampled value,
  keyed/animated state and an animation menu; Composition retains key/curve editing.
  Scene-transform auto-key and graph-parameter auto-key use preview/commit/cancel.
  Graph auto-key finishes with the gesture as one undo entry.

## Verification

All Cargo runs used offline dependencies, two build jobs, disabled incremental
compilation/debug symbols, and the existing pinned video package where needed:
`FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`.

- Animation/Graph/Composition/Motion suites, with Motion and Composition UI enabled:
  **170 passed, 0 failed, 9 ignored**, including doc-test groups. Log:
  `/tmp/fold-m5a-affected-tests.log`. Covers coupled keys, Bézier sample agreement,
  spherical rotation and reverse branch choice, repeated graph occurrences,
  override clocks, paused samples, parenting, undo and source round trips.
- App `common_composition`, `motion_cutover`, `motion_video`: **18 passed,
  0 failed, 2 ignored**. Log: `/tmp/fold-m5a-app-regressions-final.log`.
  The retired creation-command test now verifies rejection, Scene defaults and
  atomic undo/redo. Existing Construction editing/unlock fixtures are seeded as
  existing project resources instead of invoking the deleted creation affordance.
- Renderer `spatial_gpu`: all eight explicitly exercised GPU tests passed across
  the suite and the color-runtime rerun. The initial suite had seven passes and
  one missing-environment failure (`FOLD_TEST_COLOR_ROOT`). Supplying the existing
  `/tmp/fold-ocio-package` made that ACES test pass. Logs:
  `/tmp/fold-m5a-spatial-regressions.log`, `/tmp/fold-m5a-spatial-aces.log`.
  The new shear oracle compares GPU-transformed geometry pixel-for-pixel with
  explicitly transformed vertices and checks retained meshes on matrix edits.
- Focused auto-key gesture test passed, including one-step undo and cancellation.
  Log: `/tmp/fold-m5a-autokey-tests.log`.
- Workspace/all-target/all-feature check passed after the shared animation model
  changes. Log: `/tmp/fold-m5a-continuation-check.log`.
- Affected-crate all-target/all-feature Clippy completed with existing warnings;
  introduced collapsible-condition warnings were cleaned up. Log:
  `/tmp/fold-m5a-continuation-clippy.log`.
- Final Animation/Graph/Motion library tests passed after UI and quaternion-context
  cleanup. Log: `/tmp/fold-m5a-final-unit-tests.log`.
- The explicit mixed-scene GPU integration test passed: independent background and
  foreground scene outputs, an intervening compositor with explicit input, shared
  scene depth and alpha composition. Log: `/tmp/fold-m5a-mixed-scene-final.log`.
- Eight normal/narrow GPU captures passed. Reviewed the
  [scene panels](m5a-affine-scene-panels.png) and
  [compact animated controls](m5a-compact-animation.png). Labels and animation
  menus remain visible at narrow widths. These captures do not qualify the full
  Composition curve workflow or live artist input.
- Boundary policy passed. Formatting and whitespace review were performed.

Two old negative assertions (reject all Bézier edits; reject rotated nonuniform
instance transforms) were replaced with positive behavioral oracles for the newly
implemented capabilities. They were not simply removed. Other uncovered failures
from the previous broad checkpoint remain recorded in the earlier scene evidence.

## Visual and acceptance limits

Normal/narrow offscreen Inspector and scene captures were exercised. They are
read-only GPU captures, not live computer input or physical artist acceptance.
The existing desktop process was not closed or relaunched. Full Composition
curve/instance navigation and all auto-key combinations still need workflow
qualification. Public candidate transport is still Construction-specific.

The compositor still executes its schema-6 authoritative model. Sharing the
curve adapter does not replace compositor persistence, source or authoring.
The 28-operation inventory remains binding. No full workspace test gate, complete
public mixed-scene demo, removal audit or M5a completion is claimed; M6 stays blocked.
