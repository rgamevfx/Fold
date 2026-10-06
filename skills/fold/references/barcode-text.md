# Animated barcode text through MCP

Use this construction for independently changing bars inside live text. The verified example is [Katie — Chromatic Bars](../../../demos/katie-chromatic-bars.md).

1. Create a Motion text `object` to establish its embedded font and scene roles. Render and position it before building the treatment. Glyph geometry has a font-dependent vertical offset; inspect the rendered result rather than treating the source position as the visible top edge.
2. For stepped outlines, compress text vertically with Transform, pass it through Shape Grid with `world_aligned: true`, and apply the inverse scale. Optional centered 3×3 copies before sampling expand strokes while retaining the original text. Shape Grid returns combined geometry: Realize Instances does not turn those subpaths into independently addressable cell elements.
3. Create a separate Rectangle object as a unit bar. Center its source with a `[-0.5,-0.5]` transform, distribute it with Grid Points → Copy to Points, and run an Instances-domain Rust Wrangle. Use `position` for grid row/column, assign `color`, and write `set_attrv("bar_scale", [width,height])`. Read Attribute (Content/Vector) → Set Scale (Instances/Multiply) applies that attribute. Wrangle itself exposes translation, not writable scale.
4. Mask the bar object with the hidden text object's output in World space. Both objects retain their transforms; verify registration by rendering. The scene mask's verified serialized shape is below. Inspect the current scene array and preserve every unrelated object when applying a revision-guarded `scene` edit.
5. Group internal processing nodes and `publish` useful inputs. For inspector access, register the corresponding group-instance IDs in the owning scene object's `modifiers` list; validate the complete candidate before committing. Keep the text source and bar source as native scene objects.
6. Background objects need explicit `[0,0]` translation and full-frame dimensions. Scene objects are ordered front to back: place the background last. A newly created rectangle can otherwise obscure only part of the artwork and resemble a masking failure.
7. Render several times, compare the loop boundary, and test a text/control change before saving. Keep the live viewer on the intended composition.

Verified mask entry (`source` is the text **scene object** ID):

```json
{"id":"MASK_UUID","source":"TEXT_OBJECT_UUID","space":"World",
 "operation":"Intersect","invert":false,"opacity":1.0,"feather":0.0}
```

The text object's `visible: false` hides its direct presentation while retaining it as a mask source. A fresh mask needs its own stable ID. Local/World coordinate spaces and Intersect/Add/Subtract operations are supported by validation. Prefer a targeted mask tool if a newer catalog supplies one; this version requires a scene-array edit.

For loops, compute `t = time.seconds.rem_euclid(period)` before both the width phase and palette refresh tick. Clamp period and width-shaping inputs to valid ranges. Use deterministic row/column/seed hashing for colors. Separate the width-wave period from discrete color refresh so the artist can tune motion without changing the palette controls.

This workflow used only public MCP catalog, inspect, batch, render, navigate, capture and save calls. Current catalog gaps for mask fields, targeted scene edits and font resource authoring are tracked in dev-plan phase 45 follow-ups.

## Construction checkpoints

Render the plain text, sampled silhouette and unmasked bars separately before combining them. Confirm the domain and output type at each stage: a Shape Grid path is combined geometry, while the independent bar field supplies addressable instances. If a transform affects the entire word instead of individual cells, inspect that distinction before adding more modifiers.

If part of the word disappears, inspect background position, front-to-back scene order and mask coordinate registration before changing text geometry or diagnosing a mask bug. Katie's misplaced background initially covered part of the text; adding stroke copies did not address that occlusion.

Expose controls according to artist intent: text/font size; silhouette expansion/detail; bar spacing, row height, width and row fill; loop period, palette refresh and seed; palette colors. Keep text and silhouette reachable even while their direct visibility is off. Check long text against the finite bar-grid coverage and make the coverage limit clear in the artist guide.

Render at the target count before delivery: Katie uses 180×18 = 3,240 instances. For slow playback or verification after an optimization, follow [Performance and output equivalence](performance.md). Its matched-request checks explain why a diagnostic thumbnail differs from a native render at thumbnail dimensions, and how to preserve the artwork while locating the runtime cost.
