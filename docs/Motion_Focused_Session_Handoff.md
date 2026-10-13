# Motion replacement: focused session handoff

Updated 2026-10-12. This file owns the next session's working method.

## User instruction

Finish the remaining Motion, 3D and Compositing replacement in a normal session.
Do not create or resume goal mode. The previous goal is paused. Work quickly in
small, complete changes: implement, run a targeted check, launch a release demo,
then use the user's feedback. Preserve existing work. Avoid excessive tests,
builds, infrastructure and status reports. Continue useful independent work
while waiting for feedback. Do not reopen accepted features without evidence.

## Start here

1. Read the current-status table and implementation order in
   [Motion_Context_Replacement.md](Motion_Context_Replacement.md). It owns scope
   and acceptance. Read dated evidence only for the feature you are changing;
   older open items can be superseded by later entries.
2. Inspect the current code for one concrete remaining gap. The suggested next
   review is the separate keyframe curve editor; this is a recommendation, not
   an approved redesign. Establish what is missing before changing it.
3. Implement the smallest complete change, check it, and provide a release demo.
   Keep the user informed with short, plain updates. Use this loop for the
   remaining work instead of starting a large parallel program.
4. Update the plan with implemented behavior, verification and user acceptance
   separately. At completion update the concise HTML report with real evidence.

The old `Motion_System_Rewrite_Plan.md` and architecture proposal named in
AGENTS.md are absent. The current replacement plan supersedes their Motion scope.
[Motion_Replacement_Handoff.md](Motion_Replacement_Handoff.md) contains historical
session evidence; its old goal-resume and launch instructions are superseded here.

## Accepted work: preserve it

The user accepted Motion layout at normal and narrow widths. The latest optimized
canvas Bézier demo is also accepted: **“okay. it feels good.”**

Canvas point input and guides use `dear-implot` 0.18 through the shared UI SDK.
Keep this implementation. The separate animation/keyframe graph was not replaced
or accepted by this canvas review.

The final snap-back cause was in the actual app Session, not ImPlot:
`PreviewExtension` kept an active preview; release dispatched a worker edit;
worker completion rejected the edit because that preview was active. Guides
returned to committed data while the render retained the displaced preview.

The fix retains the preview's command request and commits the validated preview
when release matches its command ID, base revision and arguments. Project revision
guards and Undo remain authoritative. Relevant files:

- `crates/app/src/session.rs`, `session_view.rs`, `session_commands.rs`
- `crates/app/tests/motion_gesture.rs`
- `crates/motion-context/src/ui/path_input.rs`, `viewport.rs`, `state.rs`

The real-session regression failed before the fix and passed after it, including
preview identity and Undo/Redo. Earlier synchronous UI fixtures missed this bug.
For lifecycle bugs, test the real Session rather than adding more shallow tests.

## Remaining scope

Use current code and the plan to turn these open categories into concrete tasks;
this list is not permission to invent additional features.

- Finish remaining path/animation workflows and qualify the keyframe editor.
- Extend image/color support beyond current 8-bit sRGB image input, and complete
  resource accounting beyond Skia's budgeted cache.
- Measure and improve larger-scene frame speed. Canvas user acceptance does not
  qualify every workload.
- Obtain native acceptance for animated 3D parenting and remaining mixed
  Motion/3D/Compositing/Timeline and artist/agent workflows.
- Complete remaining delivery/color cases and HDR display qualification.
- Audit removal of superseded code, dependencies, protocols and guidance; verify
  the final package and required checkpoint checks. Preserve unknown documents
  through generic opaque handling, not legacy fallback engines.

Already implemented evidence includes native paths, text, shapes, groups, scripts,
images and paint; exact-time animation; script conversion; public candidate/frame
review; independent 3D tools and animated parenting; mixed save/reopen; and exact
mixed signed/HDR Float EXR parity in both alpha modes for the tested fixture.
Do not rebuild these from scratch.

## Validation budget

- Format touched files. Use an affected-crate check or one focused test group
  appropriate to the change. A desktop release build can supply compile evidence;
  avoid a redundant check of the same code when it adds no coverage.
