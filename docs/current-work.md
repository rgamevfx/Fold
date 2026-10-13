# Current work: Motion, 3D and Compositing replacement

Status consolidated from the 2026-10-12 checkpoints, including the existing
uncommitted image-alpha, font-ownership and keyframe fixes. **All five replacement
slices remain open.** This checklist owns remaining scope and acceptance;
[architecture](architecture.md) owns contracts. No new runtime checks were run
for the documentation consolidation.

## Remaining work, in dependency order

| Slice | Implemented at selected seams | Still required |
| --- | --- | --- |
| 1. Shared 2D document | Schema-4 native shapes, text, paths, groups, images, paint and scripts; stable IDs, guarded edits, scalar/point/handle animation | Complete remaining path/animation and image/resource workflows; establish concrete missing cases before adding features |
| 2. fframes integration | Retained compiled scripts, shared native/script SVG/Skia rendering, floating-point GPU sharing and signed/HDR shader transport | Broader image/color input, full resource accounting, measured larger-scene performance and native package acceptance |
| 3. Artist and agent tools | Canvas, Pen, numeric point controls, hierarchy, graph navigation, script controls, conversion and immutable multi-pose frame review | Native keyframe, conversion and remaining vector/artist-agent workflow acceptance |
| 4. Context cutover | New Motion and independent 3D registered; old Motion crate/protocol paths removed; 3D hierarchy, resources, picking, transforms and animated parenting | Native 3D parenting/workflow acceptance, final deletion audit and current checkpoint validation |
| 5. Complete application slice | Mixed Motion/3D composition and Timeline save/reopen; candidate/Apply workflow; tested Float RGBA EXR parity | Other delivery/color cases, HDR display, sustained frame speed and end-to-end native artist/agent acceptance |

Work one concrete gap at a time: inspect current behavior, make a complete change,
run a relevant check, then offer an optimized demo. Preserve accepted work. Broad
categories above are not permission to invent new features or resume an old
session's goal mode.

## Accepted work and important limits

- **Accepted:** Motion layout at normal/narrow widths and the release canvas
  Bézier interaction (“okay. it feels good.”). Keep ImPlot input/guides. The
  separate animation/keyframe editor has not received native acceptance.
- **Images:** Motion accepts 8-bit sRGB stills without embedded ICC profiles,
  with default Straight or explicit Straight/Opaque alpha. Opaque retains source
  RGB even at zero alpha. Premultiplied reassignment, ICC and high-bit-depth/HDR
  input remain unsupported. Upstream image transport is premultiplied RGBA8;
  silently reducing precision is not a solution.
- **Memory:** known image/font ownership and Skia's budgeted cache have checks.
  Font parser metadata, text/drawing caches, unbudgeted GPU resources and driver
  allocations are not fully accounted for. Cache allowance is not total VRAM.
- **Subdivision:** Linear/Hold/HoldNext and aligned Bézier timing have geometry
  evidence. Incompatible timing rejects. Original/subdivided raster edges can
  differ; Bézier public/native/export qualification remains open.
- **Delivery:** tested mixed signed/HDR Float EXR matches linear preview in
  premultiplied and straight alpha. This does not qualify HDR image import,
  native HDR paint, other delivery formats or HDR monitor presentation.
- **Performance:** the application 10,000-dot baseline at 640×360 is 42.482 ms
  warm median / 45.123 ms p95; native warm median is 15.962 ms. These include graph
  evaluation and display output, not full UI FPS. Preview submission's 0.120 ms
  median excludes rendering and display. None is general performance acceptance.

## Evidence needed for the next decisions

Detailed historical results stay beside fixtures in
[resources/motion-context](../resources/motion-context), not in this checklist.

| Decision | Evidence |
| --- | --- |
| Image input and retained font bytes | [Alpha/font ownership](../resources/motion-context/alpha-font-ownership-evidence.json) |
| GPU cache versus full accounting | [Ownership](../resources/motion-context/gpu-cache-ownership-evidence.json), [pressure/recovery](../resources/motion-context/gpu-cache-pressure-evidence.json) |
| Geometry versus raster parity | [Bézier geometry](../resources/motion-context/bezier-subdivision-evidence.json), [subdivision pixels](../resources/motion-context/subdivision-render-evidence.json) |
| Signed/HDR delivery | [Mixed EXR](../resources/motion-context/mixed-hdr-export-evidence.json), [Skia precision](../resources/motion-context/color-precision-evidence.json) |
| Frame cost | [Application profile](../resources/motion-context/application-frame-profile.json) |
| Installed package | [Relocated precision package](../resources/motion-context/installed-precision-evidence.json) — predates later source changes |
| Public multi-pose and downstream review | [Animation](../resources/motion-context/animation-review-evidence.json), [mixed candidate](../resources/motion-context/mixed-candidate-evidence.json) |

