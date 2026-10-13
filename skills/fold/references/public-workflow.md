# Public authoring workflow

## Connect and discover

On Linux, enable a project socket with
`fold-desktop --protocol-dir <directory> [--project <project.fold>]`.
Use the reported private socket. MCP launches `fold-cli mcp <socket>`;
CLI sends the same JSON on stdin to `fold-cli protocol <socket>`.
MCP negotiation uses `2025-11-25`. The Fold `fold_request` tool uses the
schema from discovery:

```json
{"version":2,"command":{"method":"discover"}}
```

Keep `epoch` from the response and `revision` from `result`. Read
`documents`, `scoped_proposals`, `project_authoring`, `request_schema` and
`limits`. Follow the connected build's advertised capabilities.

Motion uses `fold.motion.canvas`; 3D uses `fold.3d.scene`. Composition,
Compositor and Timeline remain separate providers. Unsupported documents
stay opaque through save/reopen.

## Inspect, propose and apply

1. Send `inspect_document` with `epoch`, `revision` and
   `scope: {"document": DOCUMENT_UUID}`. Parse `result.source` as JSON.
   The authoritative payload is under `native`; outputs and image inputs
   are derived descriptions.
2. Build typed edits from the provider's discovered authoring schema.
   Retain object, point, parameter and key IDs. Use the exact inspected
   values in expected-value fields.
3. Send `propose` with a fresh request UUID and this shape:

```json
{
  "version": 2,
  "command": {
    "method": "propose",
    "guard": {
      "epoch": "EPOCH_UUID",
      "request_id": "NEW_REQUEST_UUID",
      "base": 1,
      "scope": {"document": "DOCUMENT_UUID"}
    },
    "edit": {
      "kind": "native",
      "request": {
        "op": "author",
        "document": "DOCUMENT_UUID",
        "expected": "SUBSTITUTE_INSPECTED_OBJECT",
        "edits": ["SUBSTITUTE_TYPED_EDITS"]
      }
    }
  }
}
```

The substitution strings above are placeholders for JSON objects, not
literal request values. For example, moving an existing native shape to
X = 42 uses:

```javascript
const object = expected.native.roots[0];
const before = expected.native.objects[object].transform.x;
const edit = {
  op: "parameter", object, expected: before,
  value: {...before, value: 42.0}
};
```

Preserve floating-point number representations when sending this record;
see the guard note below. This authoring shape is verified through the real
CLI/MCP subprocess in `crates/app/tests/agent_mcp.rs`, including inspection,
Apply and exact replay. Artwork authoring uses public schemas; repository
implementation tests are evidence, not required artwork inputs.

4. Poll any returned job. On completion, retain `result.handle`.
5. Send `inspect_candidate` with that handle. Review its native source and
   diff, and retain its receipt. Preparation and inspection preserve the
   committed project.
6. Send `apply` with the same handle, receipt and a fresh guard at the
   current revision. Apply commits one undo entry. An active artist gesture
   causes a conflict. Finish the gesture before sending a new request.

Exact retries use the same request UUID and identical bytes. Changed
requests use a new UUID. For concurrent Motion/3D document changes,
inspect and prepare again; these providers currently reconcile unchanged
or already-identical records. Compositor has its own discovered source
and reconciliation capabilities.

## Jobs, project assembly and saving

A running response returns `result.job`. Poll
`{"version":2,"command":{"method":"job_status","handle":JOB,"wait_ms":1000}}`.
Retain JOB intact. A wait timeout leaves the job running; it does not undo work.

For multi-document changes, use `edit: {"kind":"project","edits":[...]}`
and the discovered `project_authoring.schema`. Current operations include
creating documents, adding `motion`, `3d` or `image` composition layers,
typed provider authoring and connecting discovered layer inputs. Inspect
all changed documents before applying the candidate. Motion and 3D outputs
retain their provider's published port identity.

Save committed artwork with `save_project_copy` and a project guard
(`epoch`, fresh `request_id`, current `base`, no document scope). Use the
absolute destination requested by the user. Poll the save job; a transport
timeout does not establish whether publication occurred.

## Convert a script to editable objects

When the discovered Motion request schema includes `capture`, place this body
under the existing proposal's `edit.request`:

```javascript
{
  op: "capture",
  document: documentId,
  expected: inspection,
  object: scriptId,
  time: {numerator: 1, denominator: 2}
}
```

`inspection` is the losslessly parsed document inspection from the same guarded
revision. Choose a time inside the output duration. Poll preparation, inspect
the candidate and its pixels, then Apply its receipt. The object keeps its ID,
placement and transform; its captured local output becomes static editable
children. Undo restores the script. Text captures as outlines. Protected script
controls and unsupported captured paint/effects cause preparation to fail.

