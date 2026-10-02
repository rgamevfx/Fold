# Phase 14 — panel instances and explicit targets

## Goal and boundaries

Introduce stable workspace panel-instance identities, explicit editor document
bindings, group-linked/pinned viewer outputs, and shared compact target navigation. Keep delivery selection and authored undo independent. Reuse provider
editors through the shared SDK. Independent playback, per-consumer cancellation
and audio monitoring remain phase 15; production scheduling remains phase 19.

## Tasks

- Add a feature-neutral workspace model and instance-scoped desktop context.
- Add reusable editor instance creation and context-bound inspector/overlay routing.
- Enumerate provider outputs; keep document selection in editors and link panels through A–D groups.
- Preserve exact nested local time and back navigation; retain missing references.
- Persist versioned instance/binding/layout state outside project content.
- Share the bounded presentation cache across viewer instances.

## Acceptance and verification

Two editors and two viewers can target distinct documents or the same document.
Pin/follow, nested navigation/back, rename/deletion, focus, editor closure and
restore must not cross-wire targets or change delivery/undo. Run focused model,
host and UI tests, affected headless/desktop checks, and native normal/narrow
workflow inspection. Record actual evidence and limits before updating the
completion checkbox.

## Approved link-group refinement

The user approved replacing the viewer's exhaustive target list with a small
letter button and a dropdown containing only A, B, C, D. Editors and the shared
inspector use the same control; A is the default. The editor keeps its document
picker. One explicitly designated editor supplies each group's source; changing
documents/back or choosing "Use as group source" designates it, while focus alone
does not. Changing an editor's group explicitly supplies its new group. Viewers
follow their group by default; pinning is a secondary context-menu action.
Group membership never copies playback clocks or changes delivery. Closing or
moving a source away pins its followers to their last output, with no automatic
replacement editor. The inspector follows its selected group's source, not the
last focused panel.

Implementation tasks: introduce group routing and version-2 sidecar migration;
replace the viewer list and manual direct-tool editor list; add one shared compact
group control; remove routine pending text from the artwork (use a small header
indicator); retain output ports, explicit pinning, exact-time navigation, missing
reference recovery, and the shared cache. Test group isolation, explicit source
ownership, inspector/overlay routing, pinning, closure, migration, independent
viewer time, and normal/narrow mouse/keyboard interaction. Native evidence and
limits must be recorded after verification.

## Implementation

- `crates/platform/src/workspace.rs` owns stable instance IDs, contribution
  associations, editor selection/breadcrumbs, viewer pin/follow bindings, explicit
  inspector association, missing/closed-editor recovery, and version-2 workspace
  serialization with migration of earlier version-1 direct-follow references. Static contribution
  windows also receive separate runtime IDs.
  References use IDs, not labels or menu positions. Duplicate names are qualified
  with bin/type context and deterministic disambiguation.
- `crates/platform/src/panel_context.rs` adapts existing feature APIs to explicit
  document/time/selection contexts. Normal editors never fall back to another
  document. Non-document contribution windows retain host project services with
  their own instance identity. Project-open bootstrap is the intentional legacy
  first-document boundary; normal subsequent navigation is explicit.
- `VideoProvider`, `VideoRegistry`, and `PackageRegistry` expose output-port
  descriptors and resolve explicit references. Preview keys and evaluation carry
  the selected port rather than silently assuming `video`.
- `crates/ui/src/sdk.rs` supplies fresh provider-owned editor/inspector bundles.
  Timeline, compositor and motion reuse their existing authoring and transaction
  logic; they do not duplicate feature models in the shell. The timeline inspector
  participates in the shared inspector. Overlay gestures retain their originating
  viewer and exact time; another viewer cannot continue/cancel them.
- `crates/ui/src/target_selector.rs` retains the compact editor document picker.
  `crates/ui/src/group_selector.rs` supplies the small letter/arrow button shared
  by editors, viewers and inspectors; its dropdown contains only A, B, C, D. The
  source editor's badge is highlighted; its context menu offers "Use as group
  source". Viewer source, quality and exact time are passive header text, not a
  second dropdown. "Pin this output", "Follow group", and compatible ports for
  the current document are secondary context actions. Graph actions share the
  feature toolbar row and use overflow when necessary.
