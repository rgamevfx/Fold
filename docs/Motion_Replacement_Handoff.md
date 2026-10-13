# Motion replacement handoff — 2026-10-12

> **Next session:** Start with [Motion_Focused_Session_Handoff.md](Motion_Focused_Session_Handoff.md). It supersedes the working method and launch state below. Work in a normal session; do not resume goal mode. The canvas Bézier interaction is now accepted.

## Resumed checkpoint — 2026-10-12

The original `/tmp` package was absent after reboot. New SDK staging completed,
then moved to `target/motion-sdk-relocated`. A fresh script compiled from that
relocated SDK, and native MCP conversion/pixel-parity tests passed. This remains
historical SDK evidence. A later complete package producer run and relocation
checks passed, as recorded below.

The earlier complete package is `target/motion-installed-relocated-20261012`.
All declared file hashes passed. Installed native/script/3D/native-video renders
passed, and the installed desktop MCP frame exactly matches CLI output with the
same explicit sRGB transform. The user accepted the Motion workspace at normal and narrow widths.
The desktop remains open. A later source fix routes native-only GPU documents
to the bundled native worker. Application first evaluation fell from 7.042s to
1.543s; warm median is 15.962ms. The earlier installed package predates that fix.
The 10,000-dot application baseline is 42.482ms median / 45.123ms p95.
The color precision difference is now fixed in a pinned Skia vertex-color patch.
The native-worker regression and direct signed/HDR shader check pass.
`target/motion-skia-precision` holds the verified native archive. SDK staging
now rejects unpatched Skia archives. Use `resources/motion-context/Skia_Precision_Build.md`
for the build commands. A new complete package now includes both fixes at
`target/motion-installed-precision-relocated-20261012`; all 19,582 file hashes and six installed CLI render cases passed. Mixed Motion/3D signed/HDR preview and Float EXR parity passed in
both alpha modes (35.63s). See `resources/motion-context/mixed-hdr-export-evidence.json`.
Broader workflow and HDR display acceptance remain open.

The current plan records added hierarchy duplication, independent 3D resources,
viewer picking and move/rotate/scale gizmos, script image controls, native image
patterns, retained image/font reuse, and public contact-sheet/onion review.
Declared project images and full-tile image patterns now convert to native
objects with exact pixel parity. The public MCP capture workflow passed in
234.66s. The latest SDK is `target/motion-sdk-image-capture`; the latest
workspace/all-target desktop check passed in 5.13s.
Focused input, geometry and native Vulkan/MCP checks passed. Real review images
and the current HTML report are in `docs/Motion_Replacement_Report.html` and its
adjacent assets directory. The report explicitly lists remaining work.

Animated world-preserving 3D parenting is now implemented with exact frozen
parent motion, without baking. Nine focused 3D tests passed, including 137
fractional-time parenting samples, shared authoring, rendering/viewer matrices,
serialization, protection, Undo and normal/narrow viewer input. Native artist
acceptance remains open. The source check and feature-matrix decode fix are
recorded in the plan.
All five slices remain open. Continue from the current-status table in
`Motion_Context_Replacement.md`; the following original handoff is historical.

## Task

Finish the Motion, 3D and Compositing replacement defined in
[Motion_Context_Replacement.md](Motion_Context_Replacement.md). That plan owns
scope, order, acceptance and evidence. All five slices remain open.

The user paused implementation to test the release build. This handoff records
that state. The next session prompt authorizes resuming and completing the work.

Work quickly. Focus on coding and replacement. Use the smallest complete changes.
Run affected-crate checks and a few focused tests at real seams. Reuse valid
evidence. Run required workspace checks at the plan's final checkpoints; avoid
repeated full builds, broad suites and extra test infrastructure during iteration.
Finish the goal, rather than stopping at a plan or partial implementation.

Preserve the large dirty worktree and unrelated artwork. Many new crates and
documents are untracked. They are part of the current work, not disposable files.
Remove superseded code, dependencies, tests and guidance. The final app must have
one implementation, without legacy engines, aliases, shims or fallback paths.
Keep independently valid shared services and generic opaque project storage.

