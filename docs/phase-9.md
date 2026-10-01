# Phase 9 — procedural motion authoring

## Goal

Deliver a real node-based 2D motion workspace, not a preset title generator.
Follow product sections 2, 11–15, 17–18 and the agreed discussion: kinetic text
and radial graphics authored directly, refined through the same editable graph,
and consumed as live sources by the timeline and compositor.

## Agreed scope

38 built-in node types plus graph-group instances:

- Content/scene: Rectangle, Ellipse, Path, Text, Fill, Stroke, Transform, Group, Output.
- Distribution/instances: Line Points, Grid Points, Radial Points, Instance on Points, Realize Instances.
- Context: Position, Index, Element ID, Domain Size, Compare.
- Values/animation: Value, Time, Keyframes, Wave, Math, Combine XY, Separate XY, Map Range, Mix, Random Value.
- Influence: Range Falloff, Radial Falloff.
- Responses: Set Position, Set Rotation, Set Scale, Set Color / Opacity.
- Interfaces: Published Control, Group Input, Group Output; reusable graph-group instances.

Direct tools edit actual graph nodes/connections; arbitrary compatible wiring is
preserved. Native curves, shaped glyphs, instances, stable identities, explicit
field domains, units, spaces, and combine semantics survive evaluation. Published
controls have stable IDs and per-reference overrides. Basic Bézier point editing,
transform/falloff handles, layers, keyframe editing, and a basic curve view are in scope.

## Organization

Keep ownership in `fold-motion`: document, graph, categorized nodes, geometry,
fields, animation, scene, evaluation, authoring, and UI. Colocate node definitions,
typed settings, validation/lowering, and focused tests. Use an explicit registry,
not a global enum containing all node implementations. Reuse geometry algorithms
below nodes and shared UI interactions above them. Render infrastructure receives
feature-neutral immutable drawing work, never motion documents. Preserve the
legacy solid source and unknown extension data. See `graphite-reference.md` for
the primary Rust implementation reference and pinned upstream sources.

## Implementation tasks

1. Establish versioned graph/document model, typed sockets and static node catalog;
   validation, unknown preservation, deterministic explicit-time evaluation.
2. Implement native geometry, shaping/font assets, fields, instances, animation,
   scene output and a bounded vector renderer through the shared engine.
3. Register motion document/video/command capabilities for desktop and headless.
4. Add graph authoring and direct tools, parameter inspectors and animation editing
   through shared UI contracts; all changes use preview sessions/transactions.
5. Add source creation/insertion/navigation and exposed-control overrides to the
   existing cross-feature workflow without feature-to-feature model imports.
6. Test workflows, arbitrary-time evaluation, persistence, cancellation/limits,
   undo and render equivalence; perform native acceptance with the user.

## Acceptance gates

- All listed nodes can be created, wired, grouped, saved/reopened and evaluated;
  type/domain/cycle failures identify the offending node/socket.
- Build kinetic title and radial pattern from an empty document without graph
  wiring, then refine the actual construction through its graph.
- Keyframes use exact times. Sequential and shuffled frame requests agree.
- Text uses real shaping, reproducible font identity and explicit missing-font
  errors. Paths remain native curves; instances remain references until realization.
- Canvas/inspector drags preview live, commit one undo entry, and cancel cleanly.
- Motion output nests in timeline/compositor and renders with alpha in preview,
  desktop export and CLI. Per-reference controls do not modify the source document.
- Global undo, save/reopen and full-quality preview/pre-encode export agree.
- Bounded work, diagnostics, stale-result handling and headless isolation remain.
- Relevant tests pass during iteration; workspace checks/tests/Clippy/boundary checks
  run at the completed slice checkpoint. Native interaction evidence is recorded
  separately from automated tests. No performance claims without measurement.

## Inspector correction (user direction)

Motion nodes use the normal **Node Inspector**, not a separate Motion Inspector
panel. Both compositor and motion register package-owned property implementations
with the shared UI SDK. The SDK creates one Node Inspector window, routes by the
selected document type, and lets workspace navigation focus that same window.
A registry regression test verifies that two implementations create only one tab.

## Shared graph editor correction

Compositor and motion now both use `fold_ui::sdk::graph_canvas::GraphCanvas`.
The implementation in `crates/ui/src/graph_canvas/` owns native editor lifetime
and IDs, socket/wire drawing, event-time background hit testing, continuous
trackpad zoom, pan, multi-selection, Tab/context-menu search, F framing, Arrange,
and movement preview/commit/cancel. The native node-editor backend is private to
that module rather than exposed to feature packages.

Each feature's `ui/graph.rs` implements `GraphContext`: presentation and catalog,
side-effect-free domain validation, and translation of shared events into atomic
feature transactions. These adapters know compositor/motion semantics, not native
editor handles or input gestures. Motion-specific direct authoring and viewer
handles remain in motion; group enter/return controls live in the shared editor.
Each canvas instance has its own view state; changing document/group reframes it.
Returning to a previous group's exact camera/selection is not yet implemented.