- `crates/ui/src/shell.rs` composes instance lifecycle, target selectors, group-scoped
  inspector contexts, scoped overlays, and explicit delivery controls. New panels
  do not reset existing docking arrangements. Ordinary source changes preserve
  the viewer's rational time and clamp to the new range; explicit nested navigation
  carries mapped time and back restores the saved parent context. Editor seeks
  choose one compatible associated viewer; ambiguous association requires an
  explicit viewer selection rather than changing both or guessing the last update.
- `crates/ui/src/workspace_store.rs` performs bounded/coalesced sidecar I/O on a
  worker. State lives under `$XDG_STATE_HOME/fold/workspaces`, falling back to
  `$HOME/.local/state/fold/workspaces`; it does not enter the project archive or
  undo history. Canonical project paths are checked on restore. Loads validate
  size/version, instance count/identity, navigation depth and presentation values.
  Saves use temporary-file rename; shutdown drains retained requests. Missing
  contributions/documents retain their references and show unavailable state.
- `crates/ui/src/preview.rs` serially services unique content demands through the
  existing worker and **one shared 256 MiB GPU presentation cache**. Rotation
  prevents an advancing first viewer from starving another; pending uploads are
  retained under backpressure. Referenced/in-flight textures remain protected and
  failures are isolated by key. This is not a per-consumer lifecycle implementation
  or a real-time performance claim.
- `crates/app/src/session.rs` supplies output-specific immutable preview metadata
  and validates demand against its requested document rather than a global target.
  Ordinary navigation does not globally cancel another valid demand. Open/save
  establishes the canonical workspace association without changing delivery.

## Original implementation evidence (before link-group refinement)

The original implementation's focused coverage comprised **64 unique passing tests**.
This records the earlier checkpoint; the refinement and its final rerun are below:


| Coverage | Passing tests |
| --- | ---: |
| Platform workspace identity, closure, exact time and restore | 2 |
| Shared UI, docking/drop regression, target mouse/keyboard activation, context isolation, seek routing, source switches, cache adapter, sidecar flush | 28 |
| Compositor feature UI/transaction regression | 2 |
| Motion feature UI/transaction and cross-viewer overlay ownership | 5 |
| Timeline feature UI, gestures and normal/narrow toolbar regression | 3 |
| Provider multi-output enumeration/resolution | 1 |
| App panel instances, compositor session/workflow, motion/NLE workflow, delivery selection, viewer transport | 23 |

Commands used (all locked):

```sh
cargo test -p fold-ui -p fold-platform --lib -- --test-threads=1
cargo test -p fold-compositor -p fold-motion -p fold-timeline -p fold-platform \
  --all-features --lib --test outputs
cargo test -p fold-timeline --all-features --lib -- --test-threads=1
cargo test -p fold-app --all-features \
  --test panel_instances --test compositor_session --test compositor_workflow \
  --test motion_workflow --test nle_workflow \
  --test delivery_selection --test viewer_transport
cargo check -p fold-app --no-default-features
cargo check -p fold-app --all-features
cargo build -p fold-app --features desktop --bin fold-desktop
python3 scripts/check_boundaries.py
git diff --check
```

Desktop/all-feature commands use the documented local ALSA pkg-config path:
`/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig`.
The initial check without it failed because the machine does not provide system
`alsa.pc`; it passed with that environment. Touched Rust files were formatted.
Headless/desktop checks, desktop build, package boundaries and diff checks passed.

The default-parallel timeline UI suite exposes an existing Dear ImGui
`ContextThreadConflict` between its two context-creating tests; the serial suite
passes. The added motion overlay test initially introduced the same conflict with
its existing draw test; both now share a test-only lock and pass in parallel.
A compositor diagnostic regression was fixed to retain the expected missing-output
**port** wording. The selector keyboard test uses real ImGui navigation events
(Tab, Space, Enter), not a forced-focus assumption.

### Original native evidence and limits

Fixture and captures remain outside the repository at
`/home/rgame/Documents/fold-phase-14/`. `navigation.fold` extends the phase-11
fixture with two visibly different motion titles. The isolated workspace restore
contains two motion editors and two viewers: one pinned to Title A, another
following editor 5/Title B. The NVIDIA GTX 1070/Vulkan adapter is recorded in
`native-final.log` (driver 580.173.02).

- `screenshots/Screenshot_2026-10-02_02-20-52.png` (final rebuilt source) and
  `screenshots/Screenshot_2026-10-02_02-10-28.png` show restored separate titles,
  editor instances, and bottom transports at different paused times (23/30 s and
  17/30 s). Narrow approximately 350-pixel panes retain two toolbar rows. These
  are restore/render and visual evidence, not proof of click-driven creation.