## Start here

1. Read repository instructions and the current status/product contract in the
   replacement plan. The older motion plan paths in AGENTS.md are missing; the
   replacement plan explicitly supersedes their motion scope.
2. Inspect the current diff and the plan's latest evidence. Treat dated entries
   as historical: later entries can close earlier implementation gaps.
3. Identify the remaining acceptance gaps. Complete the next coherent gap with
   focused checks, then update the plan with the result and any limitations.
4. At the final checkpoint, audit replacement/removal, verify the complete app
   workflow, and deliver the HTML report described below.

Before UI changes, apply `.agents/skills/fold-ui/SKILL.md`. For demos and agent
workflows, use `skills/fold/SKILL.md` and its references. Artwork must use public
tools; the agent artwork workflow must not require reading Rust crates.

## Agreed design

- Motion owns 2D vectors, text, images, animation and fframes script objects.
- 3D owns geometry, materials, lights and cameras. Rendering uses Bevy 0.18.1
  behind Fold's rendering adapter. Fold owns documents, evaluation and time.
- Compositing combines image outputs. Timeline arranges outputs.
- Artists and agents edit the same document through guarded transactions and Undo.
- A script is one graph object and exposes artist controls. The graph projects
  document state; it is not a second stored model.
- Native vectors and script output use the same upstream SVG/Skia renderer.
- Convert to editable captures script output at a chosen time. It does not recover
  procedural logic or animation from the script.

fframes is version 1.2.0 at commit
`b644f22cf5be2333e7098caf7ec064c3d36aeb02`. This is a pinned revision, not a
promise to track latest main. Its shader API exists, but Fold shader integration
is not qualified. Shader effects do not replace the editable 3D scene context.

## Implemented and checked at selected seams

- New `motion-canvas`, `motion-context`, `motion-fframes`, `three-d` and
  `gpu-sharing` packages. App assembly registers Motion and independent 3D.
- Schema-4 native objects, stable IDs, hierarchy, script controls, native images,
  solid/linear/radial paint, scalar and path-point animation, guarded edits and Undo.
- Canvas/Pen/path tools, numeric point and handle controls, graph projection,
  hierarchy and inspectors. Basic 3D creation, grouping, parenting, transforms,
  geometry/material and camera/light controls.
- Real MCP candidate inspection, frame observation and Apply at selected seams.
  Public Fold skill guidance includes adapted upstream motion-craft practices.
- Retained fframes workers, direct Rust script compilation, native/shared Vulkan
  output and tested Float RGBA EXR preview/export parity for selected fixtures.
- Old Motion crate and old protocol paths removed. Full retirement audit remains.
- Relocatable SDK version 2: bundled compiler, prebuilt dependency libraries and
  native-only worker. Runtime Cargo compilation was removed. Old SDK v1 is rejected.
- SDK integrity/worker identity, cancellation, recovery and deferred script-library
  verification have focused evidence in the plan.
- Installed-layout CLI renders passed for native and script projects. A current
  optimized desktop was built and launched. The user said it looks good.

These are implementation and seam results. They are not full artist acceptance.

## Remaining work

Use the plan's current status to refine this list before coding:

- Complete vector/path animation and resource workflows; broader image/color
  support, rich paint and resource ownership/accounting/recovery.
- Qualify vector edge quality. Equivalent curve subdivision still changes some
  stroke coverage. Eight-sample GPU coverage improved tested accuracy but is an
  experimental choice, not final quality/performance acceptance.
- Complete 3D artist workflows, including picking/gizmos, animated parenting and
  full object/material/resource workflows where required by the plan.
- Complete public animation review tools and both directions of artist/agent editing.
- Verify native UI at normal and narrow sizes. Automated ImGui input checks do
  not substitute for artist review. Earlier native screenshot automation was
  unavailable on Linux; use available capture tools and state any remaining gap.
- Measure and reduce application startup and frame cost. The last isolated
  10,000-dot warm median was 35.656ms, p95 43.819ms; this is not app FPS.
