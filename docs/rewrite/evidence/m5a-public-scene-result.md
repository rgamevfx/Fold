# M5a public scene and sphere continuation — 9 October 2026

Implemented shared proposal lifecycle in `fold-project`; Motion and application
mixed-document candidates use the same bounded generations, inspection receipts,
rebase, expiry, cancellation and atomic Apply. Public v2 now creates Composition,
scene and compositor documents, adds layers, binds explicit layer image inputs,
authors provider records/hierarchy and edits scoped object/definition source.
Scene rebasing uses native graph merges and independent scene properties; new
artist locks still reject publication. Unsupported documents remain opaque.

Added Sphere with smooth normals, stable vertex/face identities and fixed UV-seam
topology, exposed in native discovery and the object creation menu. Radius edits
retain topology; budget/cancellation checks are enforced. No legacy shim is added.

Public frame observation accepts exact named provider output strings and renders
scene/Composition candidates as well as committed documents. Mixed frames hash
the immutable project and explicitly do not claim complete external-resource
fingerprints or a single camera/clock. CPU measurements remain Construction-only.

## Verification completed

- Motion sphere/source/identity/budget test and all five candidate lifecycle tests pass.
- All 12 existing app dispatcher tests and seven new project protocol tests pass.
  New tests cover mixed atomic creation, undo/redo, public save/reopen, stale expected
  records, unlock rejection, object source, disjoint scene rebase, new artist locks,
  discoverable image inputs, explicit layer connections, concurrent dependency
  cycles and removed named outputs. Proposals validate affected consumers as well
  as changed documents; rebasing repeats named-output validation against the new
  immutable snapshot. Failed rebase leaves the original candidate inspectable.
- Native Vulkan: existing candidate/committed frame test and new mixed Composition
  candidate/committed pixel comparison pass (2 tests, NVIDIA GTX 1070).
- Saved demo render test passes: static sphere pixels agree at zero title opacity,
  held title, and reverse seek while a separate Viewer renderer continues scene
  evaluation and GPU submission on the same host at another resolution. All three
  frame tests pass together, serially (39.38 seconds).
- All nine native scene tests pass. A regression first failed when direct scene
  replacement attached a new child under a locked parent; the central replacement
  gate now rejects both insertion and reparenting beneath existing locked ancestry.
- All 16 Motion UI tests pass, including object source, primitive creation,
  auto-key, instance overrides/clocks and normal/narrow offscreen drawing.
- Media regressions pass: four sequence tests, the preview/export NLE workflow,
  and the color/profile test. Sequence expectations now verify linear-space
  resizing; the NLE test releases its unused reference decoder before concurrent
  preview/export; profile coverage accepts supported untagged SDR and explicitly
  rejects PQ. Processing limits were not raised to satisfy these tests.
- Affected desktop/all-target check and package boundary check pass.
- Workspace/all-feature tests passed with local-socket permission: **713 passed,
  115 ignored**, including the previously failing media and protocol workflows.
  [Complete test log](m5a-public-scene/workspace-tests.txt). The initial sandbox run
  could not create protocol sockets; the permission-enabled rerun passed.
- Workspace/all-target/all-feature Clippy completed with existing warnings. Strict
  `-D warnings` stops on unchanged media code (`exr_sequence`, `video_format`,
  `video_stream`, `wave`, and test-module ordering in `budget`). Introduced proposal
  type-complexity and candidate-enum size warnings were fixed. Final all-target
  Clippy and the 19 focused protocol tests cover the lint-only result alias and
  boxed candidate variant added after the full test run.
- Formatting, package boundaries and diff whitespace checks pass. The checkpoint
  validates this continuation, not the unimplemented compositor cutover.

Checkpoint commands used offline Cargo, two build jobs, incremental compilation
disabled and dev/test debug info disabled. Runtime roots were
`FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime` and
`FOLD_COLOR_ROOT=/tmp/fold-ocio-package` (also `FOLD_TEST_COLOR_ROOT` for GPU tests):

```text
cargo test --offline --workspace --all-features --no-fail-fast -- --test-threads=1
cargo test --offline --workspace --all-features --test agent_frames -- --ignored --test-threads=1
cargo test --offline --workspace --all-features --test agent_project --test agent_dispatch -- --test-threads=1
cargo clippy --offline --workspace --all-targets --all-features
```

## Public artwork

`demos/Make-it-move.fold` was created using public CLI discovery → proposal →
inspection → Apply → observation → save. Three scene layers: backdrop (2 nodes),
text reveal (5 nodes), and 44 randomly colored spheres (7 nodes, Copy to Points).
Each scene also has its own one-node camera and light graphs. No exposed inputs or
extra demonstration objects. Composition opacity/position keys reveal the title.
The source workflow is `demos/make-it-move.py` (a complete clean-session run passed and saved revision 7 with start/reveal/held
frames; their static sphere regions each contained 16,073 distinct colors).
Saved public revision 13; save reported durable. Color runtime was provided through
`FOLD_COLOR_ROOT=/tmp/fold-ocio-package`. Screenshots only; no live input.

Reviewed [held frame](m5a-public-scene/held.png) and
[partially revealed frame](m5a-public-scene/reveal.png).
An earlier [zero-time frame](m5a-public-scene/first-start-missing-spheres.png)
omitted the static spheres. Repeated capture initially reproduced the omission;
a resolution change and later direct regression runs did not. Cold-host and reverse
seek and shared Viewer/observation-host tests pass. No speculative renderer fix was made; retain this as an unresolved
intermittent observation issue, not a passing acceptance claim.

## Remaining phase work

Full compositor static model/source/protocol cutover is untouched by this continuation.
The original two-scene/intervening-compositor gate, full animation/instance Inspector
qualification, complete native normal/narrow visual review, final removal audit
and physical artist acceptance remain open. Future cutover changes must repeat
the workspace checkpoint. M5a is not complete.


## Later cutover correction — 9 October 2026

The “missing spheres” interpretation above is withdrawn. The retained
`first-start-missing-spheres.png`, `reveal.png` and `held.png` have byte-identical
sphere regions (x=0..959, y=230..539), with 22,116 distinct RGBA pixels. Their full
frame differences are the expected title reveal. The original filename remains
for provenance; it is not proof of an omission. See the reproducible
[pixel audit](m5a-compositor-cutover/compare-observations.py) and its
[result](m5a-compositor-cutover/observation-audit.json). No renderer fix is claimed.
Six public source-edit/zero-time/held/reverse-seek cycles per fixture, concurrent
Viewer work and independent preview/public pixel comparison pass in the cutover
GPU suite. Any future omission needs a new failing frame with request provenance.
The [cutover record](m5a-compositor-cutover/result.md) supersedes the remaining-work
list above and preserves the distinction from physical artist acceptance.
