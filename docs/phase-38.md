# Phase 38 — General Oscillator driver

## Delivered design

Oscillator is a normal editable group containing Time, a numeric Oscillator primitive, Group Inputs and Group Output. It drives scalar, vector and color properties. Existing Wave groups and primitives are preserved; the new demo generator and Position Oscillator action use the new asset.

The primary inspector exposes Shape, Minimum, Maximum, timing in seconds or BPM, Cycle duration/Tempo and Phase. A compact normalized cycle preview uses the same sampling function as evaluation. Custom curves allow double-click insertion, point dragging, numeric phase/value editing, deletion of internal points, and smooth or linear segments. Curve settings live in the shared group definition; Make unique separates instances before editing a shared curve.

Advanced controls expose Time scale, Time offset, Phase spread, Channel offset and Strength. The internal Time connection remains editable in the graph. Strength scales variation around the range midpoint; zero strength produces the midpoint. Channel offset is cycles per vector/color component. Phase spread distributes cycles from the first through the last element of a consuming domain. It is distinct from the Duplicator's existing seconds-per-copy Stagger, which retimes upstream animation. A nonzero phase spread requires a consuming element domain and cannot drive global Count directly.

Right-click a numeric property → Add Driver → Oscillator creates and connects the asset in one undoable edit, using property-based ranges. Existing connections are retained. Existing keyframes are not discarded: replacing an animated input reports that its animation must first be removed. Count continues to use the existing numeric rounding semantics; explicit Math rounding remains available.

## Evaluation and compatibility

- Time and sample offsets flow through the existing Time/Field evaluation. Arbitrary frame order, negative time and reverse time scale require no simulation or per-copy graph rebuilding.
- New waveforms: Sine, Triangle, Square, Sawtooth and Custom. Custom curves use bounded normalized points and linear or smoothstep interpolation, not Bézier handles.
- Settings and ports are ordinary graph data. Curve edits retain unknown primitive settings. No project/render infrastructure changes or new dependencies.
- Numeric output sampling uses stack values without per-copy component-vector allocations.
- Research basis: [Cavalry Oscillator](research/cavalry-oscillator.md). This implements Fold's proposed workflow, not Cavalry's undocumented sampling formula.

## Validation and demo

- All 66 Motion tests pass, including duration/BPM equivalence, reverse and delayed time, phase spread, shapes, vector channels, custom-curve serialization, invalid inputs, and atomic driver creation/undo. Existing Wave tests remain green.
- Explicit ImGui/WGPU render test passes. Normal/narrow Oscillator inspectors and custom curve controls reviewed in `/tmp/fold-motion-v2-ui/oscillator-*.ppm`. Live native gesture acceptance remains unverified.
- Release desktop, Badge Wave generator and profiler build successfully. Full workspace checks and Clippy remain deferred; shared UI code was unchanged in this phase.
- Generated `/tmp/fold-badge-oscillator.fold` and 120 preview frames in `/tmp/fold-badge-oscillator-frames`. All 120 frames match the prior modular Wave demo exactly (maximum RGB difference 0). Original project retained.
- GTX 1070 Vulkan verification at 480×270: two frames, maximum CPU/GPU difference 1 color code, three GPU passes and zero CPU adapter nodes. The second frame took 47.358 ms including preparation/readback during concurrent builds; this is a parity check, not a live UI performance benchmark.
- The revised demo retains the same badge distribution, palette, range ±60 and period 1.25 seconds. Select Duplicator → Edit Oscillator, or right-click another numeric property → Add Driver → Oscillator.

- Launched the rebuilt desktop with `/tmp/fold-badge-oscillator.fold`; startup confirms GTX 1070 presentation and shared GPU rendering.
