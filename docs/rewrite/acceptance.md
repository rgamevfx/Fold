# Frozen first-slice acceptance specification

These are required scenarios, not passing results. The implementation must expose
them as executable tests and public-tool/native runtime runs before phase gates close.

## M2/M3 construction

`animated-shape`: native asset with protected count input, color input, rectangle,
Time, bounded expression, vector Tween from (0,80) to (0,0) over 3/5 second,
Keyframes opacity at 0, 1/2 and 1 second, Screen Place, Scene and named Output.
Separate immutable instances override color and local clock without changing the
definition. At 0, 1/4, 1/2, 3/5 and 1 second, compare forward/reverse/random seek
samples exactly for identities and within 1e-9 for CPU scalar results.

`save-reopen`: parse, canonical print, parse, save/reopen and undo/redo preserve
semantic IDs, unknown extension fields, graph positions, annotations, tracks and
asset overrides. Unsupported operation/version records survive but cannot evaluate.
Unsupported old documents preserve byte-identical payload with explicit diagnostic.
Malformed source retains its buffer and leaves the last valid snapshot unchanged.
Cycle and recursive asset fixtures report the participating IDs.

`handoff`: source edit, node/control edit, timeline key edit, artist ramp edit,
count lock and graph rearrangement precede a disjoint agent field/timing proposal.
After apply, all unrelated artist edits survive. A same-key race conflicts. Parent
replacement and asset override cannot bypass the count lock. Observe does not move
artist clock/selection. Expired/cancelled/wrong-generation apply fails. Identical
retry does not create a second undo entry. Reused request ID with changed content
fails. Repeat through CLI and MCP and compare normalized responses and schemas.

## M4/M5 exact visual scene

