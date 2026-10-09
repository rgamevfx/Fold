# C5 — focused application coordination

9 October 2026. Base: C4 `7798dde` (C3 `f6e7f8b`, C2 `6f6f161`, C1
`f80804f`). Initial `git status --short` was empty. Reused the C0 deletion
inventory. Implementation and evidence are recorded together in the C5 commit containing
this record.

## Implementation

- Session remains the application coordinator and committed project writer.
  `session_jobs` owns bounded file/ingest/extension/export admission, worker
  inputs, execution and completion. Explicit kinds replace extension/export/file
  flags; Running/Cancelling/Completed define worker lifetime. `session_commands`
  returns a handled/edit outcome to the coordinator's common refresh path.
  `session_agent` owns the dispatcher, endpoint, bounded request/deferred-read
  handling and revision-change outcome. Audio polling lives with transport.
- A single replacement path retires agent handles, cancels preview and file/export
  work, releases monitored audio, clears transient/viewer metadata and increments
  a distinct project generation. Every completion is checked against that
  generation before changing project content, saved roots, paths, serials or
  status. Existing edit revision guards remain. New Project can retire running
  work without waiting on the UI thread.
- Cancelled workers retain admission until acknowledgement. This prevents an old
  save at atomic publication from racing a new save to the same path. A native
  portal dialog can still require dismissal; its cancelled result cannot publish.
  Session shutdown cancels and detaches unfinished workers without giving them
  a writer. Successful ingest teardown preserves proposal cancellation tokens.
- File saves reuse Project's existing checked serialization/atomic publication
  helper through `save_cancellable`. Cancellation before publication preserves
  the existing archive and removes the temporary file. Finalized save/export
  results remain successful despite a late cancellation; project replacement
  still rejects their Session completion. No rollback of finalized files is
  claimed. Open cannot overwrite intervening committed or transient edits;
  extension workers also preserve live artist gestures.
- Dirty/undo/redo refresh is separate from output preparation. Save acknowledgement
  refreshes history only. Output selection is separate from provider metadata
  validation; Session and viewer refresh reuse the existing immutable-root
  metadata cache. Provider validation and content identity semantics remain
  authoritative. No new incremental evaluator/cache framework was added.
- Deleted `media_workflow` and updated every Rust consumer. `output_identity`
  owns selection/dependency identity, `output_scene` owns shared preparation and
  scene evaluation, and `output_export` owns committed-snapshot delivery, exposed
  through the existing `output` module alongside its retained renderer. No old
  module alias or compatibility wrapper remains. The obsolete generic import
  completion/status branch was removed.
- No wire/schema/discovery/skill contract changed. C4 provider preparation,
  reconciliation, protection, inspection receipts and atomic publication remain
  unchanged. No new UI feedback, generic job framework, dependency, C6 or M6 work.

## Verification

Automated verification passed **82 distinct application tests and 20 Project
tests** (including Project's compile-fail doctest). One native GPU frame handoff
remains ignored, not counted as a pass. [Combined application run](focused-tests.log),
[corrected Session test target](agent-session-tests.log), [Project suite](project-tests.log).
Coverage includes cancellation before execution/serialization, late finalized
save/export results, replacement while a worker is running, same-revision stale
completions, dirty saved roots, undo/redo, transient gestures, stale open guards,
provider validation and simultaneous viewer queries, file save and agent work.
Existing viewer tests also exercise independent clocks and export during playback.

The real CLI/socket and MCP handoff tests passed: current discovery, native and
mixed candidates, inspect/apply/cancel, exact retries, parameter expressions,
undo/reopen and committed project copies. These are automated public-tool
handoffs, not physical artist acceptance. Public schemas and Fold skill recipes
remain unchanged because no authoring contract changed.

Affected desktop/all-target and headless/all-target checks passed; affected
App/Project Clippy passed with existing warnings and none in the new modules.
Package boundaries, headless isolation, provider ownership, formatting and diff
whitespace checks passed. See [desktop check](desktop-check.log),
[headless check](headless-check.log), [Clippy](clippy.log),
[boundaries](boundaries.log), [ownership](ownership.log) and [commands](checks.json).
The final test-fixture correction is separately checked by
[focused Clippy](agent-session-clippy.log). The relocated committed-only export
contract is checked by [application doctests](app-doctests.log).

Iteration first caught a cancellation-message expectation and the sandbox's
local-socket restriction ([sandbox run](initial-desktop-tests.log)). Local sockets
were enabled for the public handoffs. A renamed completion variant left one map
call to fix ([compile failure](initial-compile.log)). The combined run then found
that the new simultaneous-activity fixture requested a Browser creation kind
that is intentionally unavailable; it now constructs its initial provider
records directly. Only the affected three-test Session target was rerun after
that fixture correction; all three pass. Unchanged successful targets were not
repeated.

Commands use `--offline --locked`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and serial tests.
Native-runtime tests use `FOLD_COLOR_ROOT` and `FOLD_VIDEO_ROOT` pointing to
`/home/rgame/Documents/fold-native-pipeline/release-package`. C5 is not a broad
workspace checkpoint; the C4 workspace evidence remains historical and was not
rerun or relabeled. Final diff review found no unrelated implementation changes,
compatibility aliases, C6 workspace state changes or M6 evaluator work.

## Acceptance limits

C0 desktop SIGSEGV captures and the earlier desktop freeze remain unresolved.
No physical native interaction, portal dismissal, desktop capture, latency or
allocation acceptance is claimed. Automated native-runtime tests are distinct
from artist acceptance. M3–M5/M5a–M5c gates remain open and M6 remains gated.
See the corresponding [Motion coordination record](../../../rewrite/evidence/c5-coordination.md).
