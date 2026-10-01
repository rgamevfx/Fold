# Phase 8 — Compositor and nested document outputs

## Status
The original list-based editor was rejected: it did not meet the product's node-graph requirement. It has been **removed and replaced**, not retained as an alternative compositor. See `phase-8-correction.md` for the corrective design and acceptance record. The user accepted the corrected basic compositor after native inspection and navigation refinements; phase 8 is complete within the scope documented here. This acceptance does not assert that every manual recipe step below was individually verified. The separate sequence/audio/cross-document-undo checkpoint remains pending native acceptance.

Architecture reference: `Fold_Application_Product_Architecture.docx`, sections 3–6, 10, 14–18.

## Implemented

### Real graph workspace
- Native `dear-node-editor` 0.18 through the shared UI SDK, matching `dear-imgui-rs` 0.18. The SDK initializes native editor contexts and destroys panels before ImGui.
- Spatial nodes with labeled typed sockets, visible curved wires, pan/zoom, box/multi-selection, searchable creation (Tab or context menu), wire creation/replacement/deletion, node deletion, framing (F), and automatic arrangement.
- Left-drag empty canvas to pan; Shift + left-drag empty canvas to box-select. Node/pin dragging and wire clicks retain normal left-button input. MMB panning remains available; Space-pan was removed because holding a key can suppress laptop touchpad movement. The event router hit-tests current pointer coordinates against cached node/pin bounds and curved wires, then latches the gesture through release. Shift is consumed during box selection to avoid the native editor's group-only selection binding.
- Scroll to zoom. Smooth zoom preserves fractional trackpad wheel input (the native default discrete mode truncated it, causing zoom to snap once then stop).
- Viewer above the graph; a separate, full-height **Node Inspector** shows only the selected node's properties. Workspace buttons reveal the matching graph and inspector tabs.
- Named operator sockets have stable identities within the versioned schema; UI-native integer handles are transient mappings from stable node/socket identities. Node positions are saved and participate in undo.
- Graph storage order and canvas positions do not determine execution order. Compilation derives a topological order from connections. Incomplete spare nodes are allowed while authoring; an incomplete reachable output produces a specific diagnostic, never an implicit bypass.
- Numeric controls support dragging and Ctrl-click typing. Parameter changes preview asynchronously from transient immutable state; release/deactivation commits one undo entry. Escape cancels. Node movement similarly commits once on release. Navigation and revision changes discard transient edits safely. Save/export use committed state, not the overlay.

### Operators and evaluation
- First-party `fold-compositor` package: document provider, edit command, static typed operator registry, video compiler, canvas, and inspector. Creative packages do not import each other's implementation models.
- Read (asset or document), Solid, Transform, Crop, Blur, Grade, Mask, ApplyMask, Merge, and Output.
- Read has exact start/source-in/duration; Transform provides translation, scale, and opacity; Crop/Mask use half-open rectangles; Blur is a transparent-border box filter; Grade is scene-linear gain; Merge is premultiplied foreground-over-background.
- Nested capability providers append render IR fragments rather than rendered intermediate frames. The host validates declared dependency closure, provider availability, and output ports; the project coordinator independently rejects document cycles atomically.
- Compositor schema 2 adds disconnected authoring sockets and positions; schema 1 remains readable. Timeline schema 3 adds document video sources; schema 2 remains readable. Load/save without editing does not rewrite opaque payloads.
- Transitive content identities include nested documents/assets. Layout-only changes and node-array reordering do not invalidate rendered content. Snapshot-pinned preview/export share the same evaluation semantics.
- Nested images retain alpha. Standalone compositor delivery uses the existing explicit opaque-black SDR/MP4 profile.

### Composition creation and navigation
- Application-owned Create Composition stages document creation and sequence replacement in one atomic batch. The normal linked A/V selection is accepted: video enters the graph, while audio stays on its track with identical timing/gain and is unlinked. One Undo restores both documents and original links.
- Creation currently accepts video clips on one unlocked track. Locked linked audio, enclosed unselected clips, and unknown selected-video extension data block conversion rather than being lost.
- Open Source from a timeline clip maps the current sequence time into the composite. Open Source from a Read maps through its own exact time range. Viewer keys identify document and exact local time; breadcrumbs and Back restore parent context/time.
- `--compositor PROJECT` loads on a worker and opens the compositing workspace directly. The ordinary `--project PROJECT` entry remains available.
- Playback correction: switching to Timeline via the workspace button or a dock-tab focus change restores its viewer document and saved parent time, rather than leaving the source-view playback restriction active. Timeline Play explicitly activates that context too. Repeated activation of the current workspace does not interrupt playback or edits. Nested compositions remain live provider-evaluated timeline sources; no flattening is introduced.

## Automated verification

Passed again at final acceptance, after the background-pan and selection corrections:
- `cargo test --workspace --all-features` (the opt-in native audio-device test remains ignored).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- Package-boundary/headless-isolation script; formatting; `git diff --check`.
- Desktop check/build using the reference machine's existing external ALSA development files. No machine-specific build configuration was added to the repository.