1920×1080 canvas, 24 fps, duration 4 seconds, seed 20261008. Packaged
`resources/fonts/NotoSans-Regular.ttf`, text `MAKE IT MOVE`, size 140 canvas units.
Record the font content hash with every run. Text word groups enter from (0,80)
to (0,0), cubic-out duration 3/5 second and 7/100 second word delay. Screen text
uses orange fill (#FF5A36 decoded to linear) and a 2 canvas-unit white stroke.
The same shaped layout feeds Outline Text → Extrude (0.15 m) → lit Material →
World Place. Include an `O B 8` contour fixture so holes cannot pass accidentally.

Grid: 20×12 instances of one 0.06 m box prototype, 0.12 m spacing. Radial field
center moves diagonally from (-1,-0.5) to (1,0.5) in 2 seconds. Radius 1 m; clamped
linear ramp maps influence to scale 0.2…1 and orange/blue color. Named random stream
`rotation` yields fixed Z rotation per cell. Growing the grid preserves surviving
cell IDs and random values; one shared prototype remains resident.

Perspective camera at (0,1,5) looks at origin, vertical FOV 45°, near 0.01 m,
far 100 m. Directional white light with declared illuminance and fixed transform;
1000 lux for the directional source, verified against the adapter units before visual qualification. Ordered Screen background and foreground marks test
spatial pass composition. Expose wording, palette, entrance duration, word delay,
extrusion depth, count, spacing, field center/radius and camera framing.

Inspect times 0, 1/4, 1/2, 3/4, 1, 2, 4 seconds and wording/count/palette variations.
Perform the handoff, save/reopen, canonical print and continue through public tools.
Native UI, CLI, MCP and headless must render this construction, never a replacement
image. One normalized text layout supplies vector, spatial and outline branches.

## Rendering qualification and measurement

Linear working Rec.709/sRGB primaries, premultiplied RGBA16F intermediate, transparent
background fixture, sRGB display conversion exactly once, fixed no-history profile
initially. No stochastic antialiasing. Compare CPU linear reference color swatches
within 2e-3 absolute/channel; alpha within 2e-3. Compare headless and desktop
readback under identical settings within 1/255 on interior display pixels and one
pixel contour distance on edges. Any exception requires evidence, not tolerance
relaxation to hide errors. Depth tests require correct front/back ordering of known
occluders; screen marks retain paint order. Camera Plane sizing has known projected
bounds within one output pixel.

Vector fixtures: nonzero/even-odd nested contours, holes, self overlaps, clipping,
round/butt caps, round/miter joins, thin strokes, 0.25×/1×/4× zoom. Outline area and
hole coverage must match source contours. Report tessellation attribute transfer.
Rapid edits and cancellation may not present frames for a stale revision or release
resources before GPU completion. Device-loss recovery rebuilds caches from snapshot.

Hardware and backend versions must be recorded before timing runs; no unnamed
hardware result qualifies. Fixed benchmark variants: this small scene, 1k/10k/100k
clones, changing text, overlap, deformation and mixed screen/world. Keep counts,
fonts, quality and color fixed. Record cold and warm parse/validate/compile/evaluate/
prepare/submit/present/readback/encode separately, P50/P95/P99, memory and missed
presentations. M6 targets remain ~100 ms small source edit, a few hundred ms frame
observation, and 1080p60; M5 supplies correctness rather than performance acceptance.

## Evidence still required

Current implementation evidence is tracked in the rewrite plan. Dependency probes,
native fixtures and selected UI/transport checks now exist; complete native artist
interaction and rendering qualification remain outstanding. No M0–M5 gate is passed
by this specification alone.

## Executable first-slice oracle

`fixtures/animated-shape.json` fixes the semantic construction independently of
language spelling; it is not an executable legacy asset or a new public schema.
`python3 docs/rewrite/verify_slice.py EVIDENCE.json` checks actual captured results
against rational sample expectations, seek identity, canonical/save/undo round trips,
unknown data, handoff preservation, locks/conflicts, retry and transport parity.
The evidence object keys in the verifier define the capture contract. Snapshot
objects must contain the complete normalized semantic and presentation state,
excluding only publication revision counters. Capture errors with complete before
and after snapshots. Do not fill these fields with fixture expectations or booleans
claiming success. M2/M3 tests must produce them from actual service/CLI/MCP actions.
M0 freezes this executable specification. The [native capture](evidence/m3-native-slice.json)
now records actual seek samples, canonical/persistence snapshots, undo/redo, invalid
Source retention and unknown-data preservation. Its [canonical Source](fixtures/animated-shape.foldsrc)
is available for the public handoff. `verify_native` checks this portion without
qualifying the full gate; `verify` still requires provenance, handoff, errors and
transport evidence. Corrupting a sample or reopened snapshot rejects. The full
verifier rejects that partial capture, as required. The combined capture below
now supplies the remaining automated evidence.

Regenerate the native capture with
`FOLD_SLICE_CAPTURE_DIR=/tmp/fold-slice cargo test -p fold-motion capture_frozen_slice_native_evidence --locked --offline`.
This writes actual results to `native-evidence.json` and `animated-shape.foldsrc`;
it does not fill missing public/native acceptance fields with expected values.
The M5 visual scene and native interaction still require separate recorded runs.

The [transport capture](evidence/m3-transport-slice.json) now runs the canonical
animated shape through actual CLI and MCP processes. It records matching schemas,
forward/reverse/shuffled CPU observations and the complete reopened Source after
native Source/key/layout edits, Count locking, candidate conflict/rebase/inspection,
Apply/retry and Undo/Redo. This replaces the prior starter-scene transport fixture.
It does not yet include ramp/field handoff or the full oracle error/provenance object.
Regenerate with `FOLD_SLICE_CAPTURE_DIR=/tmp/fold-slice cargo test -p fold-app --test agent_handoff --locked --offline`;
the test requires permission to create its private temporary local Unix socket.

The transport capture also retains complete before/after construction snapshots for
ten matching rejection cases: count override/parent lock bypass, same-key conflict,
wrong-generation receipt, cancelled candidate, changed retry identity, node cycle,
recursive asset, unsupported operation and old protocol version. Every capture
asserts unchanged revision as well as complete document equality. Expired-candidate
capture, ramp/field handoff and complete provenance remain missing; these records
must not be reported as a full oracle pass.

The [expiry capture](evidence/m3-expiry-slice.json) exercises the version-2 dispatcher
with an explicit monotonic time past the candidate idle deadline. Apply returns
`expired`, preserves the complete frozen construction/revision, and exact retry
returns the same failure. It records source-content build identity and target/profile.
This is controlled-clock service evidence, not ten-minute elapsed CLI/MCP evidence.
Combining this record with either transport's rejection records passes the unchanged
`verify_errors` assertions; corrupting the expired request's after-snapshot rejects.
That expiry record alone does not satisfy the full verifier. The combined run below
supplies handoff/provenance/retry evidence from fresh service and transport actions.

## Combined automated slice capture — 8 October 2026

The [complete capture](evidence/m3-complete-slice.json) passes the unchanged full
`verify_slice.py`. This supersedes the missing combined-oracle evidence described
in the historical partial records above. The [run record](evidence/m3-complete-slice-result.txt)
distinguishes native service actions, actual CLI/MCP processes and controlled-clock
expiry. Physical artist interaction remains unverified; this is not the full M3 gate.

Regenerate from the current worktree (socket tests require local Unix socket access):

```sh
FOLD_SLICE_CAPTURE_DIR=/tmp/fold-slice cargo test -p fold-motion capture_frozen_slice_native_evidence
FOLD_SLICE_CAPTURE_DIR=/tmp/fold-slice FOLD_M5_HANDOFF_DIR=/tmp/fold-handoff cargo test -p fold-app --test agent_dispatch --test agent_handoff
python3 docs/rewrite/assemble_slice.py /tmp/fold-slice/native-evidence.json /tmp/fold-slice/transport-evidence.json /tmp/fold-slice/expiry-evidence.json /tmp/fold-handoff/cli.json /tmp/fold-handoff/mcp.json /tmp/fold-slice/combined.json
python3 docs/rewrite/verify_slice.py /tmp/fold-slice/combined.json
```

Choose an unused output path for assembly. The assembler checks input completeness,
application build/schema agreement, both transports' seek/error/handoff equality,
nonempty artist edits and override/layout projections. It retains input hashes and
complete before/after constructions; it does not insert expected outcomes. The
integrated test counts actual Undo operations before and after exact retry, captures
artist clock/selection, saves publicly, reopens through native services and continues
public observation. The title-color override is nonempty and survives the handoff.
The native animated-shape capture supplies frozen rational samples and persistence;
the integrated scene supplies ramp/field preservation. These are distinct recorded
fixtures, not a claim that one physical UI session performed every action.
