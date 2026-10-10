# C6 — explicit desktop workspace and viewer state

9 October 2026. Base: C5 `2e540bf`; initial `git status --short` was empty.
Reused C0's deletion inventory and C5's coordination evidence.

## Implementation and ownership

- Each stable viewer ID owns one `RuntimeViewer`: window key, transport widget,
  navigation, viewport/DPI, presentation and review state. Persistent bindings
  remain in Workspace; playback workers remain in Session. Removed all six
  parallel auxiliary viewer maps and the redundant first-viewer window key.
  Editor instances retain their cohesive provider-owned editor/inspector bundle.
- `shell_workspaces` owns instance reconciliation, docking, save/restore, preset
  replacement and close transitions. One runtime reset clears queued selection,
  review actions, cached preview references, dialogs, visibility and presentation
  state even when restored panel IDs are reused. The native integration harness
  uses the same reset. A sidecar load ticket rejects earlier lifetimes even when
  reopening the same project path; save failures cannot finish a pending restore.
- `shell_linking` owns selection, peer navigation, target refresh and linked
  transport routing. Close and selection transitions are callable without an
  ImGui frame. Closed sources reject late selection/navigation; navigation removes
  the prior occurrence link and queued source selection. Unlinked panels cannot
  retain links merely because both carry the Unlinked group value. Viewer overlay
  selection uses the same publication path as editor selection.
- Delivery owns its export form and range refresh in `shell_delivery`; existing
  commands go to Session. `shell_viewer` retains viewer controls, transport and
  provider overlays; image drawing consumes explicit preview/rectangle/sampling
  inputs with no workspace or project authority. Useful panels and widgets remain.
  No visible feedback, public authoring contract, schema, dependency, compatibility
  wrapper, general framework, C7 implementation or M6 work was added.

Ownership rules: keyboard focus identifies the active editor without publishing
selection. Explicit navigation changes following viewers in its group; ordinary
selection updates the group inspector and linked content without taking focus or
publishing a reciprocal event. Pinned viewers own their chosen output and clock;
closing their former editor does not rebind them. Closing an editor pins its
followers to their last output and clears focus, links and queued selection.
Closing a viewer releases its complete runtime record, monitored/inspector
ownership and pending review actions; late callbacks cannot recreate it. Retarget
clears only that viewer's review/presentation/transport widget state. Workspace
replacement creates fresh runtime records for persisted identities.

Session remains the single coordinator. C5 generation guards, cancellation and
admission, transient-edit protection and C4 provider-owned proposal semantics are
unchanged. No discovery/client/Fold skill update is needed because no public
artwork-authoring contract changed.

## Automated verification

The [workspace checkpoint](final-checkpoint.json) passed **792 Rust tests** with
zero failures and 118 ignored native/specialized tests ([full log](final-workspace-tests.log),
[summary](test-summary.json)). Workspace/all-target check, Clippy, headless check,
package boundaries, provider ownership and 16 Python tests passed. Clippy retains
existing warnings, including warnings in relocated code; it is not warning-free.
The initial [checkpoint](checkpoint.json) records a test-link failure when the
filesystem filled, before tests executed. Only reproducible old debug example
and unused feature-configuration test executables were removed; source, fixtures
and evidence were retained. The retry passed.

Explicit native checks passed **nine previously ignored tests**: five public
CLI/MCP frame and mixed-document handoffs ([log](native-frames.log)), one GPU
normal/narrow linked-panel capture ([log](native-layouts.log)), and three preview
texture/pressure/range/BC7 retention tests ([log](native-preview.log)). 109 of the
originally ignored tests remain unqualified; these are not counted as passes.

The native checkpoint then exposed the empty-layout restoration bug described
below. Its minimal regression first terminated with SIGSEGV. After the localized
fix, **122 UI tests** pass, including two new empty/nonempty restore regressions
([log](restoration-ui-tests.log)); these overlap the workspace run and are not
added together. [Restoration checkpoint](restoration-checkpoint.json) records
final workspace/all-target checking, Clippy, focused application integration,
desktop build and repetition of the original reproduction. All passed; the
focused application run passed nine tests (one additional capture remained
ignored in that run and had already passed explicitly). Unchanged full-suite,
headless, provider and native pixel checks were not repeated after this UI-only
restore correction.

Regression coverage includes multiple viewers, closed callbacks, reused panel
identities, source retarget, independent clocks, linked graph/parent selection
without reciprocal publication, closed and rebound sources, unlinked groups,
queued events, same-path sidecar generations, persisted contexts and presets.
Existing normal/narrow UI tests retain toolbar height, playback choices, preview
geometry, provider overlays, docking and context-menu behavior. Rendering tests
and native acceptance are recorded separately below.

