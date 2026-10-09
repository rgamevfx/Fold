# Composition selection and grouped panel linking — 9 October 2026

Baseline: `e05f98d`. The user identified two repeatable failures in the M5b recovery
copy: Camera → Light alternated selections continuously, and selecting Explicit
background grade returned to the preceding scene object. The original project
and running application were not modified by the investigation.

## Reproduction and cause

A native Dear ImGui graph-canvas probe emitted eight selection events over eight
idle frames with no input (expected zero, test runtime 0.02 seconds). Motion's
selection contains the owning scene object as well as any graph nodes. The native
canvas only selects graph nodes. Comparing these two representations as though
they were identical produced a new `Select` event on every frame.

A second probe loaded the actual recovery copy and used the registered feature
panels and production Shell. With Composition and Network both visible at usable
sizes, Camera → Light alternated between those rows on all eight subsequent
frames. Selecting Explicit background grade was overwritten by Camera on all
eight frames. Both stability assertions failed. An initial probe with unsuitable
window layout stayed stable; it did not exercise the visible Network path and was
not accepted as a reproduction. The minimized trigger needs both linked surfaces.

The shell published Network's stale selection back to Composition while separately
queuing Composition's graph-opening request. Composition's per-frame `follow_layer`
then reopened the opposite graph, sustaining the feedback loop. Double-clicking
could change the pending navigation and interrupt it. The grade layer was unlocked;
its click was being overwritten, rather than rejected by the layer model.

This establishes the reported selection flashing. It does **not** establish the
cause of the earlier process freeze/crash in [the desktop incident](../m5a-desktop-freeze.md).

## Implementation

- The native graph canvas projects editor selection onto its actual nodes and
  compares selection membership, not incidental ordering. Passive projection does
  not publish a new artist selection.
- `shell_linking.rs` owns selection routing. It consumes explicit selection
  snapshots once; providers resolve a selected item's editable content through
  the shared panel SDK. Composition no longer navigates Network from its draw loop.
- Image-layer selection inspects its placement/animation controls and opens its
  content graph. Scene-object selection inspects the actual object, including its
  output-node parameters, and opens that object's graph. Selecting a graph node
  updates its original Composition occurrence without republishing the selection.
- Network targets are stored per link group. Network, Outliner and Inspector have
  their own group choices; Inspector locking affects only Inspector. Reverse
  routing retains the source editor and layer occurrence instead of consulting
  whichever Composition happens to be visible.
- Linked graph clocks follow the source occurrence's exact local time without
  changing selection or focus. Navigation and viewer/delivery ownership remain
  separate from selection.
- Projected scene-row identities derive deterministically from layer and object
  identity. Links and selections survive separate editor instances and workspace
  save/reopen. Explicit occurrences never fall back to another matching layer.
- Locked Inspector has independent provider editing state. Inspector widget IDs
  include the inspected selection so a reused editor slot does not reuse another
  object's active property widget identity. Scene transform drafts are also scoped
  to their document, object and base revision, and cancel when that scope changes.
- Camera/light object transforms remain accessible in an expandable section; the
  output parameters are visible immediately at both reviewed widths. Nested source
  clocks may be negative before a layer starts and still survive workspace reload.

## Verification

`crates/app/tests/panel_linking.rs` drives the real Shell and registered panels in
native frames. Its fixture defaults to `demos/m5b-background-spheres.fold`;
`FOLD_SELECTION_PROJECT` can point it at the user's recovery copy. It verifies
camera/light/grade stability, correct Inspector/Network targets, normal/narrow
widths, Inspector locking, group isolation, reverse graph selection, repeated
scene occurrences, exact linked clocks, workspace reopen, unchanged project
revision and unchanged delivery output. Additional occurrence data is added only
to an in-memory fixture after the original sequence has passed.

An opt-in GPU capture test renders the same registered panels at both widths.
These are native offscreen frames and injected selection intents, not a claim of
physical desktop interaction or whole-application artist acceptance.

The final recovery-copy integration run passed in 8.22 seconds ([log](recovery-final.log)).
The original failed production-shell trace is retained in [integrated-red.txt](integrated-red.txt).
The final capture run passed in 11.06 seconds on llvmpipe (LLVM 20.1.2), Mesa 26.1.6,
Vulkan ([log](native-captures-final.log)). All six images were visually inspected:

| Selection | Normal panels (~790 px) | Narrow panels (~280 px) |
| --- | --- | --- |
| Camera | [Capture](camera-normal.png) | [Capture](camera-narrow.png) |
| Light | [Capture](light-normal.png) | [Capture](light-narrow.png) |
| Explicit background grade | [Capture](grade-normal.png) | [Capture](grade-narrow.png) |

The matching hierarchy row, content graph and Inspector are visible. Primary
camera/light parameter labels remain readable at the narrow width; graph fitting
reduces node text at that extreme width, with normal graph zoom retained. The
camera property list scrolls vertically. No new toolbar rows or routine selection
notifications were introduced. Physical pointer/keyboard operation, hardware GPU
presentation, and the earlier process crash remain outside this evidence.

Validation environment: offline dependencies, `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`, and
`FOLD_COLOR_ROOT=FOLD_TEST_COLOR_ROOT=/tmp/fold-ocio-package`. The full suite runs
outside the sandbox because its local-agent tests require local sockets.
`LD_LIBRARY_PATH` is unset: the first broad run supplied the packaged codec path,
which broke system FFmpeg's `libavfilter` linkage. All three audio workflow tests
passed after removing that override. This was a validation environment error.

- Workspace all-target/all-feature Clippy completed with existing repository warnings;
  the new lint caused by simplifying the shell conditional was fixed.
- Workspace all-target/all-feature check passed during implementation.
- Formatting, package boundaries/headless isolation and `git diff --check` passed.
- `cargo test --offline --workspace --all-targets --all-features --no-fail-fast -- --test-threads=1`:
  738 passed, 118 ignored, one failed shell fixture ([log](workspace-tests-final.log)).
  That fixture created a Composition binding without a registered provider;
  instance synchronization correctly removed its unavailable shared-panel target.
  The fixture now registers actual test Composition/Graph panel instances and
  reuses the Composition context created by the shell. No production change was
  needed for this failure.
- Final `cargo test --offline -p fold-ui --all-features --lib -- --test-threads=1`:
  **113 passed, 0 failed, 6 ignored** ([log](ui-final.log)), including the corrected
  graph-open fixture and idle scene-selection regression. Together with the full
  run, **739 active tests are verified passing; 118 remain intentionally ignored**.
- Final UI test-fixture Clippy completed with existing warnings
  ([log](ui-clippy-final.log)); workspace Clippy evidence is in
  [workspace-clippy-final.log](workspace-clippy-final.log).

The original recovery file was read only. Additional test occurrences were created
in memory. No release application was replaced or restarted. The earlier freeze/
crash and complete physical workflow acceptance remain open; this evidence does not advance M6.


## User demo review

The updated debug desktop was subsequently launched in Composition view with
`demos/m5b-background-spheres.fold`, using the NVIDIA GeForce GTX 1070 Vulkan
adapter. The temporary recovery copy was no longer present, so the saved demo
containing the same reported selection targets was used. The user reported
“looks good” and requested a commit. This records user approval of the launched
demo; it does not establish completion of every broader M5a acceptance scenario.
