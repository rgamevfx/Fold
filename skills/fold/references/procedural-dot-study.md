# Workflow: procedural chromatic dots, overlays, and glow

Use this recipe for a morphing dot system with live labels, sampled stipple, guides, and artist-facing controls. It documents the editable `demos/cavalry-chromatic-motion.fold` project. The reference was `https://www.youtube.com/shorts/vCOame09Fns`; this is a procedural interpretation, not a pixel-identical reconstruction. Other briefs may need only some branches.

Read [Motion authoring](motion.md) for public edit syntax and group/font setup; use the [glossary](glossary.md) for fields, domains, and coordinate spaces. This recipe is self-contained within the skill; the saved demo is an optional inspection fixture, not a requirement to read Rust or project archives.

## Result and controls

Create two documents at 720×1280, rate `[30,1]`, 300 frames:

- Motion: **Chromatic Motion Study**, scene object **Orbit Dots**, reusable group **Chromatic Dots**.
- Composite: **Chromatic Motion — Glow**, reading the Motion document's `video` output.

Discover current IDs with `state` and `inspect`; names help locate objects but are not unique identifiers. Inspect `/graph`, follow the instance's `settings.group`, then inspect `/groups/GROUP_ID`. The shipped group has these 21 ports; local changes use the stable key on the outer instance:

| Key | Name | Default |
| --- | --- | --- |
| `count` | Dot Count | Scalar 40 |
| `radius` | Ring Radius | Scalar 245 px |
| `size` | Dot Size | Scalar 22 px |
| `spread` | Cluster Spread | Scalar 1 |
| `waveheight` | Wave Height | Scalar 72 px |
| `speed` | Drift Speed | Scalar 1 |
| `markers` | Markers | Scalar 1 |
| `guides` | Guides | Scalar 1 |
| `color0` | White | Color `[2.3,2.3,2.3,1]` |
| `color1` | Mint | Color `[0.01,1.2,0.65,1]` |
| `color2` | Blue | Color `[0.03,0.3,1.5,1]` |
| `color3` | Yellow | Color `[2.2,1.4,0.001,1]` |
| `color4` | Coral | Color `[1.4,0.2,0.055,1]` |
| `center` | Center | Vector `[360,610]` px |
| `labelsize` | Label Size | Scalar 14 px |
| `labeloffset` | Label Offset | Vector `[0,-30]` px |
| `labels` | Labels | Scalar 1 |
| `labeltemplate` | Label Text | Text `x: {x}  y: {y}` |
| `cellsize` | Stipple Spacing | Scalar 17 px |
| `cellscale` | Stipple Dot Scale | Scalar 0.7 |
| `stipplegrow` | Stipple Source Scale | Scalar 2 |

Counts are positive integers; spacing is positive. Overlay controls are opacity factors, normally 0–1. Colors are working-space RGBA, not display hex colors. These defaults describe this demo rather than universal limits.

## 1. Establish the editable construction

Discover catalog entries for the needed nodes and the `batch` operation. Create the Motion document and a visible source object through public authoring. Set document `info` to the dimensions/rate/frames above. Establish an embedded font with a text object if the document has none; retain that resource when removing or incorporating the helper's visible scene content.

Use an extracted group or the inspected group schema described in the Motion reference. Publish typed ports, wire internal Group Inputs to consumers, and export Content through Group Output. A raw `add` only creates a graph node: ensure the group instance reaches the scene output. Finish this stage with one visible centered circle and a working size control.

## 2. Build shared position and size fields

Use Linear Points with `count=N` and `spacing=[0,0]` as an ordered distribution. Define `i=Index`, `u=i/N`, and `t=Time*speed`. Use Math, Wave, Combine XY, and typed Mix nodes to implement:

```text
cluster = spread * [215*sin(2*pi*(3*u + 0.2*t)),
                    210*sin(2*pi*(2*u + 0.17))]
ring    = radius * [cos(2*pi*u), sin(2*pi*u)]
wave    = [650*u - 325, waveheight*sin(2*pi*(1.5*u - 0.2*t))]
position = mix(mix(cluster, ring, ring_weight), wave, wave_weight)
size_variation = 1 + 0.6*sin(2*pi*(0.381*i + 0.1))
scale = mix(0.72, size_variation, variation_weight)
```

Wave phase is in cycles: cosine is Sine with phase `0.25`. Set `settings.value_type="Vector"` for position Mix nodes. Build scalar x/y math separately, then Combine XY; do not assume Vector × Scalar is accepted by Math. Mix weights remain Scalar.