The real MCP check qualified script creation, capture at 1/2 s and a subsequent
native linear-gradient stop edit on a sheared rectangle under Motion schema 4.
The same check includes a declared project image and repeating image paint.
Conversion preserves their asset IDs. Centered Contain/Cover/Stretch images and
full-tile image patterns are supported; arbitrary pattern effects and other image
alignments reject. Original, candidate and committed display pixels matched.
The stop edit changed the candidate pixels; Apply produced those same pixels
and retained all stop IDs. Public save/reopen retained the documents. Artist
undo restored the stop, then the script, then the original shape. Conversion
and gradient inspector controls still need native desktop artist acceptance.

After capture, inspect the current document again. A native linear paint has
`kind: "linear"` and ordered `stops` with stable IDs, offsets and encoded sRGB
RGBA colors in 0..=1. Make `editedObject` a lossless copy of the inspected object
and change the desired stop color. Preserve its IDs and unknown fields. Use
the [lossless JSON guard](#lossless-json-guards) for copied numeric values.
Place this request inside the standard native proposal:

```javascript
{
  op: "author",
  document: documentId,
  expected: inspectedProvider,
  edits: [{op: "replace", id: objectId, expected: inspectedObject, object: editedObject}]
}
```

Inspect the candidate and its frame before Apply. Discover the new revision
after Apply before inspecting or observing committed data again. Linear paint
is editable; radial/pattern paint and gradient animation remain unqualified.

## Frame review

Read `frame_observation.available` from discovery. With an attached GPU host,
`observe_frame` accepts a committed target or an inspected candidate handle with
an explicit output document.
For a committed Motion output, substitute discovered IDs and revision:

```json
{
  "version": 2,
  "command": {
    "method": "observe_frame",
    "target": {
      "kind": "committed",
      "epoch": "EPOCH_UUID",
      "revision": 1,
      "document": "DOCUMENT_UUID"
    },
    "output": "video",
    "time": {"numerator": 1, "denominator": 2},
    "dimensions": [640, 360]
  }
}
```

Use the actual published output name. Time must be inside its half-open duration.
Poll the returned job. The result contains provenance and `image` with
`encoding: "rgba8_hex"`, dimensions, top-to-bottom rows and opaque display pixels.
Decode the hex into RGBA bytes to view the image. These pixels include the viewer
display transform; they are not a linear HDR or transparency export.
An optional `region: {x, y, width, height}` extracts a pixel rectangle from the
full raster specified by `dimensions`.

The native Motion MCP check qualified this route for a rectangle edit at 640×360
and time 1/2 s: candidate pixels matched the committed result after Apply, while
the original committed frame stayed unchanged before Apply. Evaluation used
zero CPU adapter nodes and zero readback bytes on a GTX 1070 Vulkan host. The
same route also passed with a sheared native rectangle under document schema 2.
Inspect each successful job before claiming visual quality. This display check
does not qualify HDR/alpha export.

For animation review, add this field to the same `observe_frame` command:

```json
"review": {
  "times": [{"numerator": 0, "denominator": 1}],
  "layout": {"kind": "contact_sheet", "columns": 2}
}
```

The command's primary `time` comes first. Additional poses follow in the supplied
order. Use `"layout": {"kind": "onion"}` for an equal-weight average of display
pixels. All poses use one immutable committed or candidate snapshot. Read each
pose's time, provenance and pixel hash under `review.frames`. Poll the normal job.

Discovery publishes limits: 2–16 poses, at most 2,073,600 total source pixels,
and at most 4096 pixels on either output axis. Contact-sheet padding counts
against the output pixel limit. Reduce frame dimensions when reviewing many poses.
This is display review, not HDR export or an interpolation mode.

The native MCP anchor-animation check qualified two 640×360 poses at 1/2 s and
0 s. It verified contact-sheet tile bytes, onion averages, unchanged committed
artwork before Apply, published candidate parity, save/reopen and Undo. See
[review evidence](../../../resources/motion-context/animation-review-evidence.json).

For linked contexts, observe the committed Compositor or Timeline document at
the current discovered revision using the same request above. The native MCP
check also qualified a saved/reopened project linking a scripted Motion object
and a 3D scene through Compositor into Timeline. Before Apply, observing Timeline
still returned the original pixels. After Apply, Timeline matched Compositor,
and both source contexts contributed visible pixels with zero CPU adapter nodes
and zero evaluation readback bytes. To review a downstream output before Apply,
choose its document in the same immutable candidate project:

```javascript
{
  method: "observe_frame",
  target: {kind: "candidate", handle, document: timelineDocumentId},
  output: "video",
  time: {numerator: 1, denominator: 2},
  dimensions: [640, 360]
}
```

Place this command inside the standard version-2 request. The candidate target
requires `document`, including when reviewing the edited Motion document itself.
The real mixed-context MCP check verified that a scoped Motion edit's candidate
Timeline frame matched the committed Timeline after Apply, while committed
pixels stayed unchanged during preparation. Choosing a viewing document does
not change the proposal's write scope. After Apply, discover the new revision
and review the committed parent output separately.

## Controls and exact time

Motion script objects expose number, text, Boolean and color controls.
Scripts stay one object in the graph, even when they generate many shapes.
Native shapes and scripts share the same Motion document and fframes output.
Artist and agent edits use the same guarded transactions and undo.

Inspect parameters through `inspect_parameters`, then use returned stable
`target` records with `resolve_parameter` and `sample_parameter`. Keep
placement owner IDs for repeated uses. Composition and Timeline map source
time before sampling the target document. Time uses exact
`{ "numerator": N, "denominator": D }` records. Native Motion distances use
canvas units; numeric script controls remain scalars. 3D controls retain
world units and coupled vector/quaternion keys.

Motion controls currently belong to their script object. Composition
publication binds its layer/effect controls. Cross-document expression
links and geometry observations require further qualification; use discovery
rather than assuming availability. Native image editing is qualified below.

## Create and edit native images

Create an image from an existing project Asset ID with the discovered native
`insert` edit. Give the object and each scalar a fresh stable UUID. Its content
uses `kind: "image"`, `asset`, width/height scalar records and `fit`. Complete
transform and paint fields from the discovered object schema; set fill and
stroke to null. Insert at a valid root or group index. The document's asset
references are derived from its native objects. Inspect and review the candidate
before Apply. Use fresh discovery and inspection to edit the created object.

For an image already in the Motion document, inspect its native object and
preserve its Asset ID. Use its stable transform, opacity, width and height
scalar IDs for guarded `parameter` edits. Image dimensions use canvas units;
fit is `contain`, `cover` or `stretch`. Stroke width is not an image control.

For example, copy the inspected `transform.x` scalar as `expected`, copy it
again as `value`, and change only `value.value`. Submit this provider edit
inside the standard native author request:

```json
{"op":"parameter","object":"IMAGE_OBJECT_UUID","expected":"INSPECTED_X_SCALAR","value":"COPIED_X_SCALAR_WITH_NEW_VALUE"}
```

Replace the quoted scalar placeholders with full inspected JSON objects.
Inspect the candidate, review its pixels, then Apply its receipt. Discover the
new revision before reviewing committed output. The real MCP image check
verified candidate/Apply pixel parity, unchanged committed pixels during
preparation, creation from an existing Asset ID, later edits, save/reopen and
artist undo/redo. Undo of creation removes the object and its document reference,
while preserving the source asset. Agent image import and fit-change workflows
still need public qualification.

## Animate path anchors

Inspect the path's native contours and point records. An anchor's existing point
ID is its stable position parameter ID. Parameter discovery returns its vector
and X/Y targets; use returned targets for exact-time sampling. Coordinates use
canvas units in the path's local space.

Copy the inspected point as `expected` and `value`. Set `value.position_track`
from the discovered typed Track schema: a replacement two-component vector in
canvas/screen space, a fresh track ID, and fresh key IDs with exact rational
times. Use the same vector type for every key. Submit this native edit inside
the standard guarded author request:

```json
{"op":"point","object":"PATH_OBJECT_UUID","expected":"INSPECTED_POINT","value":"COPIED_POINT_WITH_POSITION_TRACK"}
```

Replace the quoted point placeholders with full JSON records. Preserve point,
track and existing key IDs when changing later keys. Keep Bézier handles as
offsets from the sampled anchor. Review candidate pixels at intended times,
Apply the receipt, then use fresh discovery/inspection for later edits.

The real MCP check qualified track creation, midpoint parameter sampling,
subsequent key editing, unchanged committed pixels during preparation, exact
candidate/Apply pixels, save/reopen and artist undo of both edits. Native direct
render checks separately qualified five poses against specified static geometry.
Handle offsets use `incoming_track` and `outgoing_track` with the same typed
replacement-vector schema. Their values are offsets from the sampled anchor,
not absolute canvas locations. Preserve the point ID and existing track/key IDs
when editing them. Discover their whole-vector and X/Y targets through
`inspect_parameters`; handle property keys combine the point ID with the fixed
incoming/outgoing role. Use the returned target for `sample_parameter`.

The extended real MCP check qualified outgoing-track creation and later key
editing together with anchor animation, midpoint handle sampling, candidate/Apply
pixels, save/reopen and both artist Undo steps. Direct native render checks also
qualified incoming/outgoing animation at five specified poses. Artist Auto Key
creates/updates coordinate tracks; shared curve channels project their coupled
X/Y keys. Native artist curve interaction and exported trajectory parity remain
unqualified through the public workflow. See the subdivision workflow below
for its qualified protocol scope and remaining raster limitation.

## Subdivide animated path segments

Inspect the current native path object, contour and segment-start point. Submit
`split_segment` inside the standard guarded author request:

```json
{"op":"split_segment","object":"PATH_OBJECT_UUID","expected":"INSPECTED_OBJECT","contour":"CONTOUR_UUID","start":"START_POINT_UUID","fraction":0.35,"time":{"numerator":1,"denominator":2}}
```

Replace the quoted expected placeholder with the full inspected object record.
The fraction is a cubic parameter, or a line fraction when both handles collapse
at the explicit editing time. Compatible Linear, Hold and HoldNext intervals
preserve the whole curve trajectory. Source tests also qualify Bézier intervals
with aligned key times and matching time-handle fractions per component,
including resolved Auto handles. Other timing rejects. The real MCP qualification
used Linear tracks; Bézier MCP/native/export qualification remains open.
Inspect the candidate for generated point,
track and key IDs before further edits. Existing source IDs remain stable.

The real MCP check qualified subdivision proposal, candidate/Apply pixels,
save/reopen and Undo. Localized edge differences remain between the original and
subdivided rendered paths; do not claim exact raster invariance. Native artist
interaction and export trajectory parity remain unqualified.

## Remove a native anchor

Inspect the current path object and its point IDs. Submit this edit inside the
standard guarded author request:

```json
{"op":"remove_point","object":"PATH_OBJECT_UUID","expected":"INSPECTED_OBJECT","point":"POINT_UUID"}
```

Replace the expected placeholder with the full inspected object. Removal deletes
the chosen point and its owned coordinate tracks across the timeline. It retains
the path object, contours and other points; an empty path remains an editable
object. Neighboring points keep their handles, so the path geometry can change.
Review the candidate and Apply its receipt before further inspection or edits.

The real MCP check qualified animated-point removal, unchanged committed pixels
during preparation, candidate/Apply pixels, save/reopen and two-step Undo of
removal and prior animation creation. Native artist popup/focus acceptance and
closed-path removal pixels remain open.

## Pixel review

When discovery reports `frame_observation.available`, use `observe_frame`
with a committed epoch/revision/document or candidate handle and explicit
document, the published
output, exact time and dimensions. Decode `rgba8_hex` for review. Rendering
availability requires the shared GPU host; Motion's native GPU path is
currently qualified on Linux. CPU measurements are separate capabilities.
Record the applied revision and any unverified artist or rendering behavior.

## Lossless JSON guards

The Fold version 1 MCP session found that expected-document comparisons distinguish
`1` from `1.0`. Preserve inspected JSON number representations when sending the
complete `expected` envelope for `author` or `hierarchy`. A JavaScript client can use:

```javascript
const expected = JSON.parse(inspected.source, (key, value, context) =>
  typeof value === "number" && /[.eE]/.test(context.source)
    ? JSON.rawJSON(context.source)
    : value
);
```

Pass this object directly to the tool; storing and reparsing it as ordinary JSON
can lose the representation again. This resolved a reproducible `Expected document
differs` conflict at the same epoch/revision. Source-string edits keep their exact
inspected text. See the [demo evidence](../../../demos/Fold-version-1/evidence.json).

Composition publication currently binds position, opacity and effect controls;
it does not publish a Motion object's inputs across documents. Keep those inputs
on the Motion container and report that limit explicitly.

## Move an anchor or handle

Use the discovered native `move_point` edit with the full inspected object as
`expected`. `position` is an absolute canvas position for both anchors and
handle endpoints. Native handle tracks still store path-local offsets.

```json
{"op":"move_point","object":"PATH_OBJECT_UUID","expected":"INSPECTED_OBJECT","point":"POINT_UUID","part":"outgoing","position":[470,280],"time":{"numerator":1,"denominator":1},"linked":false,"auto_key":false}
```

Replace the placeholders with inspected IDs and the full object. Choose
`anchor`, `incoming` or `outgoing`. `linked: true` mirrors the other handle;
`auto_key: true` creates a track when absent. Existing tracks update or insert a
key at the explicit time even when Auto Key is off. Submit through the ordinary
author candidate/inspect/Apply workflow. In a multi-edit request, each expected
object must include the preceding edit's result.

The real MCP fixture qualified anchor and outgoing moves at existing keys on a
sheared path, candidate/Apply pixels, save/reopen, stable IDs and Undo. Incoming,
linked-handle and new-key MCP moves remain unqualified.