The compositor's interaction regression moved alongside the shared editor and
now covers framing, whole/fractional zoom, background/middle pan, wire selection,
box multi-selection, cancelled movement, and one commit per completed drag.
Feature adapter tests cover validation, full selection, atomic failure, movement
undo, and motion group edits preserving the root graph. No visible-desktop
interaction acceptance has been claimed for this refactor.

## Shared Viewer transport

Playback presentation lives once in `crates/ui/src/transport.rs`, at the bottom
of the shared Viewer. The button row contains only icons for In/Out marking,
range-boundary jumps, single-frame steps, play/pause and stop, plus an unlabeled
editable frame number. Enter jumps; Escape cancels. Motion's frame-drag control
and the timeline's duplicate transport buttons/keyboard implementation are removed.
The shell routes shared transport shortcuts for focused Viewer/editor windows.
A thin scrub strip above the buttons spans the full document, with a playhead,
In/Out marks and a highlighted review range. Clicking or dragging uses the same
shared Jump command as frame entry, pausing playback without changing the marks.
Pointer capture continues outside the strip with endpoint clamping; switching
viewed documents does not transfer a held drag to the new source. The Viewer
reserves both transport rows below the image, including on resize.
Preview quality and statistics are available in the Viewer's context menu.

`crates/app/src/session_transport.rs` owns command semantics. Review marks are
session-local and per document, do not alter authored duration or export, and
include the displayed Out frame. Playback holds at Out; Stop returns to In (or
zero). The existing playback worker receives the viewed document and an explicit
exclusive end frame; it uses registered audio capability or its existing silent
clock rather than assuming that every request targets the root timeline.

Focused tests cover commands for motion/compositor/timeline, per-document marks,
exact navigation time, unchanged project data, nested-source playback ending at
Out, and actual ImGui frame typing/Enter/Escape without playback overwriting a
draft. Scrub tests cover click, drag, release, endpoint mapping, unchanged marks,
disabled state and document switching; a Viewer resize test checks image/transport
separation. Native device-audio and visible-desktop acceptance remain separate checks.

## Status and evidence so far

**In progress; not accepted or marked complete.** Existing editing-checkpoint
changes in the working tree predate phase 9 and are not part of this implementation.

Implemented so far:

- Versioned motion documents, static catalog of 38 node types plus group instances,
  typed connections, graph/cycle validation, pure fields and exact-time keyframes.
- Native curves, shaped glyphs with cluster offsets, point distributions, instances,
  explicit realization and ordered vector rendering through the shared engine.
- Versioned bundled Noto Sans resource and license; missing glyph/font errors are
  explicit. Vector rasterization currently uses 8-bit linear color/coverage before
  expansion to the engine's premultiplied RGBA32F working representation.
- Static document/video/command registration; nested sources, reference-local
  published controls, transactional editing and a motion-to-sequence placement command.
- Initial graph UI, direct graph-building tools, shared Node Inspector, base/path/
  falloff handles, keyframe controls and a selected-track curve/dope view. Graph-group
  extraction/editing and publish/animate-input helpers operate on real graph data.

Focused evidence already obtained:

- 10 headless motion tests pass (catalog, typed wiring/cycles, native alpha output,
  shaped text, missing fonts, rational keys, vector math, limits, groups, unknown
  node preservation, and shuffled-time title evaluation).
- 3 app motion-workflow tests pass (reference overrides, compositor/timeline
  nesting, save/reopen, preview cancellation, commit and undo/redo).
- 3 motion UI tests and 2 compositor UI tests pass (transaction lifecycle,
  adapter validation/atomic batches, multi-selection, group editing, undo and
  native ImGui drawing without project mutations). All 16 shared UI tests pass,
  including the migrated native-editor navigation and gesture regressions.
- Shared-inspector regression passes; desktop check and build pass using the
  reference machine's existing external ALSA development package. No local
  dependency paths were added to Cargo configuration.
- Native screenshot `fold-motion/Screenshot_2026-10-01_12-58-40.png` under the user's
  agent-desktop screenshot directory shows the saved kinetic title rendering,
  its real graph, and the single Node Inspector tab. This is passive display
  evidence, not interaction acceptance. The libei click command timed out twice;
  captures were inspected after each timeout and native input testing stopped.
- Inspectable project retained outside the repository at
  `/home/rgame/Documents/Fold-motion-inspection/phase-9-title.fold`, generated with
  `cargo run -p fold-motion --example title -- <new-path>`.

Remaining before acceptance (not silently deferred out of the agreed scope):

- Finish semantic coverage and focused tests for every node combination. In
  particular, field-driven Fill/Stroke, independently driven color/opacity,
  random-vector controls and the complete domain/space rules need further work.
- Finish the property-stack explanation/evaluated-result UI, direct layer/object
  selection and full transform interactions. Exercise curve, group, binding and
  per-reference-control workflows natively. Current controls are an initial
  authoring implementation, not UI acceptance.
- Verify motion-to-sequence placement/undo, actual encoded export, and desktop/CLI
  pre-encode equivalence. Broader shaping/font-environment coverage is unverified.
- Audit geometry/cache work budgets, cancellation and larger repeated constructions;
  persistent static-geometry caching and performance measurements are not complete.
- Run the full slice checkpoint and obtain native workflow acceptance before
  checking off phase 9. No workspace-wide success or performance claim is made yet.
