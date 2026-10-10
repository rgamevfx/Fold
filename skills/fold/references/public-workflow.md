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

Use `epoch` from the response envelope and `revision`, `documents`, `scoped_proposals`,
`request_schema` and `limits` from `result`. Select an existing supported native
document. Unknown documents stay opaque. Public Composition, scene and compositor creation
are described below; opening files remains an application action. Saving a copy
is described below.

## Parameter references

Select the Motion entry in `scoped_proposals` by `type`, then check
`requests.catalog.parameter_references`. The current M5c implementation supports
references within one Motion graph definition, including nested instances of that
definition, and numeric component references within one Compositor document.
Cross-document expression references are not yet available. Project-wide
inspection paths are a separate capability described below.

Expose useful artist controls as container `input` declarations. Network displays
them together on **Channel References**; its outputs follow input IDs, names and
order. Connect directly to those inputs. Use inline `expr(...)` for arithmetic
and inline `track(...)` bindings for animation instead of generating primitive
nodes solely to carry parameter values.

For a node with `width` and `height` distance parameters, this native source binding
sets height to one quarter of its own width:

```text
"height" @EXISTING_PARAMETER_METADATA = expr(ch("width") * 0.25);
```

Retain the actual metadata from inspection in place of the placeholder. The real
CLI/MCP regression `cli_and_mcp_author_relative_parameter_expressions_without_primitive_nodes`
executes this edit through discovery, source inspection, proposal, candidate
inspection, Apply and undo/redo. It reads 50 canvas units from a width of 200 and
verifies that no nodes were added.

Relative paths start at the owning node. Use `ch("../controller/width")` for a
sibling node and `ch("../Width")` for a container input. The currently qualified
absolute form is `/definition/node/parameter`, confined to that definition.
Vector/color/quaternion components use `/x`, `/y`, `/z`, `/w` or color aliases
`/r`, `/g`, `/b`, `/a`. Escape a name segment using `~0` for `~`, `~1` for `/`,
and `~2` for `.`. Keep structural separators unescaped.

Resolved references retain parameter identities. Inspect fresh canonical source
after rename; it prints current target paths. Types and units are checked, and
cycles or missing targets reject proposals. Reusable instances read their own
inputs and local clock. Preserve protected parameters and connection identities.
Do not substitute zero for a failed reference or build fallback primitive graphs.

## Parameter discovery and sampling

Use `inspect_parameters` with `epoch`, `revision`, `document` (a UUID or null for
all supported documents), `offset: 0` and `limit: 256`. Poll the worker job.
`result.parameters` contains stable `target` objects, unescaped `path` segments,
`value_type` and `editable`. Continue at `result.next` until it is null. Keep the
same revision for all pages. Native Motion occurrences, scene placement controls and
object graphs, Compositor primitive controls/channels, Composition layer controls,
and Timeline track/clip controls currently participate. Unused Motion definitions,
reusable-effect placements and full structured parameters are not yet covered.
`target.parameter` is either `{"kind":"native","id":UUID}` or
`{"kind":"property","name":KEY}`. Property keys are fixed provider contracts,
not artist labels or JSON paths. Preserve the complete target from inspection. A null `value_type` denotes a structured control; it cannot be
sampled as one primitive.

To resolve a path, call `resolve_parameter` with `epoch`, `revision`, `owner`
(the unescaped path segments of the owning node/container) and `path` (an escaped
relative or absolute string). Absolute discovery paths start with the project
**document name**. Nested Composition layer and Timeline clip sources add their owner
path and source document name. These are distinct from the definition-local `ch()`
paths above. Clips currently have no authored name; their path segment contains the
full stable clip ID.
Duplicate names produce an ambiguity error. Use the returned stable target for
sampling and retain it across renames; refresh display paths from inspection.

Call `sample_parameter` with `epoch`, `revision`, the full inspected `target`
and an exact rational `time`, for example `{"numerator": 1, "denominator": 2}`.
The result contains `value`, `target`, `time` and `revision`. Without a placement,
time is authored **target document time**, not output-relative frame time. With
`target.placement`, time is authored **placement root time**. Preserve its root and
ordered owner IDs; these distinguish repeated uses of one source. Composition and
Timeline source mappings apply before the source canvas start and Motion object/
instance clocks. Sampling inspects controls even outside a visible layer/clip range;
it does not return a held or black video frame. Compositor and Composition use their
native configuration and animation samplers.
Missing targets, invalid components and stale revisions return errors.

The CLI/MCP parameter-expression regression also performs discovery, relative
lookup and sampling through these public methods, verifies 50 canvas units, and
checks rejection of a stale revision. The layer/Timeline public regression verifies
property targets and nested source discovery/sampling through both transports. These read tools do not authorize or implement
cross-document expression links. Continue to edit through inspected native source
and the existing proposal/Apply workflow.

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


## Earlier Construction starter

