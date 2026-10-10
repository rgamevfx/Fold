# C9 — integrated qualification and current ownership

9 October 2026. Base C8 `f8e7abb`; initial `git status --short` was empty.
**Status: implemented; available qualification executed, gate open.** This is not C9
completion or Motion acceptance. Existing user artwork and live sessions are not
modified. Test workflows use temporary copies; no commit is made by this phase run.

## Implementation

- CI installs/runs Clippy with the same desktop feature set as workspace checking.
  It also runs the existing provider ownership check. CUDA helper compilation is
  excluded from ordinary CI because it needs its separate pinned SDK/runtime.
- Extended the existing C1 retirement test to Sequence schema 1 and Compositor
  schemas 1–7, with validation/output rejection and opaque save/reopen checks.
  Existing old-Motion, removed-command and current-operation tests are reused.
- Strengthened the existing C4 source-seam check: recursive modules, aliases and
  grouped imports, comment/literal exclusion, and production after test modules.
  Four focused Python policy tests complement Cargo's existing graph checks.
  The check remains scoped and lexical; no broad keyword ban or new dependency.
- Replaced stale architecture inventory with the current ownership map, reconciled
  product/retirement authority, and reduced the current handoff to one concise
  record linking dated evidence. Reviewed Fold skill, provider-generated schemas,
  public clients/demos and CLI guidance. Public authoring contracts are unchanged;
  no duplicate schema or skill implementation is added.
- [Complete maintenance review](review.md) records C0–C8 dispositions and C9 scope.
  No production Rust/UI behavior, new cache, abstraction framework, dependency or
  M6 execution work is introduced. C4 provider authority, C5 Session coordination,
  C6 panel ownership and C7 completion/resource safety remain intact.

## Verification and provenance

[Environment](environment.json) fingerprints the base, build configuration, runtime
and toolchain. The pinned runtime matches C0. [Checkpoint runner](checkpoint.py)
and [command results](checkpoint.json) retain per-command logs; native execution
uses the [explicit runner](native.py) on the host GTX 1070 rather than sandbox
llvmpipe. Development/test debug information and incremental compilation are off,
two build jobs, offline locked dependencies and serial test execution.

Workspace/all-target checking, headless checking, formatting, boundaries, final
CLI/desktop build and Clippy pass ([checkpoint](checkpoint.json)). Clippy retains
inherited warnings; CI does not claim a warning-free build. The final ownership
check and **20 Python tests** pass ([policy tests](python-final.log)).

The [full workspace run](workspace-tests.log) reports **781 passed, 14 failed,
122 ignored**. Thirteen failures are private Unix-socket `Operation not permitted`
inside the sandbox. One was an incorrect C9 test assertion that unavailable outputs
must disappear; the existing API preserves descriptors with error metadata. The
final test checks that none is executable, while retaining support/validation and
opaque round-trip assertions. No production behavior was changed to satisfy it.

The final host C0 replay passes **54 tests**, including that correction and the
public handoffs. Targeted host replays resolve the remaining socket failures.
Together they cover **795 distinct applicable non-ignored tests passing**; this is
not a claim that the original full-suite invocation passed. Two integrated public
tests overlapped the targeted replay because subprocess output split their result
lines; they are not counted twice. The runner now skips successfully replayed C0
targets directly and hashes each selected executable once.

Explicit host qualification executed **119 ignored tests: 112 pass, seven fail**
after prerequisite corrections. [Final dispositions](native-final.json) map every
selected test to its final log; [selection](native-selection.json) pins executables.
[Inventory](ignored-tests.json) retains all 122 source locations and prerequisites:
98 GPU/offscreen-UI, nine color-runtime, one FFmpeg, 13 helper/fixture/performance
and one physical-audio test. NVIDIA GTX 1070 execution is recorded in
[device evidence](host-vulkan.log), not inferred from sandbox llvmpipe.

Three ignored tests remain explicitly unexecuted: `render_efficiency_profile`
(release timing), `alternating_viewers_preserve_two_source_decode_locality`
(uncontended sustained 30fps timing), and `native_device_clock_seek_and_drift`
(physical audio device/output). Existing C8 measurements are reused; C9 makes no
new performance or physical-audio claim. They are not counted as passes.

