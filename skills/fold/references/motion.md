# Motion authoring through the public tools

Use `catalog(type="fold.motion.document", query=...)` for current sockets and settings. The conventions below were exercised through live MCP authoring and rendering. Resolve differences with the running catalog and a small rendered probe; these examples are not a substitute for discovery.

## Sources and coordinates

| Task | Construction and check |
| --- | --- |
| Visible source | `object` creates a scene object and its appearance/transform nodes. Use batch edit receipts for the created source/appearance/transform IDs; inspect the relevant graph neighborhood for existing objects. `add` creates only a graph node; it needs an output path to become visible. |
| Rectangle placement | A raw rectangle spans `(0,0)` to `(width,height)`. To use a centered unit cell, transform a 1×1 rectangle by `[-0.5,-0.5]` **before** copying and scaling it. Pixel coordinates increase right and down. Scene-object creation adds shelf insertion placement; for a full-canvas background explicitly set its transform translation to `[0,0]`. |
| Text resource | Create text with `object`, which embeds a font and sets `settings.font`. A raw `add` text node defaults to `font: null` and cannot render. Inspection exposes font IDs/names, not resource bytes. Keep the embedded font in the document. |
| Text alignment | Text `alignment` is 0 left, 0.5 center, 1 right. This aligns horizontal advance; it does not vertically center visible glyphs. Render and adjust a source transform for the chosen font/text. A fixed vertical offset is not an automatic fit for arbitrary multiline text. |
| Color | Author in the project's working color space and inspect the rendered display transform. Display RGB/hex values are not automatically working-space values. Check whites and thin lines at the actual preview size; large scene-linear values can wash out antialiased boundaries after display conversion. |

For a simple reusable source, `extract` retains the node ID as a group instance. Inspect its `settings.group` and generated definition before modifying ports; the existing cross-feature script demonstrates this with a scalar driver.

When adding text inside a group, reuse an inspected embedded font ID from the **same document**. If none exists, create a text object to establish the resource, inspect its font and generated scene nodes, and connect or incorporate that source into the intended construction. Remove a temporary helper only after its replacement renders; preserve unrelated scene objects and nodes. Avoid leaving a setup label visible over the final artwork.

## Typed edits, not ambiguous merges

Within a batch's `edit` operation, these operations address a node in the main graph. Add `"group":"GROUP_UUID"` to address a group's internal graph.

```json
[
  {"op":"set","node":"NODE_UUID","input":"text","value":{"Text":"FOLD"}},
  {"op":"connect","node":"NODE_UUID","input":"size",
   "source":{"node":"DRIVER_UUID","socket":"value"}},
  {"op":"node","node":"MATH_UUID","value":{"settings":{"operation":"Multiply"}}}
]
```

`set` writes a literal; `connect` installs a link. They replace the socket's prior input. Inspected graph inputs serialize as `{"Value":{"Scalar":64}}` or `{"Link":{"node":"UUID","socket":"value"}}`. These are mutually exclusive variants. A `node` merge that adds `Link` over an existing `Value` can leave both tags and fail decoding. Use the dedicated operations to change input modes. `disconnect` removes the input; follow with a literal when that socket needs a value.

Math and Mix use Scalar signatures unless `settings.value_type` selects Vector or Color. Math operands must have identical types; combine two scalar channels when scaling a vector by a scalar. Mix weight is Scalar, while Compare outputs Bool: use selection sockets for comparisons or a scalar Map Range for blend weights. Color Ramp interpolation uses `Linear`, `Smooth`, or `Stepped`.

For new raw nodes in group definitions, use `version: 1`, declared defaults from the catalog, `settings`, and a two-number `position`. Required sockets without defaults need connections. Each input contains exactly one `Value` or `Link` variant.

## Editable group interfaces

Prefer adapting an inspected or extracted group over inventing its schema. A group definition contains `name`, `version`, `inputs`, `outputs`, and `graph`. Its graph has `nodes` and one `output` link. Public outputs point to explicit `group_output` nodes, whose `value_type` matches the exported content.

A port is a stable key plus artist-facing metadata:

```json
{"id":"label","name":"Text","kind":"Text","default":{"Text":"FOLD"},
 "unit":"","section":"Text","advanced":false}
```

The corresponding internal `group_input` has `settings: {"key":"label","value_type":"Text"}` and a `value` output. Connect it to the text node's `text` socket. Its `Generic` catalog socket does not mean arbitrary types are interchangeable: the port kind, default datum, and settings must agree. Use a typed Text group port for a text field; the standalone Published Control node is not a replacement for all group port types.

For a content output, an internal `group_output` has `settings: {"value_type":"Content"}` and a `value` input linked to the final content. The definition exports it as:

```json
{"outputs":{"content":{"node":"OUTPUT_UUID","socket":"value"}}}
```

An outer `group_instance` has `settings.group` set to the definition ID. Set its inputs by stable port key, e.g. `set(node=INSTANCE, input="label", value={"Text":"MOVE"})`. The definition provides defaults; the instance provides local overrides. Publishing a control requires the port **and its wiring**. Verify a changed value reaches the intended copies.

