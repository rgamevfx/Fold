# Phase 24: independent group viewing and inspection

## Goal

Keep a composite visible while inspecting Motion or Timeline in the same group, with one compact viewer header: group/unlinked control and a combined source picker.

## Boundaries and decisions

Workspace/UI routing only; no feature models in shared infrastructure, rendering changes, delivery retargeting, or project mutations. Groups A–D contain at most one editor per contribution/type. Additional editors use vacant groups, then Unlinked. An occupied group is unavailable when moving an editor. Viewer source selection is per viewer, preserving type across group changes; missing types require an explicit choice. Closing/moving a followed editor retains its last output unlinked. All-network choices pin stable document/output references. Inspectors follow explicit selections in their group and can lock the inspected selection. Edit time comes from an associated viewer or the editor's explicit local navigation time when no viewer shows that source; ambiguous multiple clocks still require a viewer choice.

## Tasks

- Replace shared viewer ownership with per-viewer source routing; migrate saved workspaces without changing targets or clocks.
- Add group/unlinked and combined source controls on one row; remove obsolete publication controls.
- Follow group selections independently and lock inspector selection.
- Enforce group occupancy and preserve source/time/closure behavior.
- Add focused model, migration, shell and compact-control checks; inspect normal/narrow layouts.

## Acceptance and verification

Two viewers in A independently show different editor types. Selecting Motion changes the Inspector but not a composite viewer. Locked inspection survives subsequent selection/navigation. Group switching preserves type or explicitly lacks a source. All-network selection unlinks only that viewer; closing an editor preserves its output. Duplicate group/type membership is prevented, including restored layouts. Exact times, independent clocks and delivery targets are preserved. Run affected platform/UI checks and focused tests; record visual evidence and remaining limits here. Full workspace/performance acceptance remains with the integration checkpoints.

## Evidence

Implementation and focused acceptance complete; native interaction limits are recorded below.

## Implementation and validation

- Workspace version 4 stores each viewer's chosen editor contribution separately from the group's latest explicit selection. Contributions provide stable, feature-neutral type identities. Existing v1–v3 layouts retain their viewed documents and clocks; duplicate legacy editor slots move to vacant groups, with excess sources retained unlinked. Inspector ownership migrates with its editor.
- The source picker lists available group networks first and all project outputs below. Group choices follow editor navigation; all-network choices pin document/output references and show the unlink icon. Empty editor slots are omitted. Occupied editor groups are disabled and labeled “in use”; duplicated editors receive a vacant group, then Unlinked.
- Inspector locks retain the selected document and objects through subsequent editor selection/navigation. A matching viewer supplies edit time; otherwise the retained local time is used. Ambiguous matching viewers still require an explicit choice. Editors without a matching viewer can seek their own local time without seeking the compositor viewer.
- Explicit selection requests, including re-clicking an already-selected graph node, update inspection. Focus alone does not. Viewer overlay selections use the same routing. Project mutations, undo, delivery selection and render ownership remain with their existing services.
- Viewer controls stay on one row. Narrow viewers move channels/playback into the overflow control; half/quarter resolution remains visible. Shared drawn icons provide Unlinked and inspector lock states without relying on an icon font.

### Automated evidence

- `cargo test -p fold-platform --lib --offline`: **10 passed**. Covers independent sources, exact clocks, inspection/lock retention, missing types, occupied slots, source closure/movement, stable unlinked outputs, and v1–v3 migration/version-4 round trips.
- `cargo test -p fold-ui --lib --offline`: **58 passed, 2 existing GPU tests ignored**. Includes real ImGui keyboard selection of linked and unlinked sources at 680/240 px, full-header bounds at 680/240/170 px, locked inspector dispatch after editor navigation, explicit local seeks, and repeated graph selection without an authored edit. Existing docking, gestures, animation, transport and selector checks also pass.
- `cargo test -p fold-app --test panel_instances --offline`: **5 passed**. Preserves delivery/history isolation, exact output/time contexts, missing-target behavior and duplicate-name identity. Verifies explicit selection intent even when the selected objects are unchanged.
- `cargo check -p fold-app --features desktop --offline` and the final `cargo build -p fold-app --features desktop --bin fold-desktop --offline` pass. Touched-file formatting and `git diff --check` pass; the diff was reviewed for unrelated edits, obsolete routing and duplicated controls.
- Initial implementation checks caught unsupported ImGui ID argument types, a private test helper reference, and a four-pixel narrow-header overflow. These were corrected. Tests asserting the superseded single-owner routing were replaced with the agreed independent-source/occupied-group behavior.

### Native evidence and limits

Launched the updated desktop with an isolated workspace-state directory and a temporary project containing “Final Composite” and “Title Motion.” The native renderer selected NVIDIA GTX 1070 / Vulkan (driver 580.173.02). Opening Motion from Project retained the compositor viewer. Inspected the normal 1280×684 and narrow 760×684 window: the new viewer header remains one row, source text clips inside its dropdown, preview quality remains visible, and inspector group/lock controls fit. The underlying narrow graph and transport retain existing clipping; this phase does not redesign those surfaces.

- [Normal cross-panel layout](media/phase-24-normal.png)
- [Narrow cross-panel layout](media/phase-24-narrow.png)

Orca initially reported `runtime_unavailable`/`runtime_open_timeout` inside the sandbox. The user-requested retry succeeded outside the sandbox, identifying a sandbox connection restriction. Its Linux provider reports screenshot/accessibility permissions unsupported and cannot enumerate the running Fold process (`app_not_found`). Native captures therefore used the temporary Fold X11 window only. Synthetic native graph/tab input was inconsistent, so full native selection/lock/drag acceptance is not claimed; those behaviors have the focused ImGui/model evidence above. DPI variants, long playback, full workspace suites, Clippy and packaging/performance requalification remain at the existing integration checkpoints. No renderer-throughput claim is made by this UI phase.