- Audit the complete installed package producer, native video, color/HDR/alpha,
  preview/export/headless parity, mixed contexts, save/reopen and full removal.
- Resolve checkpoint failures or report confirmed unrelated failures with evidence.

Known checkpoint results: an earlier full run had 581 passes, one failure and
71 ignored tests across 135 groups. The failure was a Compositor registry count
of 24 versus expected 25. Clippy also reported `never_loop` in
`crates/render/src/preparation.rs:99`. Verify current state before attributing
or fixing either issue. Avoid blindly repeating the old full run.

## Latest build and demo

Staged release directory:
`/tmp/fold-motion-installed-cli-relocated-20261012`.
It contains current `fold-desktop`, `fold-cli`, `fold-video-helper`, the SDK,
and staged color/video runtime resources. Temporary artifacts may disappear;
the scripts and repository source remain the reproducible source.

Desktop build used:

```sh
FOLD_MOTION_SDK_RELATIVE=../share/fold/motion-sdk/crates/motion-fframes \
cargo build --release -p fold-app --features desktop --bin fold-desktop --offline
```

Native video helper was rebuilt with `FOLD_VIDEO_ROOT` pointing to
`/home/rgame/Documents/fold-native-pipeline/native-worker/runtime`.
The color runtime was reused from the existing qualified package, not rebuilt.
The full package producer was not qualified by this manual staging.

Demo project: `/tmp/fold-installed-motion-artwork-20261012/script.fold`.
It is a 640x360, five-second, 10,000-dot scripted wave with a Count control.
`native.fold` in that directory is a small native ellipse fixture.

Launch command:

```sh
FOLD_MOTION_CACHE=/tmp/fold-motion-release-demo-cache \
FOLD_RENDER_BACKEND=gpu FOLD_VIDEO_DECODER=cuda-native \
/tmp/fold-motion-installed-cli-relocated-20261012/bin/fold-desktop \
  --protocol-dir /tmp \
  --project /tmp/fold-installed-motion-artwork-20261012/script.fold
```

Latest launch log: `/tmp/fold-motion-release-demo.log`.
It selected NVIDIA GeForce GTX 1070, Vulkan and shared GPU rendering.
The last live tool session was `7909`; a new session must inspect actual processes.
Preserve the user's running app. An earlier detached launch exited; the second
launch was held in a live command session and confirmed running.

Latest socket was `/tmp/fold-session-ZuxHn5/protocol.sock`; discover the current
socket rather than assuming that path still exists.
Codex already has a Fold MCP entry using `/home/rgame/.local/bin/fold-mcp`.
That wrapper selects one live same-user Fold socket and runs
`target/release/fold-cli mcp`. A fresh stdio MCP initialize/discover succeeded.
This chat's existing MCP transport was closed; reconnect Fold MCP before use.
Multiple live app sessions require explicit socket selection.

Installed CLI evidence:
`resources/motion-context/installed-cli-motion-evidence.json`.
Fresh CLI process times were 14.329s native, 14.714s first script, 7.780s script
at another time and 10.073s repeated time. Repeated pixels matched exactly.
These include renderer/color startup and are not warm frame timings.
`FOLD_PROFILE_RENDER` now emits project-load, renderer-init and evaluation timing
from the CLI. Attribution was not completed before the pause.

## Final delivery

Update the replacement plan with final implementation, verification and acceptance
evidence. Review the diff for duplication, unnecessary complexity and unrelated edits.
Claim completion only when the plan's required work is complete. Clearly identify
any external acceptance that still needs the user's action.

Create `docs/Motion_Replacement_Report.html` as the final concise report.
Use simple ASD-STE100-style English: short sentences, concrete verbs and clear terms.
Use plain HTML, readable text and minimal CSS. No decorative layout or filler.
State what changed, how it works, what was checked, and any remaining limits.
Include real screenshots and simple diagrams only when they explain useful behavior.
Store screenshot assets beside the report with relative links. Label unverified
behavior; never present a mockup as a screenshot or a seam test as artist acceptance.
Keep the report brief and provide a link in the final response.

## Latest qualification