The latest focused checks reported 11 shared animation-editor tests, four Motion
context tests and 15 media tests passing, plus the real GPU alpha/font example.
Real-Session regressions live in `crates/app/tests/motion_gesture.rs` and
`composition_gesture.rs`; the latter needs `fold-app/native-probe`. The canvas
release fix commits the matching active preview; the keyframe fix cancels a drag
on seek/playback. Widget-only checks previously missed these lifecycle failures.

The earlier workspace run had 575 passes, two failures and 70 ignored tests.
After stale alpha/count assertions were corrected, all 16 tests in those two
groups passed. The all-target desktop check and normal-policy Clippy passed with
warnings. These results predate the latest focused changes; they are not a new
full-suite pass. The updated SDK and release desktop built, but the installed
package also predates the latest fixes.

## Compositor performance work

The [development plan](../resources/render/dev-plan.md) owns the requested phase
checklist. The [HTML report](../resources/render/report.html) contains before/after
timings, test evidence, rejected experiments and open work. The original
[audit baseline](../resources/render/compositor-audit.json) remains historical.

Implemented: bounded status-readback reuse, single-pass validated color output,
a graph parameter arena, retained blur kernels, masked pointwise/shuffle fusion,
request-local solid/filter reuse, and incremental cache-pressure eviction.
On GTX 1070 / NVIDIA 580.173.02, the 16-solid-layer release probe fell from
17.672 to 8.904 ms median at 1080p and from 50.052 to 33.155 ms at 4K. The
single-solid probe fell from 6.726 to 0.732 ms. These are synthetic serial
renderer/display timings, not desktop FPS. Equal solid sources favor reuse;
distinct media layers require their own benchmark. Clocks/load were not controlled.

**The full optimization plan is not complete.** General binding/merge reuse,
large-filter algorithm changes, prepared-document and cross-frame caches,
per-node tiled regions, format-aware working storage and pooled native ingress
remain open. Two blur layouts were rejected after measurement. A cheap-opacity
reuse policy was also rejected because it increased live-image memory. FP32,
validation and the native ingress ownership copy remain intact.

Focused validation passed: 66 normal render tests, 49 explicit native GPU tests,
and 17 compositor tests. Feature checks and focused Clippy passed with existing
warnings. See [commands and limits](../resources/render/validation.json).
Workspace, spatial adapter, installed package and native artist acceptance were
not run for this change set. Existing uncommitted media/Motion/UI work was preserved.

## Photographic compositor nodes: artist review accepted

Current acceptance (2026-10-13): the artist approved the filters, RIFE, Viewer
and graph interactions after optimized-build review. Workspace all-target Clippy
with the desktop feature passed with warnings after correcting a test-only
renderer call. The earlier validation notes below record the evidence and limits
at each implementation stage; their deferred Clippy and general artist-review
status is superseded by this acceptance. Full workspace tests were not rerun,
as requested. Broader footage, hardware and installed-package qualification
remain open.

Lens Distortion, Film Emulation (eight original scene-linear looks), Grain
(film/sensor modes) and Retime use native compositor records and the shared
animation, linking, expression and edit lifecycle. The old Film Look/Grain
library constructors now create these native operators. Lens has a viewer center
handle and reference grid; advanced controls remain in inspector sections.

Retime's Source frame curve starts at identity, in the upstream sampling grid
initialized from the composition rate. Nearest/frame blending, hold/black/loop/
bounce boundaries and shutter angle/offset/sample controls request upstream
node/time pairs. Grain before Retime follows source time; grain after it follows
output time. Optical flow is not included. A reusable effect's already-bound
image cannot be requested at a different time inside that effect: place Retime
before the effect or use an internal Read source.

Artist review found an ImGui ID collision between Retime's Motion blur checkbox
and its shutter section; the checkbox now has a distinct internal ID. Shutter
samples refine integration over the existing source-frame grid, not optical-flow
intermediate images. At identity timing with a centered 180° shutter, 4 and 64
samples can produce identical frame weights; increasing shutter angle changes
the exposure duration. Native confirmation of the corrected inspector remains
pending.
The focused temporal-sampler regression passed: identity timing gives identical
4/64-sample weights, while 16× timing changes the contributing frame set from 4
to 9. The existing compositor inspector draw regression also passed. This does
not establish live Retime interaction or rendered-pixel acceptance; the desktop
was not rebuilt or restarted for this correction. Workspace tests and Clippy
remain deferred.

