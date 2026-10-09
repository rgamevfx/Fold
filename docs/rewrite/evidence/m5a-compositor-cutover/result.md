# M5a native compositor and mixed-scene qualification — 9 October 2026

This continuation preserves the uncommitted public-scene work present at session
start and completes its compositor/model/source/protocol replacement. The original
`Make-it-move.fold` stays separate from the new mixed acceptance fixture.

## Implemented behavior and removal

Compositor schema 8 stores shared versioned Node/Parameter/Edge records. All 28
operation/version entries use the registry's native defaults. There is no persisted
`Parameters` enum, old schema reader, old `Add` edit, Parameters-tag protocol alias
or executable fallback. The renamed `OperationConfig` is a disposable validated
kernel/Inspector projection. Domain configurations remain operation-validated;
primitive controls use typed literal/list bindings. Tracks, mask edges, effects
and reusable override controls retain persistent identities and protection.

The [inventory](../../m5a-compositor-inventory.md) accounts for every old authoring
path and retained color/channel/media behavior. Compositor depends on shared
`fold-graph`, never on Motion. Shared reconciliation merges disjoint native
properties; competing values/topology and newly protected controls conflict.
Project candidates remain immutable and require new inspection after rebase.

Network now exposes native compositor Graph/Source views. Source drafts preserve
the last valid graph until guarded Apply, with Reload for discarding a draft.
Public `compositor_source` supports exact expected source and node scopes.
Reusable compositor overrides appear in the common Composition curve service.
Mixed static/animated components retain literal bindings across Inspector projection;
synthetic host curves have stable temporary identities and do not become authored
tracks during a no-op update. Scoped Source rejects duplicate node identities.
Protecting an authored base also protects that control’s animation; protected
effect mix cannot be bypassed by adding a track. Unrelated controls retain their
own protection scope.

Inspector now retains the instance occurrence path when entering reusable Motion
assets. Nested, reverse and paused clocks reach the actual local parameter clock;
a directly opened definition has no fabricated occurrence time. Commits/cancel
reload content without discarding navigation. The new regression first failed
(root time 1 instead of local 1/4), then passed with auto-key and undo/redo.

## Public authored mixed fixture and rendered result

[`demos/m5a-mixed-scene.py`](../../../../demos/m5a-mixed-scene.py) starts from an
empty desktop session, uses v2 discovery/proposal/inspection/Apply only, observes
frames and saves through the public save-copy command. It reads no Rust or project
payloads. The saved [fixture](../../../../demos/m5a-mixed-scene.fold) is revision 13;
[public-result.json](public-result.json) records provenance and its SHA256.

Front-to-back layers are the transparent title scene, an explicit Read → Grade →
Output compositor, and the background 3D scene. Background spheres and a plate
are independently selectable objects sharing their scene's depth and light. The
plate occurs after the spheres in object order but remains behind them in depth.
Each scene owns separate camera/light objects and scoped graphs. The compositor
binds the background's exact named output; stack adjacency creates no dependency.
The script also edits and round-trips a scoped compositor node with stable IDs.

Reviewed [start](start.png), [reveal](reveal.png) and [held](held.png) frames show
the title's alpha reveal above the same lit colored-sphere region and midnight
plate. The GPU acceptance test independently renders native preview and public
observations at identical dimensions/time and compares every RGBA byte. Six
public source-edit → zero/held/reverse-zero cycles run per fixture while a second
Viewer renderer keeps submitting 1920×1080 work. Both the original three-scene
fixture and new mixed fixture pass. Separate tests cover candidate-versus-committed
frames, public scoped rebase, conflict/lock checks, explicit bindings, atomic
undo/redo, saved reopening, parenting and source/graph identity preservation.

## Workflow and native visual review

Actual Dear ImGui/WGPU offscreen captures, reviewed at normal/narrow widths:

- [Scene Outliner, object Network and Inspector](motion-scene.png): shared selection,
  independent camera/light/object rows, compact Graph/Source and overflow controls.
- [Network](motion-network.png), [instance Inspector](motion-instances.png) and
  [animated override/local clock Inspector](motion-override-keys.png): retained
  instance scope, typed controls and exact local timing fields.
- [Compositor Source normal](compositor-source-normal.png) and
  [narrow](compositor-source-narrow.png): Apply/Reload plus scrollable source;
  advanced raw IDs stay inside Source.
- [Compositor Inspector normal](compositor-normal.png) and
  [narrow](compositor-narrow.png): named channel controls remain readable.
- Composition [Track normal](composition-normal.png)/[narrow](composition-narrow.png)
  and [Curve normal](composition-curve-normal.png)/[narrow](composition-curve-narrow.png):
  common mode button, key/curve identity, readable Inspector controls. Narrow plots
  have less horizontal space and long row labels clip within the row; controls and
  plot remain inside the panel without horizontal window overflow.

