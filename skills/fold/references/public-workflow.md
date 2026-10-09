# Public authoring workflow

## Connect and discover

Use the explicitly enabled socket for the open project session. On Linux the host
starts with `fold-desktop --protocol-dir <directory> [--project <project.fold>]`
and reports its private socket path. MCP clients launch `fold-cli mcp <socket>`.
The MCP transport negotiates version `2025-11-25`; Fold tool arguments still require
`version: 2`. `tools/list` exposes `fold_request` with the same schema returned by
discovery. CLI clients send those same arguments on stdin to
`fold-cli protocol <socket>` and receive the Fold response on stdout.

Call `fold_request` with:

```json
{"version":2,"command":{"method":"discover"}}
```

Use `epoch` from the response envelope and `revision`, `documents`, `catalog`,
`request_schema` and `limits` from `result`. Select an existing supported native
document. Unknown documents stay opaque. Public document creation and opening
project files are not implemented by this adapter. Saving a copy is described below.

## Jobs and inspection

Preparation, inspection, observation and project saving may return
`{"state":"running","job":...,"scope":...}` inside `result`. Keep the returned
job object intact and send `{"version":2,"command":{"method":"job_status","handle":JOB,"wait_ms":1000}}`
until the response contains the final result or error. `JOB` is a substitution,
not a literal JSON token. Use bounded polling; `wait_ms` is optional (default zero) and allows at most
25,000 milliseconds. A wait deadline returns running status without cancelling
work. Transport timeouts do not imply rollback.

Inspect a committed document with `inspect_document`, providing its `epoch`, exact
`revision` and `scope: {"document":DOCUMENT_UUID,"definition":null}`. For asset
scopes use the definition UUID returned by inspection. The canonical source is in
`result.source`. Obtain operation/input details from the discovered schema/catalog.

## Exact-time frames

Check `result.frame_observation.available` in discovery. A GPU-enabled host with
an attached shared rendering device exposes `observe_frame`; CPU-only sessions
report it unavailable. Use the document/output UUIDs from public inspection:

```json
{
  "version": 2,
  "command": {
    "method": "observe_frame",
    "target": {"kind": "committed", "epoch": "EPOCH_UUID", "revision": 1, "document": "DOCUMENT_UUID"},
    "output": "OUTPUT_UUID",
    "time": {"numerator": 1, "denominator": 4},
    "dimensions": [480, 270]
  }
}
```

Replace placeholders and revision with inspected values. For an immutable proposal,
use `target: {"kind":"candidate","handle":HANDLE}`. Poll the returned job as above.
Frame observation does not grant an Apply receipt or move artist playback/selection.

Frame time is output-relative: zero means canvas start, and duration itself is
excluded. Provenance records requested `time`, absolute `scene_time`, root
`asset_time`, revision/candidate, camera, font fingerprints, configuration and display
transform. CPU `observe` uses absolute scene times; its measurements remain distinct
from frame availability. For the four-second integrated demo, exact time four is
still outside the registered frame endpoint.

`result.image` contains opaque viewer-display RGBA8 pixels, encoded as `rgba8_hex`,
with dimensions and top-to-bottom row order. Decode losslessly to an unused file
for visual inspection; do not apply another display transform. With Pillow:

```python
from PIL import Image
image = response["result"]["image"]
Image.frombytes("RGBA", tuple(image["dimensions"]), bytes.fromhex(image["data"])).save("frame.png")
```

Dimensions are positive, at most 4096 per axis and 2,073,600 total pixels. One worker
admits at most two frame jobs; use the existing scoped `cancel_job` for cancellation.
For a full-resolution crop, keep `dimensions` equal to the full raster and add
`"region":{"x":37,"y":43,"width":361,"height":192}` to the command. The rectangle
must be nonempty and inside that raster; bounds use top-left pixel coordinates.
The full raster remains subject to the same pixel budget. Cropping preserves camera
projection and sampling resolution. Returned image dimensions and `output_size`
describe the crop; provenance `canvas_size` and `region` locate it in the full frame.
Omitting `region` returns the full raster. Contact sheets, clips, transparent linear
delivery are not exposed here.

## Saving a project copy

Use the current epoch and committed revision from discovery, a fresh request UUID,
and an absolute unused destination whose parent directory exists:

```json
{
  "version": 2,
  "command": {
    "method": "save_project_copy",
    "guard": {"epoch": "EPOCH_UUID", "request_id": "FRESH_REQUEST_UUID", "base": 4},
    "path": "/absolute/new-project.fold"
  }
}
```

The project guard has no document `scope`. Poll the returned job (`scope: null`)
until completion. Saving captures that exact committed revision, including opaque
extension data; later artist edits cannot change its contents. Existing files and
symlinks produce `conflict` and remain intact. The live session's file path, dirty
state and undo history remain unchanged.

Success reports `state: "saved"`, `path`, `revision`, `durable`, `warning` and
`session_file_changed: false`. If `durable` is false, the archive was published but
directory synchronization failed; retain and report the warning. Exact retries
reuse the original request bytes and UUID and return the original job/result even
after the live revision advances. A changed destination requires a fresh UUID.

To cancel, send `cancel_job` with the returned handle and a fresh project guard at
the current revision, again omitting scope. Cancellation before publication leaves
no destination. Publication wins a cancellation race: a saved result remains saved,
and a late cancel returns `already_committed`. A transport timeout is not proof
that saving failed; poll or retry the exact request to establish its outcome.