The photographic filters use the shared GPU shader path, explicit scene-linear
color interpretation and preserved alpha. Looks are original analytic film
looks, not measured named-stock LUTs. A focused compositor/UI check and optimized
Desktop build passed. A combined 960×540 ACEScg diagnostic frame was rendered and
inspected on GTX 1070 / NVIDIA 580.173.02 after correcting a shader keyword and
adjusting film-grain density response. This is shader-loading and image evidence,
not temporal/media regression or native interaction acceptance. Native screenshots
were unavailable through the current computer-use provider.

Color Shift and Chromatic Aberration are also implemented as native operators.
Color Shift provides an input eyedropper, selected-color swatch, Range/Softness,
collapsed hue/saturation/brightness tolerances, hue/saturation/brightness edits,
and an optional target color that preserves luminance shading. Show matte is a
viewer-only diagnostic; it does not mutate the effect or the normal output.
The input picker samples an alpha-weighted 3×3 neighborhood in scene-linear
working RGB from an aspect-preserving preview capped at 1024 pixels per axis.
Sampling/rendering runs on a cancellable worker; seeking, playback or selection
changes invalidate the pick. Matte refreshes after a parameter gesture completes.
Chromatic Aberration provides global and per-channel radial separation, signed
channel X/Y offsets, fringe-only blur/saturation, center protection, falloff,
edge sampling and a draggable optical center. Both use the existing numeric
curve/link/expression catalog and guarded Undo transactions; RGB swatches use
the shared animated color control.

For these two nodes, the desktop feature check, three WGSL source validations
and optimized desktop build passed. A 480×270 uncommitted ACEScg candidate was
rendered on GTX 1070 / NVIDIA 580.173.02: input, combined effects (including
fringe blur), and qualifier matte all rendered and were visually inspected.
This verifies shader loading and diagnostic output routing, not native
interaction acceptance. The optimized app was opened on a separate saved copy
of the artist's revision 103; the original running session was preserved.
Input picking, matte interaction, target-color behavior on footage, curve/link/
expression edits, Undo/cancel and narrow inspector acceptance remain for artist
review. Real-Session interaction regressions have not been run for these nodes.

At the user's request, full tests and Clippy are deferred until their hands-on
review. Still review each film look on real footage, lens round trips and edges,
grain temporal behavior, retime ramps/reverse/freeze/boundaries, curve and
link/expression edits, and viewer/inspector Undo/cancel at normal and narrow sizes.
The real-Session lifecycle and media/export regressions remain pending.

### RIFE interpolation: implemented, artist review accepted

Retime now offers Nearest, Frame Blend and RIFE. RIFE options contain Full/Half/
Quarter motion resolution and a cut threshold (zero disables detection). Existing
Source Frame curves, links/expressions, boundary modes and shutter controls stay
on the same Retime node. Each distinct shutter position is synthesized before
exposure accumulation; exact source frames and held/frozen samples bypass inference.

The packaged, pinned ncnn/Vulkan v4.6 backend is embedded behind `fold-rife`.
Weights, pipelines and bounded GPU tensor pools persist. Source preparation is
reused within a shutter evaluation; exact graph operations use renderer deduplication.
This is not a persistent cross-frame flow cache: v4.6 flow depends on the requested
timestep. External-memory buffer exchange keeps tensors GPU resident, with explicit
ownership transfers and completion waits. Cut detection reads a four-byte score.
Missing runtime/model, unsupported GPU sharing or budget exhaustion produce errors,
not a silent change to frame blending. Native inference cannot be interrupted
mid-dispatch; cancellation is checked between jobs/stages.

The current adapter uses FP32. An sRGB guide estimates flow/masks; reconstruction
warps original scene-linear premultiplied RGBA. Synthetic motion qualification
checks fractional positioning, HDR and transparency, but does not establish stock
RIFE pixel parity or high-quality handling of all HDR/alpha footage. Auxiliary
channels retain frame blending. Hard cuts use a coarse image-difference heuristic
and nearest-frame hold; camera flashes and rapid motion need artist review.

Focused temporal tests cover exact frames, distinct shutter fractions, freeze and
legacy blend sampling. The native runtime builds and the GPU probe runs on GTX
1070 / NVIDIA 580.173.02; warm 720p RIFE-stage time was roughly 47–53 ms in the
focused FP32 run (not end-to-end playback FPS). See the reproducible
[`rife_profile` probe](../crates/render/examples/rife_profile.rs) and
[qualification measurements](../resources/rife/qualification.json).

The optimized desktop also rendered the existing animated Retime curve with
16 shutter samples, Half motion resolution and downstream artist filters in
ACEScg through the public candidate path. The frame was visually inspected;
committed pixels matched the candidate and the review copy was saved. This is
a smoke check, not moving-footage or native interaction acceptance.