Helper qualification initially lacked an explicit helper path; the installed
release helper then rejected the current private protocol. The current helper was
built against the pinned development SDK: all-feature helper check, Clippy, tests
and build pass ([results](helper-results.json), [fingerprints](helper-environment.json)).
A diagnostic attempt with global `LD_LIBRARY_PATH` interfered with system ffprobe;
its failures are environmental. The final [temporary package](helper-package.json)
scopes libraries through the helper's existing relative RPATH, leaves the installed
package untouched and passes all seven helper GPU replays
([runner](helper_replay.py), [results](helper-replay-results.json)).
The fake-helper supervision test passes using the small verified fixture
([log](small-fixture-native-024.log), [fingerprint](small-fixture.json)); source pinning
of the larger 450-frame fixture exhausted its 500ms deadline. No timeout was relaxed.

## Integrated workflows

C0's frozen [workflow/profile](../../c0-workflows.md) is reused. The final host replay
passes 54 tests covering public CLI/MCP authoring, revision guards and inspection,
native command continuation, undo/redo, protected state and save/reopen identity
([log](final-c0-workflows.log)). [Public summary](public-summary.json) records matching
discovery schemas, handoff/samples, unchanged source digest and zero extra undo
groups from retries. Explicit native tests additionally pass public frame jobs,
preview-worker/direct-render agreement and Timeline encoded export. This does not
claim universal Motion preview/export agreement: the spatial cases below fail.

The isolated final-build desktop run passes with revision 5→6 public continuation,
653 heartbeat samples, 555 ready samples and 568 presented samples
([run](desktop/run.json), [summary](desktop/summary.json)). This records presentation
submission, not scanout timing, freeze resolution or physical artist approval.
Six normal/narrow Camera, Light and grade panel captures were visually reviewed:
linked selection is consistent; narrow labels/graphs retain the already recorded
C6 readability limits. No UI change or broader physical usability acceptance is claimed.
[Artifact index](artifact-formats.json) maps raw report/capture names to lossless
JSON gzip/PNG files, with original and retained hashes; snapshots remain preserved.

## Required native failures

All seven remain failed/open with unchanged assertions and tolerances. Retained
pre-maintenance executable controls establish inherited behavior, not permission
to waive C9. Controls are fingerprinted in [C0 control](c0-control.json) and
[color control](color-control.json).

| Test | Current evidence | Inheritance evidence |
| --- | --- | --- |
| `persisted_color_undo_legacy_and_matched_cli_preview_output` | [GPU publication timeout](native-017.log), also isolated | [Pre-maintenance control](color-control-final.log) reproduces the publication timeout; its early CLI prelude uses the current CLI, its failing GPU Session path is the retained binary |
| `animated_cloner_gpu_matches_field_scale_palette_and_registered_output` | [Rotated depth oracle](native-076.log) | [C0 control](c0-control-native-076.log) fails the same oracle at a different first cell |
| `motion_document_spatial_frames_match_ray_oracle_and_retain_snapshot_binding` | [Ray depth](native-081.log) | [C0 control](c0-control-native-081.log), identical values |
| `native_box_gpu_matches_analytic_slabs_and_retains_shared_prototype_frames` | [Box depth](native-083.log) | [C0 control](c0-control-native-083.log), identical values |
| `registered_mixed_motion_aces_preserves_screen_world_order` | [Mixed ACES precision](native-085.log) | [C0 control](c0-control-native-085.log), identical values |
| `registered_mixed_motion_output_preserves_screen_world_order_and_snapshot_frames` | [Mixed precision](native-086.log) | [C0 control](c0-control-native-086.log), identical values |
| `retained_text_mesh_and_extrusion_match_vector_interiors_and_depth` | [Extrusion ray depth](native-111.log) | Identical C7/C6 values, 4.015864849090576 vs 4.01612410445728; [C7 record](../c7/result.md) |

## Gate blockers

- Original unknown-trigger desktop freeze: unresolved; passing automation cannot
  establish causal resolution or native freeze acceptance.
- C0 separate unbound shutdown SIGSEGV: unresolved. C6's empty-layout restore fix
  remains causally verified and is a separate incident.
- Spatial extrusion ray-depth oracle: inherited **failed/open** with unchanged
  tolerance. C7's user-accepted exception does not automatically qualify C9 or M4.
- Physical artist continuation/acceptance: unverified. No automation is relabeled
  as artist input or approval.
- The seven required native failures listed above block C9. Environmental retries
  and inherited controls explain the failures; they do not grant acceptance.

M3–M5/M5a–M5c remain open; M6 stays gated. C8's measured local preparation results
are reused at their original scope: 27–35% decode/validation and 53–55% preparation
improvement for 256 nodes, no cache, no refresh/native-latency claim.
