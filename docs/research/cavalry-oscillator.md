# Cavalry Oscillator reference

Research date: 2026-10-05. Primary documentation only; no Cavalry application testing. This is design research, not an implementation specification.

## Confirmed capabilities

Cavalry's Oscillator both drives values and deforms shapes. It offers sine/cosine/tangent types; smooth, square, triangle, sawtooth, and custom graph styles; minimum/maximum and value offset; separate X/Y channels; frequency in cycles per second or BPM; connected composition time, time offset, and time scale. It includes duplicate staggering and strength/falloff behavior. Deformation adds normals and spatial wave count. Its looping example starts by right-clicking Position and adding Oscillator to Y, then connects FPS divided by desired loop frames to frequency. [Oscillator documentation](https://cavalry.studio/docs/nodes/behaviours/oscillator/)

The documentation does **not** specify Oscillator Stagger's units or complete sampling formula. Its circular-motion example uses a 0.25-second offset; that is a quarter-cycle only at 1 Hz. Cavalry 2.4 changed time/frequency semantics and separated deformation wave count; old scenes retain the legacy implementation and require manual adjustment to migrate. Treat older tutorials accordingly. [Oscillator documentation](https://cavalry.studio/docs/nodes/behaviours/oscillator/)

Shared Behaviour controls provide strength plus an ordered falloff list with arithmetic/blending operations. These are reusable controls across effects. [Common Behaviour attributes](https://cavalry.studio/docs/nodes/behaviours/common-attributes-behaviours/)

The curve widget supports presets, editable points and Bézier handles, flips, copy/paste, numeric point controls, and animation/connections on point values. This is a substantial part of the custom-wave UX. [Graph Attribute](https://cavalry.studio/docs/user-interface/menus/window-menu/attribute-editor/widgets/graph-attribute/)

Cavalry's standalone **Stagger** distributes a minimum-to-maximum range across element IDs, optionally shaped by a graph. Its documented time example connects that distribution to Duplicator Shape Time Offset, adding the values to the current frame. This must not be confused with the Oscillator's built-in Stagger setting or Fold's existing seconds-per-copy sampling delay. [Stagger documentation](https://cavalry.studio/docs/nodes/behaviours/stagger/)

## Recommendation for Fold

Use Oscillator as the general reusable driver asset. The current Wave group exposes Amplitude, Speed, Phase, and Center around a time-fed wave primitive ([current authoring code](../../crates/motion/src/authoring/duplicator.rs)). Its wiring model is useful; the artist-facing range and waveform controls can be much more expressive.

Fold's primitive already implements sine, triangle, and square; exposing that existing choice is an immediate improvement rather than new evaluation machinery. [Current animation nodes](../../crates/motion/src/nodes/animation.rs)

Proposed default surface: **Shape, Minimum, Maximum, Duration, Phase**, and a compact cycle preview. Duration could switch units between seconds, frames, and BPM without displaying three competing speed controls. Keep explicit time input, strength, custom curve editing, and per-element timing accessible at a deeper level. These are Fold proposals, not claims about Cavalry's exact interface.

Allow right-clicking any compatible property → **Add driver → Oscillator** to create and connect the same editable graph asset. Seed sensible ranges from the target property. Preserve its identity when opened from the inspector or graph. Retain the existing Wave asset for saved-project compatibility.

Examples: Y offset uses a signed range; opacity uses 0–1; scale uses a range around 1; Copies uses Oscillator → Round/Floor → Count, with the conversion visible and count clamped to a valid range. Count evaluates once to determine topology; per-copy phase or delay belongs to already-created copies. Do not imply that copy count can depend recursively on each copy's index.

Keep duplication/layout separate from signal generation. Keep time delay distinct from phase spread: delay retimes any upstream animation, whereas phase spread controls cycles across copies. Avoid two unexplained stagger controls both affecting the same motion. A future path-deformation asset can consume the same signal machinery without putting deformation-only controls in the everyday property driver.