Animate the four weights with Keyframes using `Ease`. Each list below is `frame:value` at 30 fps for the first five-second period:

| Weight | Keys |
| --- | --- |
| `ring_weight` | `0:0, 45:0, 60:1, 78:1, 93:0, 123:0, 150:0` |
| `wave_weight` | `0:0, 78:0, 93:1, 108:1, 123:0, 150:0` |
| `variation_weight` | `0:1, 45:1, 60:0, 108:0, 123:1, 150:1` |
| `stipple_weight` | `0:0, 15:1, 45:1, 57:0, 147:0, 150:0` |

For the ten-second document, repeat keys with a 150-frame offset, keeping only one key at the shared boundary. Key times are rational seconds, e.g. frame 45 is `{"numerator":45,"denominator":30}`. This is a repeated finite key schedule, not an automatically looping driver. Drift Speed changes the continuous waves, not the key times; the default speed has verified 0s/5s equality. Recheck the loop after speed edits.

## 3. Instance and color the circles

Ellipse width/height are `size`. Translate its source by `[-size/2,-size/2]` before instancing. Fill it, connect it and the shared points to Instance on Points, then Set Scale with `[scale,scale]` (`Instances`, `Multiply`) and Set Position with the shared position field (`Instances`, `Replace`). Keep the whole composition's `center` translation for the final combined content.

For the demo palette, compute `q=0.5+0.5*sin(2*pi*0.618*i)`. Chain four Color Mix nodes from `color0` toward `color1`…`color4`; map q across narrow clamped ranges `[0.2,0.200001]`, `[0.4,0.400001]`, `[0.6,0.600001]`, `[0.8,0.800001]` to scalar weights 0–1. Set Color/Opacity on the instances with `Replace`. This keeps all five colors publishable. A catalog Color Ramp can be a simpler alternative when individually published palette inputs are unnecessary.

Render the cluster, ring, and wave before adding overlays. Changing count must update every corresponding branch; changing radius should affect the ring without baking new positions.

## 4. Derive stipple from the moving circles

Branch the colored circles **before** their visibility response:

```text
colored circles ── opacity(1-stipple_weight) ─────────────── normal dots
              └── instance scale(stipplegrow)
                  → Shape Grid → opacity(stipple_weight) ─ sampled dots
```

Shape Grid inputs are `circular=true`, `world_aligned=true`, `cell_size=cellsize`, `cell_scale=cellscale`. It samples transformed filled geometry on a shared lattice. A fixed patch copied onto each dot would not sample the moving boundaries. The final center translation comes after sampling; “world aligned” here refers to the transforms upstream of Shape Grid, not later transforms.

Add the faint background lattice with 32×32 Grid Points, spacing `[cellsize,cellsize]`, origin `[-15.5*cellsize,-15.5*cellsize]`. Instance centered 5 px circles filled `[0.018,0.018,0.018,1]`; opacity is `stipple_weight`. Combine it behind the artwork. This background has its own fixed count; it is not the Dot Count distribution.

## 5. Keep markers and labels attached

Use the common dot distribution and fields for both branches. The shared selection is `Modulo(Index,2) < 1` (Compare `Less`), selecting every other point.

**Markers:** combine a centered `size` square with transparent Fill and a 1.3 px Stroke `[0.8,0.01,0.18,1]`, plus a centered cross made from 6×1 and 1×6 rectangles filled `[0.03,1,0.9,1]`. Instance on the common points; reuse the dot scale and position fields. Set all marker opacity to 0, then selected opacity to `(1-stipple_weight)*markers`. Keeping invisible elements preserves index correspondence. A stroke alone does not remove the rectangle's default fill.

**Labels:** apply Set Position to the common points using the same position field, with `domain="Points"` and `combine="Replace"`. Feed these points to Text on Points with the same selection, same-document embedded font, size `labelsize`, offset `labeloffset`, alignment 0, and line spacing 1.2. Drive its text with Format Text:

```text
Position → Separate XY ── add Center.x/y → Format Text.x/y
Label Text ─────────────────────────────→ Format Text.template
Format Text.value ──────────────────────→ Text on Points.text
```

Set `decimals=0`. The text reports composition coordinates by explicitly adding Center, while the actual label geometry receives Center only in the final transform. Fill labels `[0.9,0.9,0.9,1]`; opacity is `(1-stipple_weight)*labels`. Labels inherit positioned points, not dot scale, so changing Dot Size does not stretch the font.