The 64-copy 3D fixture passed on the GTX 1070 with one geometry and one material.
Actual render: `docs/Motion_Replacement_Report_assets/three-d-clones.png`.
Openable project: `target/motion-three-d-clones/scene.fold`. Native 3D review and
sustained performance acceptance remain open. Installed precision-package
evidence is in `resources/motion-context/installed-precision-evidence.json`.
Its only source-manifest difference is the qualification example's project-save
line; production source matches.

Native 3D review desktop: process session 35123; socket
`/tmp/fold-precision-review/fold-session-urlH9s/protocol.sock`; log
`/tmp/fold-precision-desktop.log`. The old Motion review desktop remains intact.
User feedback on 3D selection, numeric edits, Undo and normal/narrow layout is
pending. All package/HDR/clone build and render processes are terminal and passed.
Do not repeat them without new changes. The current code supports compatible HoldNext subdivision. Later source work
also supports aligned Bézier subdivision, as described below.

## Retained cache implementation

Added host `ResourceAllowance` and an explicit private-worker `cache_limit`.
The GPU worker holds min(host budget / 8, 64 MiB) until retirement. Skia's
budgeted cache is checked after drawing; over-limit frames fail without output
and the worker releases the renderer. This is not a physical VRAM cap.
Pressure/recovery and native/script image checks passed. Stale/blocked worker
retirement passed (0.89s). Evidence: `resources/motion-context/gpu-cache-pressure-evidence.json`.
The installed precision package predates these changes.

The cache SDK producer completed at `target/motion-sdk-cache-budget`; dependency
build 2m52s. The retained runtime passed 38 frames with a compiler wrapper that
permits `-vV` and rejects compilation. One worker retained its 32 MiB cache
allowance between frames and all host allocations reached zero after runtime
destruction. Evidence: `resources/motion-context/gpu-cache-ownership-evidence.json`.
The native/script image regression now includes one-byte cache pressure and
pixel-exact recovery. All current build/test handles are terminal. Native review
desktop sessions remain open. Next work: remaining resource/image and vector
workflow gaps, native 3D feedback, final audit/checkpoint and package refresh.

## Aligned Bézier subdivision

Added exact subdivision for Bézier tracks with matching interval times and
per-component time-handle fractions, including resolved Auto tangents and
overshoot between equal endpoint values. Focused trajectory/roundtrip/rejection
test passed (0.81s build, 0.16s execution). Evidence:
`resources/motion-context/bezier-subdivision-evidence.json`. Incompatible spans
or timing still reject. Public MCP/native/export qualification remains open.
No new build is running. Installed packages and `motion-sdk-cache-budget` predate
this latest source change; defer their refresh until the next complete checkpoint.

## Current full checkpoint (running)

Session 44803 runs `cargo test --workspace --offline --no-fail-fast --
--test-threads=1`; log `/tmp/fold-motion-current-checkpoint-tests.log`. The last
process check confirmed active compiler/linker processes. Poll this handle; do
not restart on an observation timeout. Boundary and ownership checks finished
successfully, and all 20 policy tests passed. The full suite is not yet qualified.

The current source audit found no retired Motion/Rhai/text dependency in either
lockfile. Retain independent shared vector coverage for Compositor masks/Roto;
Motion never calls that path. This resolves the earlier retained-service decision
without claiming that all non-Motion vector rasterizers are removed. The image
audit confirmed the upstream RGBA8 transport limit. HDR/high-bit-depth image
input needs an explicit upstream format change, not quantization. No image code
was changed in this audit. See `resources/motion-context/current-cutover-evidence.json`.

Checkpoint continuation: session 44803 is still live. Read-only process inspection
confirmed new compiler processes (Cargo PID 3578117); do not restart the run.
A suspected opaque sRGB image conversion defect was disproved by a real worker
probe. Mid-gray linear error is 0.00004019. Evidence is in
`resources/motion-context/image-midgray-evidence.json`; no source fix was needed.
Temporary inspection/probe processes have completed.

## Latest checkpoint result (supersedes running entries above)