## Verified seed-edit round trip

The public MCP subprocess test starts with an open native construction whose seed
is zero. Adapt these substitutions to discovery and inspection; never assume an
existing artwork's seed is zero. `GUARD` contains the discovered epoch, a fresh
UUID in `request_id`, current integer `base`, and explicit document scope. Replace the UUID placeholders
and the example base `1` with those actual values.

```json
{
  "version": 2,
  "command": {
    "method": "propose",
    "guard": {
      "epoch": "EPOCH_UUID",
      "request_id": "FRESH_REQUEST_UUID",
      "base": 1,
      "scope": {"document": "DOCUMENT_UUID", "definition": null}
    },
    "edit": {
      "kind": "patches",
      "patches": [{
        "path": [{"kind": "field", "value": "seed"}],
        "expected": {"kind": "present", "value": 0},
        "value": {"kind": "present", "value": 123}
      }]
    }
  }
}
```

After the proposal job completes, retain `result.handle`. Call `inspect_candidate`
with that handle, poll its job, and review the source and changes. Its successful
result includes `receipt`. Call `apply` with a fresh guard at the current revision,
that exact handle and receipt. Apply returns the committed revision. Retrying
identical Apply arguments returns the original result, including after artist undo.

If Apply conflicts because the artist changed the project, discover the current
revision and call `rebase` with a fresh guard and the old handle. Inspect the new
handle before applying. Same-property conflicts require a deliberate new edit;
protected controls remain protected. Candidate observation does not supply a receipt.

## Qualification

For an explicitly requested new integrated Motion demo, use
[`demos/m5-integrated.py`](../../../demos/m5-integrated.py) against an untouched
native Motion starter selected by its discovered document UUID:

```sh
python3 demos/m5-integrated.py /path/to/fold-cli /path/to/socket DOCUMENT_UUID /new/report.json
```

Append `mcp` to run the same workflow through the actual MCP subprocess instead
of CLI protocol requests. It negotiates `2025-11-25`, discovers `fold_request`,
and checks its input schema against Fold discovery before authoring.

The script refuses non-starter artwork and existing report/project destinations.
It saves the restored baseline publicly to `<report-path>.fold`. It discovers the
catalog, inspects Source, proposes the complete construction, inspects its
candidate, applies with the receipt, verifies an exact Apply retry and observes
seven times from zero through four seconds. Polling has a deadline; expiry does
not cancel a remote job. Its artwork authoring reads no Rust crates or project payloads.
It then changes wording, grows the grid to 21 columns and edits the first Palette
stop, checks three observation times, and restores the baseline through another
guarded proposal. All native Source IDs and surviving clone rotations must remain
stable; restored canonical Source must match exactly. The report retains both
baseline and variation observations.
The resulting 35-node construction includes animated text shared with extrusion,
O/B/8 holes, a field/ramp-driven cloner, named Random rotation, camera/light and
ordered Screen graphics. Native inputs expose wording, timing, grid, field,
extrusion, camera and title color; the Palette node retains editable color stops.
An explicit Asset Time node supplies Tween clocks.

Every operation has an explicit stable-ID presentation position. The graph has
separate lanes for background, animated title and extrusion, hole fixtures,
field drivers and clone responses, with camera/light feeding the final scene.
The script checks complete position coverage and candidate metadata equality;
control variations and restoration preserve that layout. Reports include the
verified `presentation` object. Native normal/narrow visual review remains open.

Completion for this workflow means the report verifies font provenance, shared
text layout/word transforms, independent word timing and cloner scale/color
formulas, and stable instance rotations across all observed times. The report
contains canonical Source, CPU measurements and the public save result. The
separate `.fold` archive contains the saved project; neither artifact alone proves
rendered appearance. The integration harness reopens the publicly saved archive
and compares complete project state; variation save and Undo/Redo use native services. A separate GPU regression loads the saved project and renders the
seven required times with reviewed captures; four seconds uses native image
evaluation because registered video output is half-open. Both CLI and MCP authoring
tests pass these checks, including control variations and native Undo/Redo and
save/reopen in the harness. The saved baseline/variation pair also passes a
headless GPU check at one second: wording and palette visibly change, restoring
the baseline reproduces its pixels, and the retained original frame stays intact.
Both captures were reviewed. When discovery reports frame observation available,
the authoring script also records seven 480x270 baseline display frames at
0, 1/4, 1/2, 3/4, 1, 2 and 95/24 seconds, plus variation and restoration frames at
one second. It checks revision/time provenance, opaque RGBA bytes, visible change
and exact restored pixels. `report.frames` retains images and provenance for
review using the decoding example above. The 95/24 frame is explicitly labelled;
it does not replace the exact four-second CPU observation or change the half-open
frame contract. CPU-only sessions record `frames.available: false`.
Public copy saving passes CLI/MCP retry, concurrent-edit, no-replacement and native
reopen checks. Physical desktop interaction and complete artist lock/key/layout
handoff remain unqualified.

The executable public test `crates/app/tests/agent_mcp.rs` verifies initialization,
shared schema, discovery, job polling, proposal, inspection, Apply and exact retry
through the actual MCP subprocess/socket. CLI/socket tests verify discovery parity,
wire-version rejection and exact-byte retry conflicts. Broader source/lock/rebase
semantics have dispatcher tests; full handoff and physical native artist acceptance
remain incomplete. Report these distinctions with any result.
