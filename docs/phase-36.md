# Phase 36 — Independent drivers and universal Duplicator inputs

The previous Duplicator bundled a specific Wave and palette into its construction. The user requested separate reusable drivers connected to per-copy inputs, with the same short creation workflow.

## Design

- Duplicator: Source, Copies, Spacing; per-copy X/Y offset, rotation, scale, stagger; optional color input and Preserve text color.
- Wave: independent editable scalar asset with Amplitude, Speed, Phase and Center. **Animate → Position Wave** connects it to Y offset. Connect the same output to X, rotation or another compatible input in Network.
- Palette: independent editable color asset; **Style → Palette across copies** connects it to Color. Palette samples consumer index/count.
- Connected inputs offer named Edit buttons and the driver inspector offers a return button. Per-copy sockets are visible without enabling every parameter socket.
- No effect is hidden inside a bespoke Duplicator evaluator: editable group construction uses Linear Points, Copy to Points, Stagger, transform responses and Apply Copy Color.

## Timing contract

Stagger shifts field sampling by `copy index × delay`. Time fields, Keyframes nodes, and animated node inputs respond to that offset, including across group bindings. Delay seconds are converted to nanosecond rational time, then combined/scaled using checked exact rational arithmetic. The original requested time is retained for geometry evaluation. No sequential simulation or per-copy graph rebuild is introduced.

Saved phase-35 groups keep their embedded graphs and behavior. New Duplicators use the modular construction; old documents are not silently rewritten. Convenience actions preserve an existing connection instead of replacing user wiring.

## Validation and delivery

- All 58 Motion tests pass (one separately run rendering test is ignored by default). Coverage includes delayed keyframes/animated inputs through groups, reusable drivers, disconnected offsets, palette/text preservation, save/reload and undo.
- The explicit ImGui/WGPU rendering test passes. Normal/narrow inspectors, the connected outer network and Wave inspector return navigation were visually reviewed in `/tmp/fold-motion-v2-ui/`. Native mouse/keyboard acceptance remains unverified.
- Release desktop, Badge Wave generator and Motion profiler build successfully. Shared UI's 75 tests passed for the preceding swatch change; no further shared UI changes in this phase. Workspace checks and Clippy remain deferred.
- Generated `/tmp/fold-badge-wave-modular.fold` and 120 preview frames in `/tmp/fold-badge-wave-modular-frames/`; inspected the rendered badge wave. The original phase-35 demo is retained. Reproduce with `cargo run --release -p fold-app --features desktop --example badge_wave -- /tmp/fold-badge-wave-modular.fold /tmp/fold-badge-wave-modular-frames` using fresh output paths.
- GTX 1070 Vulkan verification at 480×270, two frames: maximum CPU/GPU difference one color code, three GPU passes, zero CPU adapter nodes. The second frame took 24.688 ms including preparation/readback; this small parity check is not a live-UI performance benchmark.
- Launched the rebuilt `fold-desktop --project /tmp/fold-badge-wave-modular.fold`; startup reports GTX 1070 presentation and shared GPU rendering.

## Demo workflow

Select Duplicator. Distribution controls count and spacing; Per-copy transform controls offsets, rotation, scale and stagger. **Edit Wave** beside Y offset opens the independent Wave controls, with **Back to Duplicator** for return navigation. **Edit Palette** opens the color driver. Network exposes both actual connections and the unconnected transform inputs for custom wiring. Disconnecting a driver restores the editable base input without deleting the driver.