- Add a regression test only when it exercises the actual failure or meaningful
  new behavior. No tests for prose, trivial reversible UI edits or implementation
  mirroring. Reuse passing evidence for unchanged behavior.
- Build the optimized desktop once per coherent demo change. Never launch debug
  for user review. Avoid full SDK/Skia/package rebuilds unless their inputs changed
  or the final package checkpoint requires them.
- Run workspace tests/checks/Clippy at the plan's required final checkpoints, not
  each iteration. Report unrelated failures and omitted checks honestly.

Latest useful evidence (do not rerun just to start):

- 10 focused Motion UI tests passed; separate real-session gesture regression
  and compositor preview/cancel/one-Undo test passed.
- Latest desktop release build passed in 16.25 seconds.
- CPU-only preview submission, 120 updates in the saved scene: median 0.120 ms,
  p95 0.153 ms, max 0.245 ms. Excludes rendering, drawing and display latency.
- Earlier workspace run: 575 passed, 2 failed, 70 ignored. The two stale assertions
  were corrected; all 16 tests in the affected groups then passed. Workspace
  all-target desktop check and normal-policy Clippy passed, with Clippy warnings.
  These are historical checkpoint results, not coverage of all later changes.

## Release demo and project safety

The worktree has extensive modified, deleted and untracked rewrite files. Preserve
all of it, including untracked crates, docs and artwork. No reset/clean or unrelated
cleanup. Review only your own changes for complexity and duplication.

Latest saved project: `target/bezier-before-commit-fix.fold`. It was saved through
Fold's public protocol before the fixed release launch. Older backups are in
`target/bezier-before-snapback-trace.fold`, `target/bezier-demo.fold` and
`target/scene-before-bezier-review.fold`. User edits after that save may exist.

At handoff, one release instance was running: PID 3396670, log
`/tmp/fold-motion-commit-desktop.log`, protocol socket
`/tmp/fold-bezier-review/fold-session-JXuGvu/protocol.sock`. These are observations,
not durable IDs. Verify the process and socket before acting. Save current work
through the public protocol before a restart. Close only the identified review
instance; keep one Fold instance open. Native process/socket access may require
sandbox escalation. Temporary files can vanish after reboot.

Build from repository root:

```sh
cargo build -p fold-app --bin fold-desktop --release --features desktop
```

Known working launch (check referenced paths exist first):

```sh
FOLD_MOTION_BRIDGE="$PWD/target/motion-sdk-cache-budget/crates/motion-fframes" \
LD_LIBRARY_PATH="$PWD/target/motion-installed-precision-relocated-20261012/lib" \
"$PWD/target/release/fold-desktop" \
  --protocol-dir /tmp/fold-bezier-review \
  --project "$PWD/target/bezier-before-commit-fix.fold"
```

Keep `--protocol-dir` before `--project`; the opposite order previously opened
Untitled. Verify the running executable and log. The last launch used NVIDIA
GTX 1070 Vulkan and shared GPU rendering. Installed package binaries predate the
latest source fixes; use the freshly built release executable for review.

For public project access, follow `skills/fold/SKILL.md` and verified public
workflow references. `/tmp/fold_bezier_demo.py` is an optional existing helper;
its default socket is stale. Never rely on it surviving a reboot.

## UI and reporting rules

Apply `.agents/skills/fold-ui/SKILL.md` before UI changes. The user strongly rejects
routine notices such as “you are editing.” Controls and artwork give normal edit
feedback. Add a notice only for a useful failure, consequence or required action;
keep the working surface stable. The latest fix added no notice.

Computer use is limited: native screenshots were unavailable through the Linux
tool and the user reports left-click drag does not work. Let the user demo native
interaction; do not claim visual or speed acceptance from automated tests alone.
Use real screenshots when available and useful; never substitute a mockup.

Use concise, plain ASD-STE100-style English. Existing reports:
[Bezier_ImPlot_Report.html](Bezier_ImPlot_Report.html) and
[Motion_Replacement_Report.html](Motion_Replacement_Report.html).
