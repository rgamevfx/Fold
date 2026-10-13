# Public workflow

## Connect and discover

On Linux, start `fold-desktop --protocol-dir DIRECTORY [--project PROJECT.fold]`.
Use its reported private socket. MCP runs `fold-cli mcp SOCKET`; CLI accepts the
same request JSON on stdin through `fold-cli protocol SOCKET`. MCP negotiation
uses `2025-11-25`; Fold requests use the connected build's discovered schema:

```json
{"version":2,"command":{"method":"discover"}}
```

Keep the returned `epoch` and `result.revision`. Read `documents`,
`scoped_proposals`, `project_authoring`, `request_schema` and `limits` as needed.
Current Motion/3D providers are `fold.motion.canvas` and `fold.3d.scene`.
Composition, Compositor and Timeline are separate providers. Discover rather
than assume availability, schema versions or output names.

## Inspect → propose → inspect candidate → Apply

1. Send `inspect_document` with `epoch`, `revision` and
   `scope: {"document": DOCUMENT_UUID}`. Losslessly parse `result.source`;
   provider payload is under `native`. Outputs/image inputs are derived.
2. Build typed edits from the discovered provider schema. Retain existing object,
   point, parameter and key IDs; copy exact expected values. Native Motion authoring:

```json
{
  "version": 2,
  "command": {
    "method": "propose",
    "guard": {
      "epoch": "EPOCH_UUID", "request_id": "NEW_REQUEST_UUID",
      "base": 1, "scope": {"document": "DOCUMENT_UUID"}
    },
    "edit": {
      "kind": "native",
      "request": {
        "op": "author", "document": "DOCUMENT_UUID",
        "expected": "INSPECTED_PROVIDER", "edits": ["TYPED_EDIT"]
      }
    }
  }
}
```

UUIDs, revision and quoted uppercase values are placeholders; replace expected
and edit placeholders with full JSON records, not strings. See [native recipes](native-recipes.md).

3. Poll any returned job; retain its completed candidate handle.
4. Send `inspect_candidate` with the handle. Review source/diff and retain the
   receipt. Preparation leaves committed artwork unchanged. Review candidate
   pixels where available before committing.
5. Send `apply` with handle, receipt and a fresh guard at the current revision.
   Apply creates one Undo entry. An active artist gesture causes a conflict;
   finish it before a new request. Discover the new revision after Apply.

An exact retry uses the **same UUID and identical bytes**. A changed request gets
a new UUID. For concurrent Motion/3D changes, inspect and prepare again; these
providers reconcile unchanged or already-identical records. Follow the discovered
reconciliation capabilities for other providers.

## Jobs, project edits and saving

Running responses return `result.job`. Keep that handle intact and poll:

```json
{"version":2,"command":{"method":"job_status","handle":"JOB_HANDLE","wait_ms":1000}}
```

Replace `JOB_HANDLE` with the returned object. A wait timeout leaves the job
running; it neither cancels nor establishes success.

Multi-document proposals use `edit: {"kind":"project","edits":[...]}` and the
`project_authoring.schema`. Discover document creation, composition layers,
provider authoring and connections; inspect all changed documents before Apply.
Published outputs retain their provider's port identity.

`save_project_copy` saves committed artwork to a new absolute path. Use a
project guard (`epoch`, fresh `request_id`, current `base`, **no document scope**).
Existing files are not replaced. Poll until the save result confirms publication;
a transport timeout alone does not establish whether the file was written.

## Frame review

Check `frame_observation.available`; rendering requires the attached GPU host.
For committed output:

```json
{
  "version": 2,
  "command": {
    "method": "observe_frame",
    "target": {
      "kind": "committed", "epoch": "EPOCH_UUID",
      "revision": 1, "document": "DOCUMENT_UUID"
    },
    "output": "video", "time": {"numerator": 1, "denominator": 2},
    "dimensions": [640, 360]
  }
}
```

Use the discovered output name and time inside its half-open duration. For an
immutable candidate, replace `target` with
`{"kind":"candidate","handle": HANDLE,"document": DOCUMENT_UUID}`. Explicit
`document` is required, including for the edited document. It can name a downstream
Compositor or Timeline output in the same candidate project without changing the
proposal's write scope.

Poll the job. Decode `image.encoding: "rgba8_hex"` into RGBA bytes; rows run
top-to-bottom. Pixels are opaque **display-space diagnostics**, not linear HDR
or transparency exports. Inspect returned provenance and actual images before
claiming visual quality. `region: {x,y,width,height}` crops the raster defined by
`dimensions`; it does not reframe the camera.

For exact multi-pose review, add to the command:

```json
"review": {
  "times": [{"numerator": 0, "denominator": 1}],
  "layout": {"kind": "contact_sheet", "columns": 2}
}
```

Primary `time` comes first, followed by supplied poses from one immutable
snapshot. `{"kind":"onion"}` produces an equal-weight display-pixel average.
Read `review.frames` for per-pose time, provenance and hashes. Discovery owns the
limits; reduce dimensions for many poses. Current qualified limits are 2–16 poses,
2,073,600 total source pixels and 4096 pixels on either output axis; sheet padding
also counts against output pixel limits.

## Parameters and exact time

Use `inspect_parameters` for stable targets, `resolve_parameter` for path lookup
and `sample_parameter` for values. Keep returned placement-owner IDs for repeated
uses. Composition/Timeline map source time before sampling. Time is a normalized
`{numerator:N, denominator:D}` record. Native Motion distances use canvas units;
3D uses world units and coupled vector/quaternion keys.

Motion script controls belong to the script. Composition publication binds its
layer/effect controls, not arbitrary Motion inputs across documents. Discover
capabilities for expressions and geometry observations rather than assuming them.

## Lossless JSON guards

Expected-document checks distinguish `1` from `1.0`. Preserve inspected number
representations in whole expected envelopes. On a JavaScript runtime supporting
`JSON.rawJSON` and reviver source context:

```javascript
const expected = JSON.parse(inspected.source, (key, value, context) =>
  typeof value === "number" && /[.eE]/.test(context.source)
    ? JSON.rawJSON(context.source)
    : value
);
```

Serialize this object without an ordinary JSON reparse that loses the distinction.
Changed values still need the schema's numeric type. Source-string edits retain
exact inspected text. If the client cannot preserve numeric representations,
use a lossless transport rather than retrying a mismatched guard.

## Verification boundary

These routes have real CLI/MCP fixture evidence, not a claim of universal native
artist acceptance: [candidate frame](../../../resources/motion-context/mcp-frame-evidence.json),
[multi-pose](../../../resources/motion-context/animation-review-evidence.json),
[downstream candidate](../../../resources/motion-context/mixed-candidate-evidence.json)
and [historical numeric guards](../../../demos/Fold-version-1/evidence.json).
Examples here retain those public request shapes; discovery remains authoritative.