Sessions 44803 and 77751 are terminal. The first was deliberately interrupted
for excessive I/O contention; the resumed two-job workspace run completed:
575 passed, two failed, 70 ignored. Log:
`/tmp/fold-motion-current-checkpoint-tests-j2.log`.
Corrected stale transparent Timeline alpha expectations in compositor_workflow
and the old 24-entry operation count (current: 25). No production code changed.
Targeted rerun session 65905 is live; poll it. Log:
`/tmp/fold-motion-checkpoint-fixes.log`. Do not repeat the full suite for these
assertion-only changes. Current contract tests all passed in the full run.


Checkpoint corrections are now verified. Session 65905 failed on a further
stale preview-versus-scene comparison. Corrected the preview expectation to
opaque black-composited output. Session 91865 completed successfully: all 16
tests in both affected groups passed. Log:
`/tmp/fold-motion-checkpoint-fixes-2.log`. The current workspace/all-target desktop
check is running; log `/tmp/fold-motion-current-checkpoint-check.log`.

Current desktop check handle: 2464. It remains live and has reached the Motion,
3D, Timeline and Composition packages. Poll this handle before starting Clippy.
Touched test files pass rustfmt. No image-input implementation changed in this
checkpoint turn.


Desktop check session 2464 is terminal and passed in 59.79s. Current Clippy
session 50961 is live; log `/tmp/fold-motion-current-checkpoint-clippy.log`.
This is workspace/all-target with fold-app/desktop, offline, two jobs, and
normal warning policy. Do not run a second check while this handle is live.

Clippy session 50961 is now terminal: exit 0, with warnings. No build or test
process remains live from this checkpoint. Next work is image/resource support
and the remaining acceptance items, not another unchanged checkpoint run.

## Focused review mode (user request)

The user paused goal mode. Do not resume autonomous replacement work or mark the
goal complete. Work one visible change at a time; the user will demo and give
feedback. Launch optimized release builds for interactive reviews.

Current demo: `target/bezier-demo.fold`, saved through the public Fold protocol.
It preserves the 64-box document and adds Motion document
`53ef1887-b771-40aa-ae49-45637afe2417`, named Bezier demo. One cyan three-point
curve has a four-second eased middle-anchor animation. Numeric controls avoid
the unavailable left-drag input. Current release build session 81677 is running;
log `/tmp/fold-bezier-release-build.log`. The earlier focused MCP test is session
16620, log `/tmp/fold-bezier-mcp.log`. No broader test run is requested.

The earlier MCP test session 16620 is terminal. It reached public parameter
sampling and failed on exact float equality (119.99999999999956 vs 120).
Changed coordinate assertions to the existing geometry tolerance of 1e-8;
this corrected test has not been rerun. No renderer failure or pass is claimed.
The user-facing demo was authored and durably saved through the public protocol.
Wait for release session 81677, then launch `target/release/fold-desktop` with
`target/bezier-demo.fold`, the cache-budget SDK bridge, and packaged native libs.

Release session 81677 passed (optimized, 4m55s). Demo desktop session 57972
is open with `target/bezier-demo.fold`; log `/tmp/fold-bezier-desktop.log`
confirms NVIDIA GTX 1070 Vulkan and shared GPU rendering. Select the Bezier demo
Motion document if the preserved 3D scene is initially active. Await user feedback;
goal mode stays paused.

Corrected the review launch: protocol-dir must precede --project. The original
launch opened Untitled. Saved the old scene to target/scene-before-bezier-review.fold
and closed both identified review processes. Current release demo is session
19819, socket /tmp/fold-bezier-review/fold-session-dNIHd8/protocol.sock.


User rejected the first release Bézier interaction. Current focused task replaces
path interaction and measures actual UI latency, not the full replacement goal.
New `ui/path_input.rs` replaces the old point gesture branch. Ten Motion UI checks
pass. Release session 49317 is live, log `/tmp/fold-path-editor-release.log`, with
`desktop,native-probe`. Existing review session 19819 exited cleanly; its socket
is gone. Do not kill or attempt to save it again. The saved target/bezier-demo.fold
remains available. New passive-probe options keep the window open after recording
and allow a bounded 1–120 second interval. No native speed/artist acceptance yet.