The current untouched Motion starter has `Rectangle`, `Placement`, `Scene` and
`Rectangle fill` nodes. `demos/m5-integrated.py` now checks that inspected source
before replacing its reserved starter. Its CLI and MCP CPU authoring tests pass
with this starter (8 October 2026). That existing 35-node Motion demo is M5 evidence;
it is not the required M5a public-authored mixed-scene/title demo.

## M5 catalog trials and local demo

Run `./demos/run-m5-demo.sh` from the checkout to build the desktop and open a
fresh editable copy of the integrated project. The [demo guide](../../../demos/README.md)
lists the controls to try. This is a repository launcher, not a public protocol method.

The same `m5-integrated.py` invocation above now runs eight additional guarded
Source trials before the wording/palette handoff. `report.native_modes` records
canonical Source, CPU observations and, when available, a public GPU frame for
each trial. Every trial restores baseline Source. Discovery supplies all ports,
defaults and versions; the authoring script reads no implementation files.

| Operation | Qualified mode and boundaries |
| --- | --- |
| Stroke | `solid_centered_butt_miter`, `solid_centered_butt_round`, `solid_centered_round_miter`, `solid_centered_round_round`; centered width in canvas units |
| Transform | `screen`: contours/Outline Text, canvas translation/pivot, Z angle, positive XY scale; path coordinates change, Stroke width stays in canvas units |
| Transform | `world_mesh`: metric mesh, translation/pivot, quaternion rotation and positive XYZ scale; inverse-transpose normals, preserved vertex/face IDs and UVs |
| Transform | `world_instances`: shared instances, World translation/pivot, Z angle and positive uniform scale; prototype/instance identity retained |
| Tessellate | `planar`: closed contours/Outline Text, explicit nonzero/even-odd choice, curve detail and meters per canvas unit; World mesh output |
| Extrude | `closed_profile`: the same profile inputs plus positive metric depth and depth segments; front/back/sides regions |

Profile conversions generate +Z cap normals and canvas-XY UVs. Extrusion transfers
UVs through the reported output-vertex-to-cap-vertex map, duplicating vertices at
hard normal boundaries. `source_geometry` records the input geometry identity.
Appearance is not implicitly converted to a spatial Material. Assign Material
and World Placement explicitly. Topology IDs change when connectivity changes;
changing depth without changing segmentation preserves correspondence.

Instance fields sample positions from the selected upstream geometry, including
an earlier Transform. Original distribution identity is provenance, not a request
to sample the original positions. Word observations expose complete glyph groups,
advance-center pivots and staggered translation from one retained text layout.

These checks qualify the documented modes and public workflow. Native pointer/
keyboard handoff, full resource/cancellation bounds, picking and general spatial
transparency remain open in the rewrite acceptance audit.

Profile conversion retains source outlines and packaged font provenance in public
observations even without a separate Text placement. Outlined glyphs remain
separate profile partitions; overlapping glyphs are not implicitly Boolean-unioned.
Closed contour inputs share the explicitly selected fill rule.

## Compositor parameter links and expressions

Discover `project_authoring.providers` for `fold.compositor.composite`; its catalog
advertises `expression` and `reference` author edits. Use the ordinary guarded
project `author` operation with the exact inspected document as `expected`.
Numeric property components, Mix and shader controls accept the shared bounded
expression IR. Enum/boolean configuration fields and cross-document bindings are
not exposed as writable expressions in this slice.

The following author edit is covered by the public provider integration test:

```json
{"op":"expression","node":"TARGET_NODE_UUID","property":"gain","component":0,"source":"ch(\"../Source/color/r\") * 2"}
```

A direct link uses `op:"reference"`, the same target fields, and
`source_node`, `source_property`, `source_component`. Discover property paths from
revision-bound parameter inspection; resolve names uniquely. Stored references
retain native identities through node renames. Inspect the candidate and apply
its receipt normally. Cycles and missing targets reject the edit; time-dependent
runtime errors remain explicit evaluation errors. Source round trips retain native
channel bindings and edge extension data. The native inspector provides Copy
Parameter, Paste Relative Reference, Edit Expression, Go to Source and Remove
Reference on component context menus. Removing a reference retains its sampled value.

## Composition and object graphs (M5a)

Discovery's `project_authoring.schema` describes the host's `create`, `add_layer`
and `connect_layers` operations inside `edit: {"kind":"project", "edits":[...]}`.
Provider-owned operations are described by `scoped_proposals[].requests`, matched
by the inspected document's `type`. The current provider request version is 1;
the host binds it to wire version 2. The transport envelope and retry rules are
unchanged. Motion's source/patch schema is `requests.graph`, scene operations are
`requests.scene`, and Compositor/Composition operations are `requests.edits`.
The Motion operation catalog is `requests.catalog` on its provider entry; there
is no top-level creative catalog. Unknown operations and provider schemas fail.
Compositor retains its provider source limit of 1 MiB; the total request limit
still applies to expected and proposed text together. Motion source retains
its 64 KiB limit.

For example, after `discover`, select the Motion catalog without reading crates:

```python
motion = next(p for p in discovery["result"]["scoped_proposals"]
              if p["type"] == "fold.motion.construction")
operations = motion["requests"]["catalog"]["operations"]
```

C4 verifies this discovery path through both CLI and MCP handoff tests. Provider
preparation retains immutable source/diff content. Changed dependencies reject
rebasing; successful disjoint rebases require a fresh candidate inspection and
receipt. A failed member of a mixed proposal publishes nothing.
Use the ordinary guarded proposal, candidate inspection and Apply lifecycle.
`create` takes a new document UUID, a discovered supported `type_id`, and a name.
`add_layer` takes a Composition UUID and `kind: "scene"` or `"image"`.
Generated layer/document/object IDs are returned by candidate inspection; never
invent or infer them from stack position. Several operations publish in one undo entry.

For Composition and scene inspection, parse `result.source` as JSON. Its `native`
field is the owning provider's record, `outputs` declares exact output references,
`image_inputs` supplies exact named image sockets, and scenes include canonical
`object_sources` keyed by stable object UUID.
`author` and `hierarchy` edits require the complete inspected JSON envelope in
`expected`; provider operation schemas are under `project_authoring.providers`.
`connect_layers` binds an explicit named image input between Composition layers.
Layer ordering alone never creates that dependency.

Inspect one object's graph with `scope: {"document":SCENE_UUID,"object":OBJECT_UUID,
"definition":null}`. Use the resulting canonical source as `expected` in the
ordinary `kind:"source"` proposal, retaining that exact scope through rebase and
Apply. A definition UUID further narrows the graph scope. Project batches can also
use `object_source` with document/object UUIDs and exact expected/source text.
Scene hierarchy, sibling graphs and inherited locks remain owned by the scene.
Agent requests cannot unlock objects or controls. Rebase can merge disjoint scene
properties and native graph edits; conflicting topology changes require a new proposal.

`observe_frame.output` is the exact **string** declared by the provider: scene
outputs are UUID strings, while Composition currently declares `video`. Mixed
frames identify the immutable project fingerprint and output-relative time; they
do not claim one camera/asset clock or verified external-resource content hashes.
CPU `observe` still measures Construction outputs only.

Verified example: `demos/make-it-move.py` builds three scene layers from an empty
public session, authors separate graphs, keys the title in Composition, inspects
frames and saves a copy. It requires GPU rendering, the packaged color runtime
(`FOLD_COLOR_ROOT`) and Pillow for PNG decoding. No native click/drag is involved.
The saved `demos/Make-it-move.fold` has a two-node backdrop graph, five-node title
graph and seven-node sphere graph, plus separate camera/light graphs per scene.
The historical zero-time image was mislabeled: its sphere region is byte-identical
to the held frame. See the [pixel audit](../../../docs/rewrite/evidence/m5a-compositor-cutover/observation-audit.json); no renderer fix is claimed.


## Native compositor source (schema 8)

Compositor inspection returns `graph_source` alongside its `native` record and
named `image_inputs`. Provider discovery lists the 24 current operation/version entries
and their native `record` examples. Grade, Transform, Blur and Merge use version 3.
Read and Shuffle use version 2. Shuffle's required image input is B; A is optional.
Read exposes separate before/after policies and explicit proxy selection. Write v1
exports its own connected branch to EXR from the desktop Render action; Output
remains the internal Fold endpoint. Public branch references use `write/<node-id>`.
Discover the current catalog before constructing settings; public agent export
commands and a persistent render queue are not yet qualified.
Blur provides Gaussian and Box filters. Grade provides explicit clamps/inversion;
Transform provides ordered XY skews, reconstruction boundary controls and exact
shutter sampling of its own animation. Upstream images remain at the requested
time, and output stays inside the composition format. Old operation records remain opaque and cannot execute.
Example IDs are placeholders: allocate fresh
node/parameter IDs for creation; preserve inspected IDs when editing.

Use a project edit with `op:"compositor_source"`, `document`, optional `nodes`,
exact `expected` source and replacement `source`. A node scope is a JSON array of
those native nodes, preserving membership. Full source includes the complete
node array; format and output selection use the corresponding structured edits. Both go through the ordinary proposal → inspection → Apply lifecycle.
For provider structured edits, `create_node`/`replace` use `record`; static values
are typed literal/list bindings or registered domain configuration. Animation
and overrides carry native parameter/track/key identities. Use the discovered
contract; the former Parameters-tag payload and old document schemas do not run.

The verified [mixed-scene workflow](../../../demos/m5a-mixed-scene.py) starts with
an empty public session and saves [m5a-mixed-scene.fold](../../../demos/m5a-mixed-scene.fold).
It creates a sphere/plate scene, a Grade compositor explicitly bound to that
scene, and a transparent title scene. It round-trips scoped compositor source,
observes start/reveal/held frames and checks the unchanged sphere region. Run it
with the same four arguments and packaged runtimes as `make-it-move.py`.
