# Phase 39 — Color Ramp and Copy Progress

## Workflow and model

The updated badge demo uses **Copy Progress → Color Ramp → Duplicator Color**. Copy Progress returns normalized element order (first 0, last 1; a single element returns 0). It can be used independently of color. Color Ramp maps any scalar field to a color; it does not know about copies or geometry.

Color Ramp is shipped as an ordinary editable group around a scalar-to-color primitive. Its Value socket stays visible, so an Oscillator, noise or another scalar field can replace Copy Progress. Right-click a color property → Add Driver → Color Ramp creates it transactionally. On Duplicator, this also creates/wires Copy Progress and enables color application. Other color properties start with an editable constant Value. Existing connections and keyframes are not silently replaced.

The inspector reuses the path gradient editor: multiple RGBA stops, drag to reposition, click/right-click for color and numeric position, double-click to add, and removal down to one constant-color stop. A checkerboard reveals opacity. Keyboard navigation can focus stop controls. Linear, Smooth and Stepped interpolation are explicit. Ramp settings belong to the editable group definition; Make unique separates shared instances.

Copy coloring remains a separate application step. Its existing text-preservation control and preserved strokes are unchanged. Legacy Palette and Copy Colors nodes remain loadable and retain their semantics.

## Evaluation

- Finite scalar inputs outside the authored stop range hold the nearest endpoint. Stepped changes at the next stop's exact position; Smooth applies smoothstep within each segment.
- Interpolation operates on straight RGBA in the document working space. HDR RGB values are retained; opacity is restricted to 0–1.
- Evaluation performs binary stop lookup and uses the existing lazy Field sampling/domain/time-offset semantics. No per-copy graph rebuild, extra render pass or new dependency.
- Group signatures now preserve normalized-range and Oscillator timing units. Oscillators created on a 0–1 input default to that range.
- Unknown stop extensions and other node settings survive edits and serialization.

## Validation and delivery

- All 70 Motion tests pass. Coverage includes multiple stops, HDR/alpha interpolation, exact step boundaries, invalid stops, endpoint clamping, Copy Progress, Oscillator-driven color, normalized defaults, serialization, unknown stop data, and undo of wiring/stop edits.
- The explicit ImGui/WGPU render test passes. Normal/narrow ramp inspectors and the outer network were reviewed in `/tmp/fold-motion-v2-ui/color-ramp-*.ppm`. Live native gesture acceptance remains unverified.
- Release desktop, demo generator and profiler build successfully. Workspace checks and Clippy remain deferred; shared UI crate code was unchanged.
- Generated `/tmp/fold-badge-color-ramp.fold` and 120 frames in `/tmp/fold-badge-color-ramp-frames`. All frames match the previous Oscillator demo exactly (maximum RGB difference 0), retaining its endpoint colors and linear interpolation. Earlier demo files are preserved.
- GTX 1070 Vulkan CPU/GPU parity at 480×270: two frames, maximum difference one color code, three GPU passes, zero CPU adapter nodes. Second frame took 22.653 ms including preparation/readback; this is not a live UI latency benchmark.
- Select Duplicator → Edit Color Ramp. To use another input source, wire it to the ramp's Value socket; disconnecting Copy Progress also makes Value directly editable, with Add Driver → Oscillator available.

- Launched the rebuilt desktop with the new project; startup confirms GTX 1070 presentation and shared GPU rendering.