An iteration compile caught two docking-test references to the deleted redundant
first-viewer key; both now use the viewer record. The final consumer review also
updated the opt-in selection harness to use the shared replacement reset and
retained a locked inspector when an unrelated editor closes. Local Clippy
findings were resolved by naming sidecar completion state and moving tests after
implementation.

Build commands use offline locked dependencies, two jobs, no incremental/debug
information and serial tests. Native runtime roots point to
`/home/rgame/Documents/fold-native-pipeline/release-package`. No source fixture or
user artwork is overwritten by acceptance runs.

## Native acceptance and incident investigation

The original freeze has no known trigger or crash stack. C0 independently recorded
SIGSEGV in release/debug and with its runtime probe disabled. These remain
separate incidents until causal evidence establishes a relationship.

The native desktop inspection service returned `runtime_unavailable`; its
startup attempt returned `runtime_open_timeout`. An unrestricted capability check
returned the same error ([capture](computer-capabilities.json)). Physical desktop
input and window screenshot acceptance are therefore unavailable through that
service. Offscreen native GPU captures do not certify physical artist interaction or
desktop responsiveness.

Native GPU captures at 1600×1000 and 580×1000 were reviewed for
[Camera](layouts/camera-normal.png), [Light](layouts/light-normal.png) and
[grade](layouts/grade-normal.png), including their
[narrow Camera](layouts/camera-narrow.png), [narrow Light](layouts/light-narrow.png)
and [narrow grade](layouts/grade-narrow.png) variants. Selection and inspector
scope track each other; normal/narrow toolbars retain the same height, with
secondary graph actions in overflow. Inspector controls remain available through
scrolling. Narrow scene graphs fit at a small text scale and long layer names are
clipped by the timeline column; physical zoom/scroll/accessibility interaction
is not certified by these captures. These are native offscreen GPU frames with
injected selection, not a captured operating-system desktop or physical artist
acceptance. Existing viewer geometry/toolbar tests cover normal/narrow viewer
sizes separately.

### Empty-layout restore crash

The original bound-viewer C0 driver reproduced SIGSEGV on the C6 debug desktop
([run](native/run.json)); the original fixture hash remained unchanged. A second
run under `strace -f -k -e trace=none -e signal=SIGSEGV` reproduced it at address
`0x1` in `ImGui::LoadIniSettingsFromMemory`, reached through
`Context::load_ini_settings` ([fault trace](native-fault.trace),
[traced run](native-traced/run.json)). This localizes the bound restoration failure
to layout loading, independently of the renderer/provider hypotheses.

The current dear-imgui-rs 0.18.0 wrapper passes `data.as_ptr(), data.len()`; its
bundled ImGui implementation treats length zero as a request for `strlen`.
Workspace v4 legitimately permits an empty layout. A default Rust String may
supply address `0x1`, causing the C++ read. The restore completion path passed
that empty layout unconditionally; preset application already checked emptiness.

The [minimal production-path regression](empty-layout-red.log) reproduced signal
11 without a project, GPU, preview or rendered frame. One workspace lifecycle
method now treats absent layout as default docking and only passes nonempty INI
to ImGui. Both preset application and sidecar restore use it. Restoring nonempty
INI also clears a pending default-layout reset, so the following frame cannot
replace a restored arrangement. Empty and nonempty controls pass in the final
UI run. No dependency fork, compatibility shim or new visible feedback was added.

The [original bound-viewer replay now passes](native-restored/run.json): public
seed inspection/proposal/apply advanced revision 5 → 6, the process exited **0**,
and the fixture hash stayed unchanged. The passive report recorded 552 redraw
samples, 453 with presented keys, 443 ready samples and two GPU preview records
([summary](native-restored/summary.json), [raw report](native-restored/native.json)).
This is real presentation submission and clean shutdown evidence, not measured
compositor scanout, a performance improvement claim or physical artist acceptance.

**Closed:** the reproducible empty-layout sidecar restore SIGSEGV, with the same
original reproduction red before and green after the fix plus the minimal
regression. **Still open:** the original unknown-trigger freeze and C0's separate
unbound shutdown SIGSEGV; their cause is not established by this bounded finding. All remaining M3–M5/M5a–M5c
artist/agent gates stay open, and M6 remains gated. See the
[Motion continuation](../../../rewrite/evidence/c6-workspace.md).

## Final review

Reviewed the final source diff and all consumers. Superseded maps/orchestration
are removed, lifecycle/delivery state has one owner, drawing retains the existing
widgets, and no unrelated implementation, manifest or public-contract change is
present. Final formatting and diff whitespace checks passed. C6 implementation
and automated verification are recorded; full physical native artist acceptance
remains open. C7 is not started and no Motion gate is waived.
