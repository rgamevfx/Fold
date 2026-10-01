# Phase 8 correction — real graph authoring

The prior list editor is rejected, not an alternative completion of phase 8.
Reference: product sections 4–6, 10, 14–18.

## Design
- Shared UI SDK hosts `dear-node-editor` 0.18 (matching ImGui 0.18). Native editor contexts initialize and die within the ImGui context lifetime. Creative packages use only the SDK.
- The Compositor editor is a spatial canvas: typed pins/wires, pan/zoom, search/context creation, selection, connect/disconnect/delete, framing, and persistent positions. The independent inspector edits only the selected node.
- Authoring node storage and spatial positions do not define evaluation order. Named fixed-schema ports have stable identities; inputs may be disconnected while authoring. Validate types and cycles on edits; compilation topologically sorts and reports missing reachable inputs, never silently bypassing effects.
- Parameter gestures use host-owned transient proposals, evaluated on workers from immutable preview state. Release commits one history entry; Escape cancels. Save/export continue to use committed state. Navigation/undo/stale edits discard overlays safely.
- Viewer navigation records document output, exact local time, and parent breadcrumbs; Open Source maps time and Back restores the parent. Preview keys include focused output and transient content. Root export remains explicitly committed project output.

## Acceptance
- Real native graph visible and usable; no expanded list editor remains.
- Automated graph tests cover arbitrary order, typed links, cycles, disconnected inputs, deletion, positions, and persistence.
- Host tests cover overlay preview/cancel/commit, committed export isolation, stale proposals, exact navigation, and undo.
- Focused native visual inspection; user verifies actual drag interactions on this machine.
- Check affected crates during iteration; workspace tests, Clippy, boundaries, and formatting at the correction checkpoint. Record unverified behavior honestly.

## Implemented correction
- Replaced `ui.rs` list UI with canvas, selected-node inspector, and shared gesture-state modules.
- Added native typed sockets/wires, search/context creation, deletion, selection, framing, arrangement, and saved positions.
- Added feature-owned authoring graph operations with named sockets and topology derived independently of storage order.
- Added host transient proposals and worker preview evaluation without exposing overlays to save/export.
- Added exact source navigation, parent breadcrumbs/time restoration, automatic workspace-tab focus, and direct `--compositor` project opening.
- Excluded graph layout/storage order from render identities.
- Added graph-authoring, host-session, and native UI lifecycle/drawing tests. Workspace tests passed; final refinements passed focused suites and Clippy.

Native visual inspection confirmed the actual graph surface and separate inspector. User testing then identified fractional-trackpad zoom and laptop key-held touchpad suppression. Smooth zoom and background-left-drag panning replaced the problematic controls; Shift-drag selects nodes, with native modifier conflicts handled explicitly. Native input tests cover panning, zoom, wire selection, box selection, and node movement/cancellation.

The user accepted the corrected basic compositor. Final workspace tests, workspace Clippy with warnings denied, formatting, and boundary checks passed; phase 8 is marked complete. This is not a claim that agent-driven physical dragging/rewiring or every manual workflow was verified. The separate sequence/audio/cross-document-undo checkpoint remains pending native acceptance. Detailed scope, constraints, and the inspection recipe are in `phase-8.md`.
