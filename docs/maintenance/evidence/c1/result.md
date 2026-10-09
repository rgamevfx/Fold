# C1 — current document and operation contracts

9 October 2026. Implementation and evidence are recorded together in the C1 commit based on C0 commit `d262659`; C1 implementation and its checkpoint are complete.

## Implementation

- Deleted `timeline/src/video.rs`, both two-layer provider registrations, app import/discovery branches, desktop `--layers`, CLI `import-video`, and the legacy opacity command/state. Media tests now use current Sequence clips/tracks, including an explicit half-opacity overlap with the same linear-light pixel assertion. Generic output, preview, export and content identity remain in their existing module; coordinator restructuring is C5.
- Sequence executes schema 5 only, including active-document selection. Composition always writes and executes schema 3. Implicit output selection skips unsupported providers/schemas; an explicit unavailable selection still fails.
- Compositor discovery and dispatch expose only Grade/Merge/Transform/Blur v2. Removed v1 configurations, registration/defaults, animation controls, inspector branches and lowering. Grade gain and Merge fixtures use current contracts; Transform animation uses current placement controls, and opacity-specific tests were re-authored around Grade gain. Box blur v1 is unsupported; Gaussian test fixtures exercise Gaussian behavior without claiming pixel equivalence. Independently valid shared render primitives remain; no render-engine rewrite or C7 work was performed.
- Removed workspace v1–v3 upgrade code and retired window/panel-name rewrites. Workspace v4 remains supported; invalid/unsupported sidecars restore the current default layout using the existing error presentation. Unknown contributions retain generic unavailable handling. Project content is not involved in this reset.
- Archive v3 is the sole supported container. Versions 1/2 and future versions fail without writes. Removed load-time item migration while retaining transaction-time item discovery. Opaque provider bytes, unknown metadata, assets and archive extensions remain preserved in supported archives.
- No dependency was exclusive to these deletions, so manifests/lockfile are unchanged. No compatibility wrapper, new runtime diagnostics, UI feedback, C2 capability work or M6 execution was added.

## Fixtures and public verification

`Make-it-move.fold` and `m5a-mixed-scene.fold` were re-authored from empty projects through the shipped public scripts. Both use current Grid/Cloner records and Composition schema 3; the mixed scene discovers Grade v2. The attribute and Wrangle fixtures were then re-authored from independent copies of that current mixed scene. Original C0 fixture hashes remain in C0 evidence and originals remain in Git. Historical archives under `docs/` were not rewritten.

Retired the shipped `m5-integrated.fold` v1 fixture. The launcher now opens an editable copy of the already-current `m5b-integrated.fold`; `m5-integrated.py` continues to discover current contracts. Removed the attribute recipe's old-operation interpretation branch. Current recipes and skill guidance describe current schemas/records.

[Public summary and fixture hashes](public-summary.json) links the saved revisions to the four compressed reports:

- [Three-scene workflow](Make-it-move-public.json.gz): public proposal/inspection/Apply, native frame assertions and save, revision 7.
- [Mixed compositor workflow](m5a-mixed-scene-public.json.gz): explicit image connections, Grade v2, scoped source, rendered frame assertions and save, revision 13.
- [Attribute workflow](m5b-background-spheres-public.json.gz): current object source, three rendered times and save, revision 14.
- [Wrangle workflow](m5b-wrangle-public.json.gz): nonsequential seek equality, animated frames, parameter edit/restore and live source edit with retained pixels, revision 17.

The scene recipes' reports retain observation provenance; their original temporary PNG paths are not durable artifacts. The attribute/Wrangle reports retain inline pixels. [Inspected background frame](background-spheres.png) shows the saved scene's current appearance. An initial attribute run against an already-completed fixture correctly returned `NoChange`; the successful run used the base mixed scene as documented. Script wall times are incidental, not C0 performance comparisons.

The temporary authoring host used the existing Session, public transport and GPU host and was removed after verification. It supplied no alternate authoring protocol or runtime migration path. Runtime configuration matches C0 (`FOLD_COLOR_ROOT`, `FOLD_TEST_COLOR_ROOT`, `FOLD_VIDEO_ROOT` point to `/home/rgame/Documents/fold-native-pipeline/release-package`). Public checks used local Unix sockets and the native GPU outside the restricted sandbox.

## Automated checkpoint

Commands use `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `--offline --locked`.

- `cargo check --workspace --all-targets --features fold-app/desktop`: passed ([log](workspace-check.log)).
- `cargo test --workspace --features fold-app/desktop --no-fail-fast -- --test-threads=1`: completed once, initially 755 passed / 2 failed / 117 ignored ([log](workspace-tests.log)). Both failures were test expectations: schema 2 was still asserted readable, and the new fixture test assumed every fixture selected a project-wide output. The tests now assert schema-2 rejection plus current locked-audio behavior, and validate each fixture document through its provider output. The two affected suites then passed all 9 tests ([rerun](focused-rerun.log)); no runtime fix or full-suite repeat was needed.
- `cargo clippy --workspace --all-targets --features fold-app/desktop`: passed with warnings in retained code ([log](clippy.log)). Warning locations in changed files were reviewed; they concern unchanged constructs, not new C1 code. This is not a warning-free claim.
- `python3 scripts/check_boundaries.py`: passed ([log](boundaries.log)).
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 16 passed ([log](python-tests.log)).
- Focused operation tests: 22 passed across graph authoring, native authoring and reusable effects ([log](ops-tests.log)). The initial focused run found the obsolete blur animation channel in a re-authored fixture; it was changed to current Gaussian `size`, then the affected suite passed. Earlier animation/channel tests passed ([initial log](focused.log)).

Three explicitly invoked native GPU tests passed after reopening the current fixtures: static-sphere/transparent-title first frame, preview/public-source-edit agreement, and mixed-document candidate/committed Composition parity ([log](fixture-gpu.log)). Including corrected tests and these three previously ignored tests, **760 test cases pass; 114 other tests remain ignored**, not claimed as passes. [Counts](tests-summary.json) retain the original full-run and rerun results separately.

Formatting, `git diff --check`, launcher syntax and Python demo compilation passed. [Deletion search](deleted-symbols.log) found no removed symbol/entry-point callers in executable source or current public guidance. Final diff review found no unrelated refactoring or duplicated replacement implementation.

New contract tests cover retired provider/schema non-execution and opaque save/reopen, retired operation rejection, current discovery and shipped fixture validation/save/reopen. Project tests reject unsupported containers without changing their bytes; workspace tests reject unsupported generations. Media acceptance is covered by the re-authored video workflow and current ingest/placement/NLE tests in the checkpoint.

## Limits

Physical artist acceptance and desktop normal/narrow visual interaction were not performed for C1. No new UI surface was added. C0's native desktop SIGSEGV captures and original freeze remain unresolved; headless public GPU rendering does not qualify desktop lifecycle behavior. M3–M5/M5a–M5c gates remain open and M6 remains gated. No performance claim is made.