Extend existing definitions with `add`, `set`, and `connect` scoped by `group`. Use `publish` to create a port and its wiring, and `port` for targeted metadata edits (see [graph authoring](graph-authoring.md)). For unsupported interface migrations, port arrays need a freshly inspected complete list with existing entries retained. Whole graph/scene array replacement is a last resort: merge with the observed contents and guard that exact revision. Preserve node IDs, animation, layout, and extension data rather than rebuilding the user's construction.

When replacing an extracted definition's interface, disconnect obsolete overrides on the outer instance in the same batch. An instance retaining an input removed from the definition fails validation.

## Copies and deformation

Content → distribution → copies → response is the reusable pattern:

1. Establish a source's local origin and appearance.
2. Generate points and connect the source to `instance_on_points`.
3. Apply per-copy `set_scale`, `set_position`, color, or opacity fields using the intended domain and combine mode from the catalog.
4. Share field nodes between content that must stay aligned. Share the point order too: index-based selection only matches across identical distributions.

Use `set_position` with `combine: "Replace"` for absolute cell centers. Normalize the source before the copy transforms; changing size around an unintended origin creates moving gaps and offsets. Verify one source and a few copies before scaling the distribution up.

For live numeric labels, use `format_text` with `{value}`, `{x}`, or `{y}` and integer `decimals` from 0 to 6. `text_on_points` consumes this Text field at each selected point; ordinary Text expects a uniform string. Drive label points from the same position field as the artwork, reuse its point order/selection, and supply an embedded font ID from the same document. Position fields sample the point's upstream coordinates; include any later authored center offset when the label should report composition coordinates.

Shape Grid with `circular: true` and `world_aligned: true` samples live filled geometry after upstream transforms on a shared lattice. Use `cell_size` for spacing and `cell_scale` for dot diameter relative to spacing. This differs from copying a fixed dot patch onto each object. For outline-only marker rectangles, apply a transparent Fill before Stroke; a raw stroked rectangle otherwise retains its default fill.

## Example: animated grid with cell text

This is a construction pattern, not a command to add a grid to every task. A reference clip used 11 columns, 10 rows, a two-cell white diagonal, and a one-second loop; the artist later changed counts and added text through the same setup.

For row `r` of `R`, total height `H`, integer wave count `k`, loop period `T`, phase `p`, and strength `s`, define a shared row boundary:

```text
theta = t / T + p
edge(r) = r * H/R + s * H/R *
          (sin(2*pi*(theta - r*k/R)) - sin(2*pi*theta))
height(r) = edge(r+1) - edge(r)
center_y(r) = top + (edge(r+1) + edge(r))/2
```

Integer `k` anchors both outer edges. Positive counts and period are required; keeping `0 <= s < 0.5` keeps heights positive in this model. Implement with catalog Math, Time, and Wave nodes; do not precompute animation frames. Derive row and column from the common copy index. A comparison of `abs(column-row-0.5)` against half the band width selects the white cells.

Normalize a rectangle, then drive copies with cell width/height and center fields. Add grid outlines with consistent pixel widths. Expose the useful count, timing, strength, diagonal, and color controls on the outer instance.

For cell text, publish a Text port, size factor, and color. Center and normalize the embedded-font text source, copy it onto the **same points**, and reuse the rectangle's size and center fields. Hide all text copies with opacity 0, then set opacity 1 for the same white-cell selection. This makes glyphs stretch with each row while staying attached to their cells.

This construction repeats the whole label in each selected cell. It does not distribute letters across cells or automatically fit arbitrary strings. Render the requested text; provide a size control and check long/multiline edits if those are part of the brief. Compare compressed and expanded rows, red-cell exclusion, a changed label, and a full loop boundary. Keep the artist's later control changes when extending the setup.

## Editable contour echoes

For nested outline typography, use one embedded-font Text source, Align Content, and ordered pairs of Stroke nodes: a broad line-color stroke followed by a narrower background-color stroke. Keep the interior Fill opaque in the same background color. Native Expression nodes can animate stroke width and contour translation with time and a per-layer phase offset. Publish text, size, count, spacing, line width, duration, phase, stagger, motion amounts, and colors on one group instance. A bounded set of layers with count-controlled opacity supports an editable count; render the intended count and resolution.

An `object` adds its own Fill appearance, which can override authored fills inside a group. For a group that already owns its paints, bypass that appearance by wiring the group's content directly to the scene object's transform:

```js
tool.role('transform', {content: {Link: {node: tool.id, socket: 'content'}}});
```

Fade the line-color branch before combining it with its opaque interior stroke. Fading both branches blends interior paint with previous contours and introduces unwanted ghosts. Use whole-pair opacity only to disable unused layers. Removing a scene-owned graph node alone leaves an invalid scene reference; first retain the inspected scene with that object removed, then remove its owned nodes in the same candidate edit.

The live Path Points probe rejected Text geometry with `path target must contain a path`. Discover conversion/offset/trim before promising a traveling stepped contour for arbitrary text. The verified [Contour Type demo](../../../demos/contour-type.md) uses animated stroke echoes instead; S and FOLD rendered, and its eight-second same-phase frames matched exactly. Centering follows text edits; fitting arbitrary strings still needs a size control.