Focused suites:
- `crates/compositor/tests/graph_authoring.rs`: arbitrary storage order, layout persistence, named typed sockets, atomic cycle rejection, disconnection/deletion, incomplete outputs, legacy payloads/extensions.
- `crates/compositor/src/ui/tests.rs`: native editor context lifecycle, graph/selected-node inspector drawing, wide-graph framing, whole/fractional wheel zoom, background left-pan, physical middle-pan, wire selection, Shift-box selection, node movement with cancellation, and no committed authoring mutations from navigation.
- `crates/compositor/src/ui/navigation.rs`: pointer hit testing excludes nodes/pin edges and curved wires, but allows empty space within a curve's bounding box.
- `crates/ui/src/canvas_pan.rs`: background-only event mapping, gesture latching, Shift-selection modifier consumption/restoration, physical middle-button exclusion, and focus-loss reset.
- `crates/ui/src/shell.rs`: native dock startup regression proving that a workspace request reveals both its editor and inspector tabs.
- `crates/app/tests/compositor_session.rs`: transient preview versus committed roots, cancellation, one-step undo/redo, stale proposals, exact navigation, semantic cache identities, explicit missing-input errors, and asynchronous workspace opening. The playback regression restores timeline context from a nested composite, advances the real silent transport, and verifies a time-dependent nested graph changes the preview worker's output from black to red. This device-independent test does not claim new physical audio-device verification.
- `crates/app/tests/compositor_workflow.rs`: nested timing/alpha, recursive dependencies, unavailable providers, persistence, cache invalidation, locked audio, and atomic cross-document undo.
- `crates/app/tests/compositor_media.rs`: actual MP4 import, linked audio sample preservation, graph edits, save/reopen, and MP4 export with audio.
- `crates/render/tests/compositor_ops.rs`: crop, blur reference comparisons, gain, masks, parameter validation, and IR remapping.

Subsequent canvas/navigation refinements are covered by focused tests and affected-crate Clippy. Final framing and dock-focus regressions were reproduced in native UI tests before fixing them: framing now waits for stable canvas dimensions, and workspace focus waits for initial docking before selecting inspector/editor on separate frames. Focused UI tests, affected-crate Clippy, desktop rebuild, and boundary/diff checks passed afterward. The first workspace run timed out during the initial rebuild; the cached rerun completed successfully.

## Inspection

The retained project and media are outside the repository:

```sh
./target/debug/fold-desktop --compositor /home/rgame/Documents/Fold-phase-8-inspection/phase-8.fold
```

1. Confirm a real graph with Read → Transform → Merge → Grade → Blur → Output and a separate background Solid branch.
2. Select Grade or Blur. Only that node's properties should appear in Node Inspector. Drag or Ctrl-click a value; verify live preview, release, Undo/Redo, and Escape cancellation.
3. Move nodes with left-drag, box-select several with Shift + left-drag on empty canvas, pan by left-dragging empty canvas (or with MMB), zoom with wheel/trackpad scrolling, and frame with F. Verify one Undo per movement gesture and position persistence across save/reopen.
4. Use Tab / **+ Add node** / background context menu to search for a node. New processors have disconnected sockets; wire them into the graph explicitly. Verify incompatible types/cycles are rejected, rewiring replaces only the destination connection, and Delete/Undo work for nodes and wires.
5. Use the timeline inspector's Open Source and a nested Read's Open Source. Verify local time/breadcrumbs and Back restoration. Then return using both the Timeline workspace button and dock tab: the parent playhead/viewer must be restored, Play must advance the nested composition with sequence audio, and no flattened intermediate is required. Source view itself remains scrub-only.
6. Create a composition from selected linked footage; verify audio remains and one Undo restores the original linked clips.
7. Save/reopen and export the committed project output. Confirm that live parameter previews are not saved/exported until committed.

Native visual inspection confirmed the spatial canvas, actual wires, and separate inspector. The final rebuilt application was relaunched with the inspection project; its startup screenshot confirmed the entire graph framed on screen with Node Inspector visible (`Screenshot_2026-10-01_04-34-05.png` in the machine's external `agent-desktop/screenshots/fold-graph` directory). Earlier desktop input attempts timed out in this machine's libei initialization; screenshots were inspected after timeouts and input retries were stopped. The user subsequently inspected the rebuilt application, reported the laptop navigation issues, and accepted the result after fractional-scroll zoom and background-left-drag panning were implemented. Agent-driven physical dragging/rewiring and comprehensive DPI/focus testing are still not claimed as verified; the automated native input tests and user acceptance are distinct evidence.

## Explicit remaining scope

- This is the basic 2D compositor slice, not a claim of full Nuke/Fusion feature parity. Groups/published controls, animation, rotation/pivots, nonrectangular masks, richer grading, copy/paste, and advanced node presentation are not implemented here.
- Image/Mask sockets execute; Scalar/Color/Scene are reserved types, not implemented evaluation streams.
- Composites expose video, not audio. Audio stays in the sequence. Source navigation is scrub-only; root transport remains the synchronized playback path.
- Single-track Create Composition restriction remains. Multi-track conversion needs a deliberate policy for interleaved unselected tracks.
- CPU full-frame execution, nearest-neighbor transforms, scene-linear premultiplied SDR, 512 authoring nodes, 4096 IR nodes, nesting below 64 levels, ten-minute documents, and blur radius at most 64 render pixels. Blur scratch is counted against the working-memory budget.
- No new sustained-frame-rate, long-duration synchronization, accessibility, or DPI acceptance claim is made. Motion work has not started.