Path input release 49317 passed. Added bounded same-pose edit coalescing to avoid
repeated Motion-worker cold starts; ten mailbox tests passed. Final release
refresh session 97811 is live, log /tmp/fold-path-editor-release-final.log.
Launch it with passive native timing (60s, stay open) and saved bezier-demo.fold.
No other app instance is known live. Do not describe timing as measured yet.

## Current focused review: snap-back diagnosis

The user reports the guide resets both while holding the mouse and on release.
Do not resume the broad goal. Do not claim the path editor is fixed.
The single optimized diagnostic desktop is running from `target/release/fold-desktop`,
project `target/bezier-before-snapback-trace.fold`, socket
`/tmp/fold-bezier-review/fold-session-1XYUJU/protocol.sock`.
The log is `/tmp/fold-snapback-desktop.log`; temporary tagged diagnostics in
`ui/state.rs` and `ui/path_input.rs` report cancellation callers and viewer basis.
The user has been asked to reproduce one snap-back and reply done. Read the log
before further changes. Remove the tagged instrumentation after diagnosis.
The expanded shared-inspector/viewer handle test passes, but does not reproduce
the live failure. Optimized diagnostic build passed in 25.46s.
The previous native timing report is `/tmp/fold-path-editor-timing-v2.json`;
3,388 frames, revision 3 throughout, controls median 13.58ms / p95 16.38ms.

## Current review: ImPlot canvas replacement

The user explicitly rejected further custom pointer work and requested ImPlot.
The Motion canvas anchor/handle input and selected-path guides now use
`dear-implot` 0.18 through the shared UI SDK. The custom path press/drag
controller is removed. Document edits, coupling and Undo remain in Fold.
The separate keyframe graph is unchanged.
Temporary cancellation instrumentation is removed; the rejected native drag
produced no cancellation entries and did not establish the reset cause.
All 10 focused Motion UI tests pass (final run 7.87s). The optimized desktop
build passed in 3m 50s, including first-time native ImPlot compilation.
The previous desktop had exited; no process was killed for this launch.
The new release runs with `target/bezier-before-snapback-trace.fold`, log
`/tmp/fold-implot-desktop.log`; get its public socket from that log.
The attempted extra save to `target/bezier-before-implot.fold` failed because
the prior app was closed; that file was not created.
Native alignment and performance acceptance remain open. Do not claim them
from the tests. The broad goal remains paused.
Concise report: `docs/Bezier_ImPlot_Report.html`.

## 2026-10-12: real-session path release fix

The user screenshot shows reset orange guides and a displaced cyan render.
The real Session reproduced the cause: a worker edit completion was rejected
because the gesture's own preview was active. The UI then returned to committed
geometry while rendering retained the preview. The previous synchronous UI
fixture did not exercise this lifecycle.

Release now commits the validated preview when command ID, base revision and
arguments match exactly. Project revision guards and Undo remain authoritative.
`motion_gesture::released_motion_preview_commits_the_displayed_curve_once`
failed before the fix with “Finish the current edit, then retry import or relink”
and passed after it, including preview content identity and Undo/Redo.
The existing compositor preview/cancel/one-Undo integration test also passes.
The optimized desktop build passed in 16.25 seconds.

A release CPU profile of 120 preview submissions in the saved review scene
measured 0.120 ms median, 0.153 ms p95 and 0.245 ms maximum. This excludes
rendering, UI drawing and display latency; it is not native speed acceptance.
Current viewer messages do not resize the canvas. No routine editing banner
was found on the Motion path; no new notification was added.
Native drag/release alignment and responsiveness remain pending user review.
The broad replacement goal remains paused.

## 2026-10-12: native canvas Bézier interaction accepted

After the real-session release fix and optimized desktop relaunch, the user
reported: “okay. it feels good.” This accepts the demonstrated canvas Bézier
interaction. It does not qualify the separate keyframe graph, other native
workflows or larger-scene performance. The broad goal remains paused.