Remaining acceptance: real footage with occlusion, fine detail, fast pans and cuts;
reverse/ramp/freeze with shutter blur; normal/narrow native controls and Undo;
export/viewer agreement; installed-package relocation and non-NVIDIA Vulkan GPUs.
FP16 and broader caching/performance work should follow measured artist feedback.
Full suites and Clippy remain deferred until hands-on review, as requested.

## Compositor working Viewer: artist review accepted

The existing viewer panel can follow a separate terminal Viewer node. Select a
node and press **1**, or choose **View in Viewer (1)** from its context menu.
The first action creates one docked Viewer; later actions reconnect its single
input without changing selection or Output. Viewer has no parameters or output
socket. Manual wiring, disconnection and deletion use ordinary graph transactions.
The Add menu also exposes Viewer when the graph has none. Selecting Output can
preview its published branch; Viewer cannot feed a processing node.

Viewer can drag freely or dock at the graph's bottom edge. A small docking guide
appears during its drag. Docking and position commit together; Escape restores the
previous state. Its source and dock state persist in the document. The existing
viewer panel and transport controls are reused. Explicit viewing can switch an
existing viewer in the editor's link group from Composition to the child graph;
ordinary selection cannot. Composition/Timeline references to `video` continue
to receive Output.

Focused checks passed: desktop compilation, Session route/output isolation with
Undo/Redo/save/reopen, shared-canvas shortcut and dock/undock/cancel at 850/280-pixel
widths, and workspace viewer routing/time isolation. The optimized desktop build passed
and was opened on a separate copy of the latest saved review project; its public
catalog exposes the parameterless Viewer. Native artist interaction and
visual docking acceptance remain open. Full suites and Clippy remain deferred
until the user's hands-on review.

## Final acceptance gate

- Complete the remaining slices and removal audit, retaining generic opaque
  storage and independent Compositor/Roto services.
- Run the [integration checks](development.md#validation) and relevant opt-in
  native/GPU tests; record failures and unverified paths explicitly.
- Refresh and relocate the complete package; verify native/script/3D/video
  output, compilation without checkout access, save/reopen, Undo and public
  candidate/Apply flows. Compare color/alpha with explicit matching transforms.
- Review editable Bézier strokes, typography, 10,000 scripted dots, a 3D clone
  scene and a mixed composition. Record hardware, quality, warm frame and
  compile/update latency, and memory scope. Obtain native artist feedback
  separately from backend tests and renderer captures.

Update this file in place when a gate changes. Keep local project backups and
running process/socket details outside repository documentation; discover and
save live artwork before restarting any review instance.

## Compositor graph interaction: artist review accepted

Delete bridges the primary input through a removed node or chain; Merge uses B.
A deliberately quick shake extracts a dragged node and repairs the pipe, including
Viewer connections. Release commits the gesture once; Escape restores wiring and
position. **D** or the node context menu toggles bypass; mixed selections disable
together and all-disabled selections enable together. Bypass preserves controls,
curves and wiring, and skips Retime interpolation/shutter work. Source nodes bypass
to transparent black. Output and Viewer remain terminal routes without Disable.
Dropping a loose or freshly extracted node onto a compatible wire inserts it
using its primary input and output. Targeting follows the actual dragged node,
uses the card bounds plus a 12 px margin, and chooses the nearest compatible wire.
The highlighted connection previews insertion; release snaps the card center to
the pipe. Extraction, movement and reinsertion remain one Undo entry. Focused
normal/narrow gesture checks cover unselected-node dragging, near-edge drops,
alignment, shake-then-insert and Escape restoration.
Selection has a stronger outline and tinted fill; disabled cards are subdued with
a diagonal strike. Native shake sensitivity and visual acceptance remain for
artist review before broad validation.

Focused checks cover chain/Viewer reconnection, real Session bypass and extraction
preview/cancel/Undo/Redo/persistence, shake jitter rejection and cancellation at
850/280 px, the non-repeating D shortcut, and preservation of the repaired wire
through native Delete. The optimized desktop build and package boundary check
passed. Full suites and Clippy are deferred
until artist review, as requested.

Compositor search groups are I/O, Color, Filter, Distort, Time, Merge, Mask and
Utility; Chromatic Aberration belongs to Filter. Muted node fills use editable
UI > Node group colors preferences, shared by existing graph views and persisted
outside project documents. Tab search opens at the last graph click, clamped to
the panel; the scrollable catalog has category headings and searches both node
and group names. Enter adds the first matching node. With one processing source
selected, adding a compatible node inserts it after that source, reroutes its
consumers (including Viewer), selects the new node, and commits one Undo entry.
New sources and incompatible types remain loose. Focused checks cover preference
persistence/live updates, popup position and narrow bounds, and Grade/Merge
insertion through Session Undo/Redo. The artist accepted the category colors and insertion workflow.