This scoped edit fragment illustrates wiring already-created nodes; replace the uppercase strings with inspected IDs. Wrap the array in `{"op":"edit","document":"MOTION_ID","edits":[...]}` within a revision-guarded batch:

```json
[
  {"op":"set","group":"GROUP_ID","node":"FORMAT_ID","input":"decimals","value":{"Scalar":0}},
  {"op":"connect","group":"GROUP_ID","node":"LABELS_ID","input":"text","source":{"node":"FORMAT_ID","socket":"value"}}
]
```

## 6. Add the dotted guide curves

Guides have a separate 180-point distribution because they sample a continuous path rather than correspond to individual dots. Let `v=Index/180` and use the same time/drift, radius, wave height, and wave weight:

```text
lissajous = [radius*cos(2*pi*v), 0.78*radius*sin(2*pi*(3*v+0.2*t))]
wave_path = [650*v-325, waveheight*sin(2*pi*(1.5*v-0.2*t))]
main_guide = mix(lissajous, wave_path, wave_weight)
extra_y = wave_path.y ± 110*sin(2*pi*v)
```

Instance 3×2 px ellipses filled `[0.22,0.22,0.22,1]`; position with `Instances/Replace`. The main guide's opacity is `(1-stipple_weight)*guides`; the two extra wave curves also multiply by `wave_weight`. These repeated small shapes provide the dashed appearance without relying on viewer overlays. Combine guides behind the dots/markers/labels, then apply one final `center` translation to the combined content.

## 7. Keep glow in the Composite document

Create a Composite with matching `info`, then build this active output path:

```text
Read(Motion.video) ───────────────────────┐
        └→ GaussianBlur → Grade → Plus ──┴→ Output
```

Read starts at 0 with source_start 0 and duration 10 rational seconds. GaussianBlur parameters are `{"edges":"Clamp","size":[5,5]}`; Grade gain is `[0.08,0.08,0.08]`; merge parameters are `{"Composite":{"mode":"Plus"}}`. Connect graded blur to `foreground`, original Read to `background`, then connect the merge to Output's `image` and select that Output. Inspect the compositor catalog for the edit envelope and current socket names; the [cross-feature script](../scripts/demo.py) shows document Read construction.

Render the Composite output as well as Motion. The glow must reference editable Motion content; no intermediate movie is needed. The shipped demo can contain unused default nodes—follow its selected output, not the first Output node found.

## 8. Verify and preserve the setup

Render 0s (cluster), 1s (stipple), 2s (ring), 3s (wave), an in-between, and 5s at the same revision and dimensions. Compare 0s and 5s for the default loop. In an isolated copy, try Dot Count 24, Ring Radius 180, a new label template, and a moved Center. Check count/marker/label correspondence, sampled stipple, reported coordinates, and readability; restore the delivered defaults after probes.

For local control edits, use the outer instance rather than rewriting its definition:

```json
[
  {"op":"set","node":"INSTANCE_ID","input":"count","value":{"Scalar":24}},
  {"op":"set","node":"INSTANCE_ID","input":"radius","value":{"Scalar":180}},
  {"op":"set","node":"INSTANCE_ID","input":"labeltemplate","value":{"Text":"({x}, {y})"}}
]
```

Save through the public tool with fresh revision/epoch guards and verify the job outcome. When reopening is part of the brief, reopen through the supported host workflow, rediscover the session, and verify the documents, 21 ports, embedded font, and rendered result. `navigate` acknowledges a request; use viewer displayed/requested identities to establish which source/time is actually shown. During playback a pending next frame may legitimately make a capture stale.

After authorized app implementation changes, hand back a release build and verify the running executable. Measure representative times at delivery dimensions on the actual GPU with compilation stopped; distinguish cold startup, warm completion, and readback. This exercise initially handed back a debug build and incurred roughly a tenfold slowdown. Opacity hides artwork but does not guarantee that upstream graph evaluation is skipped.

## Applying this recipe to new briefs

Keep the common distribution and field wiring when replacing circles with another shape. Change layout formulas and transition weights independently of overlay construction. Reuse the separate guide distribution when smooth paths need more samples than the visible copy count. Decide which timings, palette controls, and secondary counts the artist actually needs exposed.

If a capability is missing from the live catalog, inspect a public example or make a bounded validation/render probe. Distinguish a missing setting description from a missing operation. Record the precise gap; implementation requires the user's development scope. This exercise required Format Text, Text on Points, and Shape Grid sampling options, while polymorphic settings/enums/domain metadata remain catalog follow-up work. Documentation should not require an art agent to investigate Rust crates.