- `screenshots/Screenshot_2026-10-02_02-11-20.png` verifies a successful native
  click opening the viewer source selector, including follow-instance choices and
  project-output/open-output entries.
- Subsequent pointer moves timed out (15 seconds). Follow-up captures
  `Screenshot_2026-10-02_02-12-03.png` and
  `Screenshot_2026-10-02_02-13-04.png` showed the selector still open; no choice was
  counted as successful. Only the tested `wdotool --backend libei` route was used.
  No X11/uinput workaround or portal-permission modification was attempted.

Full native click-through duplication, pin/follow selection, nested source/back,
focus, rename/delete, closing and restore remains **unverified**. Automated tests
cover the corresponding state/command behavior; they do not certify the full OS
workflow. No sustained playback, latency, aggregate process-memory, broad DPI or
GPU-evaluation claims are made.

## Link-group refinement verification

The approved revision is implemented. Obsolete viewer follow/open/all-document
lists and the manual direct-tool editor picker were removed rather than retained
as a parallel workflow. Linked viewers derive direct tools from their group's
source; pinned viewers retain an optional explicit compatible editor association.
The inspector is group-routed, independent of window focus. Source ownership and
viewer clocks are separate; joining a group whose source has an old nested
breadcrumb does not copy that editor's time. Explicit nested navigation within
an existing source still maps exact time and back restores it.

The image area no longer renders "Waiting for the current frame…". Routine work
is a small `...` header indicator; no-source groups are quiet and stale images
are not presented as current. Genuine render/dependency failures remain visible.

Version-1 workspace follow references are handled only in
`crates/platform/src/workspace_migration.rs`. They become separate groups in stable
viewer order; repeated followers share a group. Existing pinned references and
exact times are preserved. More than four independent legacy sources are pinned
rather than merged. Version-2 sidecars store editor/viewer/inspector groups and
explicit source owners, outside authored content. `CONTEXT.md` records the
workspace vocabulary without implementation details.

Final focused refinement checks: **51 tests passed** (7 platform workspace tests,
31 shared-UI tests, 5 app panel-instance tests, 7 compositor-session regressions,
1 delivery-selection regression). Coverage includes normal/narrow letter-button
size, actual ImGui mouse and keyboard choice, source competition, A/B isolation,
inspector clock/context, moving/closing a source, pinning, v1/v2 round-trip,
excess-source migration, independent viewer time, exact navigation and history.
Headless app check, desktop build, package-boundary checker and diff checks pass.
UI context tests use `--test-threads=1`; the unchanged parallel timeline conflict
above remains a separate limitation. No workspace-wide/Clippy checkpoint claim.

Native evidence is outside the repository under
`/home/rgame/Documents/fold-phase-14/groups/`. The rebuilt app restored group A's
sequence and group B's motion title with distinct paused times and small shared
letter controls in viewer/editor/inspector headers. The approximately 350-pixel
viewers, narrow motion pane and wide timeline were visually inspected in
`screenshots/Screenshot_2026-10-02_03-22-47.png`. A tested libei pointer move then
timed out (exit 124); click was not executed. Recapture
`Screenshot_2026-10-02_03-25-33.png` showed no choice applied. Native dropdown
selection remains unverified; the software interaction tests are not substituted
for that evidence.

## Status and remaining obligations

**Complete by user acceptance.** After reviewing the completed result, the user
confirmed "looks good" and explicitly requested that phase 14 be marked complete
and committed. `dev-plan.md` records that acceptance. Automated acceptance and
native restore/render evidence are documented above; the full native click-through
workflow is still not independently certified because automated desktop input
stalled. User acceptance does not convert those input failures into passing tests.
Workspace-wide checks, full suites and Clippy remain deferred to the
panel-navigation/viewer slice checkpoint after phase 15.

Phase 15 still owns independent playback clocks, consumer request generations,
cancellation, audio monitoring/handover and bounded teardown. Phase 19 owns
production fairness/admission/cost scheduling and performance measurements. This
phase retains one explicitly owned playback service; additional viewers are
independently targetable and seekable, not independently playing.

Phase 13's native import/drop acceptance and Motion media-input/still-image
placement gaps remain explicitly tracked; this work does not accept them or
convert them into approved deferrals.