Synthetic native tests exercise selection, source continuation, Inspector edits,
auto-key, track/curve gestures, protected controls and undo. These are automated
artist-workflow qualification, not human interaction. No live clicking, typing or
dragging occurred. The Orca computer-use runtime returned `runtime_unavailable`,
and `open` timed out; live window screenshot capture was unavailable. Offscreen
native captures and public frame observations were used for visual review.
Physical artist acceptance is explicitly unverified.

## Observation diagnosis

The historical `first-start-missing-spheres.png` was **misclassified**. Its sphere
region is byte-identical to historical reveal/held frames, with 22,116 distinct
RGBA pixels. The full-frame changes are the expected title reveal. The original
file/name remain for provenance. Reproduce with
`python3 docs/rewrite/evidence/m5a-compositor-cutover/compare-observations.py`;
[observation-audit.json](observation-audit.json) records the common SHA256 and zero
differing bytes. No renderer bug or renderer fix is established by that artifact.
Fresh failing pixels and request provenance would be required to reopen an actual
omission diagnosis. Passing tests alone were not used to dismiss the old report.

## Validation environment and checkpoint

All Cargo commands use offline dependencies, `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`, and
`FOLD_COLOR_ROOT=FOLD_TEST_COLOR_ROOT=/tmp/fold-ocio-package`.
Native GPU and local-socket tests run with execution permission outside the sandbox.

- `cargo test -p fold-compositor --all-features --test native_authoring`: 10 passed.
- `cargo test -p fold-motion --all-features ui::tests -- --test-threads=1`: 17 passed.
- `cargo test -p fold-app --all-features --test agent_project --test agent_frames -- --include-ignored --test-threads=1`: 8 public protocol + 4 GPU tests passed.
- `cargo test -p fold-composition -p fold-compositor --all-features --lib normal_and_narrow -- --ignored --test-threads=1`: 2 capture tests passed.
- `cargo test -p fold-motion --all-features --lib ui::presentation_render -- --ignored --test-threads=1`: 8 capture tests passed.
- All-target/all-feature workspace check passed after the shared reconciliation implementation.
- `cargo test --offline --workspace --all-targets --all-features -- --test-threads=1`: **721 passed, 116 ignored, 0 failed** ([log](workspace-tests.log)).
- Final review fixes (mixed literal/track projection, duplicate Source identities and animation protection) subsequently passed the complete compositor suite: **46 passed, 1 ignored**, including all 10 native authoring tests ([log](compositor-tests.log)). These small safety fixes do not change the reviewed unprotected fixture pixels.
- Final `cargo check --offline --workspace --all-targets --all-features` passed ([log](workspace-check.log)).
- Final `cargo clippy --offline --workspace --all-targets --all-features` completed with existing style/dead-code warnings ([log](workspace-clippy.log)); new cutover warnings were corrected.
- `cargo fmt --all -- --check`, `python3 scripts/check_boundaries.py`, Python syntax checks, the removal audit and `git diff --check` passed.
- Final `cargo build --offline --release -p fold-app --features desktop --bin fold-desktop --bin fold-cli` passed ([log](release-build.log)).
- [Checkpoint totals](checkpoint.json) and [public/GPU log](public-gpu-tests.log) preserve the separate automated scopes.

During iteration a default parallel workspace run hit the pre-existing ImGui
`ContextThreadConflict`; the final workspace run uses one test thread. Another
run exposed a new discovery determinism defect: generated native example IDs
changed between CLI and socket discovery. Examples now use deterministic IDs;
the focused three-test transport regression passes. Neither failure is hidden.

M6 is not advanced. Earlier M0–M5 physical, performance and broader rendering
obligations retain their own status; this record does not relabel historical
partial acceptance as completed. M5a's implemented, automated and rendered
workflow evidence is distinct from the unperformed physical artist handoff.

## Normal build launch

Launched the final `target/release/fold-desktop` in the Composition workspace on
`/tmp/fold-m5a-delivery-354ewy2h/M5a mixed scene.fold`, a fresh copy of the saved
fixture. Public discovery reports revision 13, two native scenes, the Composition
and executable compositor schema 8. The normal GPU renderer's public held frame
matches the reviewed `held.png` byte-for-byte. [launch.json](launch.json) records
the process, socket, arguments and pixel hash. Existing user windows were retained.
The CLI requires `--protocol-dir` before `--composition`; only this task's initial
empty launch with reversed flags was replaced. The final process remains running.

The commit containing this record includes the preserved session-start changes,
completed cutover, fixtures, qualification and updated public skill examples.
