# Motion native recipes

Use the [public workflow](public-workflow.md) for guards, lossless inspection,
proposal jobs, candidate review and Apply. The snippets below are **provider
edits** inside an `author` request unless stated otherwise. Uppercase quoted
records are placeholders for full inspected JSON, not literal strings. Discover
all required fields; these examples are not a replacement schema.

## Parameters and images

Move a native object's X scalar by copying the inspected scalar into `expected`
and `value`, then changing only `value.value`:

```json
{"op":"parameter","object":"OBJECT_UUID","expected":"INSPECTED_X_SCALAR","value":"EDITED_X_SCALAR"}
```

Preserve the scalar ID and other fields. Motion images reference an existing
project Asset ID and use this same edit for transform, opacity, width and height.
Stroke width is not an image control. Dimensions use canvas units; fit is
`contain`, `cover` or `stretch`.

For creation, use the discovered `insert` schema, new object/scalar UUIDs and a
valid root/group index. Content has `kind: "image"`, `asset`, width/height scalars
and `fit`; fill/stroke are null. The document derives asset references from its
objects. The public fixture qualified create/edit/candidate pixels/save/reopen/
Undo while retaining the source asset. Agent import and fit-change workflows
remain unqualified: [image evidence](../../../resources/motion-context/mcp-image-evidence.json).

## Convert a script to editable objects

This is a whole `edit.request`, not an edit inside `author.edits`:

```javascript
{
  op: "capture", document: documentId, expected: inspection,
  object: scriptId, time: {numerator: 1, denominator: 2}
}
```

Use inspection from the guarded revision and a time inside output duration.
Capture keeps object ID, placement and transform, replacing supported local script
output with static editable children. Text becomes outlines. Undo restores the
script; capture does not recover procedural logic or animation. Protected script
controls and unsupported paint/effects reject.

Declared project images retain asset IDs. Centered Contain/Cover/Stretch images
and full-tile patterns have qualified conversion; arbitrary pattern effects and
other alignments reject. Reinspect after Apply. For a native linear-gradient stop
edit, losslessly copy its object, change the desired encoded sRGB RGBA color
(0..=1), and retain all object/stop IDs:

```json
{"op":"replace","id":"OBJECT_UUID","expected":"INSPECTED_OBJECT","object":"EDITED_OBJECT"}
```

Evidence: [capture/gradient](../../../resources/motion-context/mcp-linear-gradient-evidence.json),
[image capture](../../../resources/motion-context/mcp-image-capture-evidence.json).
Native conversion/gradient inspector acceptance, public radial/pattern editing
and gradient animation are not established by those checks.

## Animate anchors and handles

A path point's ID is its stable position parameter ID. Discover its vector/X/Y
sampling targets. Coordinates are path-local canvas units. Copy the point as
`expected` and `value`; set `value.position_track` using the discovered typed Track
schema: a replacement two-component canvas/screen vector, fresh track/key IDs and
exact rational key times. All keys use the same vector type.

```json
{"op":"point","object":"PATH_UUID","expected":"INSPECTED_POINT","value":"EDITED_POINT"}
```

Later edits retain existing point/track/key IDs. `incoming_track` and
`outgoing_track` use the same schema but store **offsets from the sampled anchor**,
not absolute coordinates. Get their targets through parameter discovery rather
than constructing property keys. Artist Auto Key and shared curve channels project
these same tracks; native curve acceptance and exported trajectory parity remain
open. Evidence: [anchor](../../../resources/motion-context/mcp-anchor-evidence.json),
[outgoing handle](../../../resources/motion-context/mcp-handle-evidence.json).

## Move an anchor or handle

Unlike stored tracks, `move_point.position` is an **absolute canvas position**
for an anchor or handle endpoint:

```json
{"op":"move_point","object":"PATH_UUID","expected":"INSPECTED_OBJECT","point":"POINT_UUID","part":"outgoing","position":[470,280],"time":{"numerator":1,"denominator":1},"linked":false,"auto_key":false}
```

Choose `anchor`, `incoming` or `outgoing`. `linked: true` mirrors the other handle.
Auto Key creates a missing track; existing tracks update/insert a key at the
explicit time even with Auto Key off. Within a multi-edit request, each expected
object includes preceding edits' results.

[Public evidence](../../../resources/motion-context/mcp-move-point-evidence.json)
covers anchor/outgoing moves at existing keys on a sheared path, stable IDs,
candidate pixels, save/reopen and Undo. Incoming, linked and new-key MCP moves
remain unqualified.

## Subdivide or remove points

```json
{"op":"split_segment","object":"PATH_UUID","expected":"INSPECTED_OBJECT","contour":"CONTOUR_UUID","start":"POINT_UUID","fraction":0.35,"time":{"numerator":1,"denominator":2}}
```

Fraction is the cubic parameter, or line fraction when both handles collapse at
editing time. Compatible Linear/Hold/HoldNext intervals preserve curve trajectory.
Source checks also cover aligned Bézier keys with matching per-component timing
fractions, including resolved Auto handles; other timing rejects. Inspect generated
point/track/key IDs before later edits; retained source IDs stay stable.
[Public subdivision evidence](../../../resources/motion-context/mcp-subdivision-evidence.json)
used Linear tracks. Bézier MCP/native/export checks remain open. Raster edge
pixels can differ after subdivision even when geometry is preserved.

```json
{"op":"remove_point","object":"PATH_UUID","expected":"INSPECTED_OBJECT","point":"POINT_UUID"}
```

Removal deletes the point and its owned coordinate tracks across the timeline.
Other points, their handles and contours remain; an empty path stays editable.
Neighboring handles do not adjust, so geometry can change.
[Removal evidence](../../../resources/motion-context/mcp-removal-evidence.json)
covers animated removal, candidate pixels, save/reopen and Undo; native popup/focus
acceptance and closed-path removal pixels remain open.
