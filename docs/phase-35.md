# Phase 35 — Duplicator and Badge Wave

The authoring workflow below is historical; [phase 36](phase-36.md) separates Wave and Palette into reusable drivers. This original demo remains available.

Reference: [Obinary’s Cavalry text background wave](https://www.youtube.com/shorts/ZY9qAvAiIAo). The clip shows the animated result and a small scene hierarchy, not the actual wiring. Fold reproduces the visible layered badge wave with an explicit, editable per-copy time delay rather than claiming the reference's hidden implementation.

## Workflow and semantics

Create **Badge / label**, select it, choose **Tools → Arrange → Duplicate**, then **Tools → Animate → Wave across copies**. The Duplicator Inspector contains:

- Source: pick or edit the original grouped artwork.
- Distribution: Copies and X/Y Spacing. Copy count extends the row without changing the step.
- Wave · each copy: enable, Height (pixels), Speed (cycles per second), Stagger (seconds per copy).
- Color: enable Palette, First copy and Last copy colors, Preserve text color. Shape fill varies across copies; source outlines retain their appearance.

The position offset for copy `i` is `height × sin(2π × speed × (time − i × stagger))`. A zero stagger moves copies together. A zero height or disabled Wave leaves the distribution unchanged. The shipped group uses existing field semantics and remains editable: Linear Points → Copy to Points → Set Position → Copy Colors. Time and Index drive the existing Wave through ordinary Math nodes. Evaluation is independent of playback history.

## Implementation

- `authoring/duplicator.rs`: normal editable group asset and creation/enable commands.
- `nodes/distribution.rs`: Linear Points uses a per-copy X/Y step, unlike Line Points (fixed endpoints) or a single grid row (no Y step).
- `nodes/copy_colors.rs`: bounded palette operation over copy order, preserving text and stroke appearance.
- Shared scene/shelf intent menu exposes Duplicate and Wave across copies through the existing undo transaction lifecycle.
- `crates/app/examples/badge_wave.rs`: a background, a hidden editable badge source, and one visible Duplicator. The badge background auto-fits edited text.

## Validation

- 56 Motion tests pass, including analytical stagger timing at positive/negative/nonsequential times, count/spacing independence, stable identity, zero/one-copy cases, palette endpoints, preserved text color, document roundtrip, live source text edits, and separate creation/wave undo steps.
- 75 shared UI tests pass; three explicit native tests are ignored by default. Color properties now show a swatch and open their existing color picker on label/swatch click while retaining numeric and keyframing controls.
- Explicit Dear ImGui/WGPU render test passes. Normal/narrow Duplicator controls reviewed at `/tmp/fold-motion-v2-ui/duplicator-{normal,narrow}.ppm`. These are real control captures with evaluated outlines, not live desktop interaction acceptance.
- Release desktop and demo/profile examples build. Final project renders through Motion → Composite; a ten-second, 120-frame preview was generated on the hardware GPU and sampled across the full duration for visual review.
- Three-frame software Vulkan/CPU comparison has maximum output code errors 0, 1, 0 (8-bit output). No separate preview-only evaluator is used.
- GTX 1070 release profile: 12 frames at 960×540, warm mean **42.842 ms** including preparation/readback; warm GPU work ~5.8–7.6 ms, three passes, zero CPU adapter nodes. This is a headless output benchmark, not a guaranteed UI playback rate. Large-scene and interactive-latency acceptance remain open.
- Workspace checks and Clippy were deferred; affected Motion/UI checks and release compilation were run. Existing live desktop runtime limitations from phase 34 still apply.

## Demo

- Project: `/tmp/fold-badge-wave-demo.fold`
- Playback preview (12 fps, sampled from the 24 fps project): `/tmp/fold-badge-wave-preview.mp4`
- Still: `/tmp/fold-badge-wave-demo.png`

Open with the rebuilt desktop:

```sh
./target/release/fold-desktop --project /tmp/fold-badge-wave-demo.fold
```

Select **Duplicator** in the Motion scene to edit Copies, Spacing, Height, Speed, Stagger and Palette. Set Stagger to zero to see synchronized motion. Use Source → Edit to change “Hi Fold” or its background; all copies update. The negative X spacing places the last copy at the left/front, matching the reference composition.

Reproduce the project and optional frames at new paths:

```sh
cargo run --release -p fold-app --features gpu --example badge_wave -- /tmp/new-badge-wave.fold /tmp/new-badge-wave-frames
```

The reference’s textured background is simplified to a solid green scene object. The grouped source, colored overlapping copies, dark outlines and staggered position wave remain procedural and editable.
