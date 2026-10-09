# Dependency experiments and decisions

## M4 text-service integration

`fold-render::text` now pins Parley 0.7.0 with default features disabled and `std`
enabled. The workspace lockfile records the resolved closure. `cargo tree -p fold-render
-e features -i fontique --locked --offline` confirms only the `std` path; system font
discovery is not enabled. This selects Parley for the initial CPU service, not the
vector/spatial backend or their rendering acceptance.

The service accepts explicitly supplied font bytes/family and bounded uniform-style
text. It retains bytes, SHA-256 font/content identity, positioned glyphs, visual runs,
UTF-8 cluster ranges, face indices and normalized variation coordinates in Fold-owned
records. Snapshots share owned font bytes and outlive the worker context. Missing
glyphs produce range diagnostics; family absence, substitution and unsupported
synthetic styling fail explicitly. Size/width inputs are checked and text/font/glyph
budgets are bounded. The service is synchronous CPU work intended for preparation
workers; it is not wired into UI drawing.

Three native tests qualify positioned normalization against Parley for wrapping,
combining characters, ligatures and mixed-direction text; identity changes under
wording/size/wrapping; retained font ownership; and missing glyph/font/invalid-input
behavior. Vector drawing, outline extraction, spatial preparation, richer paragraph
styles, fallback policies and renderer integration remain unqualified.

`fold-render::text::outlines` now uses pinned Skrifa 0.37.0 (`std`, no defaults),
already in the Parley/fontique dependency closure, to extract unhinted curves from
the retained font face, glyph IDs, size and variation coordinates. It does not shape
again. Local curves remain Y-up; explicit per-glyph transforms place them in the
normalized Y-down layout. Repeated glyphs share path storage. Vector Drawing objects
share those same paths/transforms; editable curves and cluster mappings remain intact.
Cancellation and unique/expanded segment budgets bound extraction, and missing,
COLR or bitmap glyphs reject rather than producing a partial successful result.

The packaged-font O/B/8 fixture has 2/3/3 closed contours and five transparent hole
centers through the existing shared CPU vector reference. The [capture](evidence/m4-ob8-cpu-outlines.png)
was visually reviewed. This is evidence for the CPU outline/vector path, not Vello,
GPU/spatial rendering, extrusion, complete outline-area equivalence or color-font
support. SVG/color-font policy and held-out font fixtures still need qualification.

`fold-render::text::mesh` consumes the retained outlines without reshaping or font
lookup. It reuses shared nonzero fill tessellation, deduplicates local vertices and
normalizes triangles to face +Z. Glyph identity, whitespace and the original affine
layout transform remain explicit; callers supply projected pixel scale for curve
precision and map the layout into their chosen spatial placement. Geometry caches
are bounded and independent of material/placement; cancellation and expanded
triangle limits bound work. This is planar geometry, not extrusion or a global
memory-admission service.

The [planar spatial test](evidence/m4-text-mesh-result.txt) uses one retained Noto
Sans layout for both vector output and spatial meshes on Vulkan llvmpipe. At the
declared 640×220 plane placement, 6,189 opaque and 121,509 clear interior pixels
match exactly and opaque depth reports four view units. Three unique meshes serve
four glyph instances and remain retained on the next render. MSAA-off spatial edges
are excluded from this comparison; antialiasing equivalence, arbitrary placement,
extrusion and complete M4 acceptance remain unqualified.

The text service also accepts immutable `FontSource` snapshots. In-memory creation
supports explicitly packaged bytes; worker-side linked-file loading requires a
canonical `sha256:` fingerprint and a regular file within the 32 MiB source limit.
Changed/missing sources, growth/truncation, invalid fingerprints and cancellation
fail without publishing a source. Held snapshots survive deletion/replacement;
new acquisitions always verify the linked file again. Parley registration and
normalized layouts share the same owned byte allocation. Font-format and family
validation remain explicit at shaping-context creation. This is per-source bounded
loading, not aggregate font-memory admission, native Text authoring, project font
import or resource provenance integration.

## M5 native packaged Text

`text.packaged` explicitly selects Noto Sans Regular from the version-one packaged
resource. Its SHA-256 is pinned to the frozen font; a mismatched bundle or unsupported
font choice fails. Each evaluation owns its shaping context and retains one normalized
layout per evaluated Text node. Native appearance branches share the text object, and
the shared outline/mesh services consume that same layout without reshaping.

Native geometry distinguishes contour and text content. Text retains glyph, cluster
and line slot identities, logical ranges and shaping diagnostics. Slot IDs remain
stable when correspondence survives metric/time changes; arbitrary text-edit repair
and word/grapheme grouping are not yet qualified. CPU observations distinguish layout
bounds from ink, report actual font content and face/run data, and include the evaluated
packaged resource in font provenance. This adds no rendered-frame capability or system
font lookup. Native Text Animator wiring, project font import,
mixed placement and physical acceptance remain open.

## M5 extrusion preparation

`fold-render::spatial::extrude` consumes a welded, conforming planar triangulation
from preparation. It reuses that cap at Z=0, reverses its winding at Z=-depth and
builds side strips from unpaired oriented boundary edges. Holes therefore use the
same tessellated boundary for caps and walls. Cap normals are flat; each boundary
edge has a hard side normal. Uniform depth segmentation and separate front/back/side
index ranges are explicit. No new dependency or Bevy type enters CPU preparation.
An output-vertex→cap-vertex table makes source attribute transfer explicit, including
vertices duplicated for distinct cap and side normals.

The service checks finite/bounded input, edge incidence, closed boundary degree,
cancellation and vertex/index limits. It requires non-overlapping input cap triangles;
it is not an arbitrary mesh repair or boolean-union service. Two CPU tests qualify
O/B/8 closed solid topology, expected hole genus, winding/normals, volume and failure
cases. Native operation wiring, source association through edits, per-region material
rendering, bevels and full M5 artist/agent acceptance remain outside this qualification.

The [native extrusion result](evidence/m5-extrude-result.txt) qualifies front and
reverse views of the same O/B/8 geometry with 0.15 depth and three segments. An
independent f64 ray/triangle reference checks cap, side-wall and clear samples against
RGBA16F coverage and axial depth, with 2e-4 view-unit depth tolerance. Coverage samples
within 1/128 pixel of projected triangle edges are explicitly excluded; this is not
antialiasing agreement. Both cameras pass, and camera changes reuse existing geometry.

## M4 float vector candidate evidence

The independently retained shared analytic vector renderer now has a fresh GPU test
using the new Parley/Skrifa outline path. On Vulkan llvmpipe (hardware/driver and font
hash in [results](evidence/m4-text-gpu-result.txt)), 0.25×/1×/4× zoom, translucent
fill/stroke and ordered overlapping marks match the CPU linear reference with maximum
absolute channel error 1.85e-6. An independent interior Over/premultiplication check
passes; transparent scene pixels remain transparent. GPU display conversion matches
CPU conversion over black exactly. No CPU image-adapter node supplies the vector
image. [Quarter-size](evidence/m4-text-gpu-0.25.png), [normal](evidence/m4-text-gpu-1.png)
and [enlarged](evidence/m4-text-gpu-4.png) captures were visually reviewed.

This candidate currently renders into RGBA32F, so it does **not** qualify the frozen
RGBA16F mixed-scene boundary. Vello 0.7.0's cached primary source documents
`render_to_texture` as requiring RGBA8Unorm; its M0 red-pixel probe likewise did not
qualify HDR/linear intermediate behavior. No final vector backend selection is claimed
yet. M4 still requires its full fill-rule/clip/cap/join fixture set, the declared
intermediate/resource boundary, physical desktop agreement and spatial integration.

The shared candidate also passes thirty fixture/zoom combinations: same-winding
nonzero versus even-odd nesting, opposite-winding holes, a self-overlapping path,
butt/round caps, miter/round joins, rectangular clipping and a quarter-unit stroke
at 0.25×/1×/4×. Independent interior pixels distinguish fill rules/caps/joins and
clip exclusion; integrated column alpha checks thin-stroke coverage at all zooms.
CPU/GPU maximum channel error is 3.76e-6. [Adapter and per-case results](evidence/m4-vector-fixtures-result.txt)
remain RGBA32F candidate evidence, not the RGBA16F mixed-scene gate. Arbitrary
path clipping and the remaining resource/spatial/desktop contracts are unqualified.

The shared GPU frame now supports explicit RGBA32F→RGBA16F storage conversion,
format-aware descriptors and padded readback. Conversion retains color identity,
premultiplied alpha, rational timing and upstream completion dependencies; it uses
one GPU pass without CPU image transfer. Out-of-range half-float values fail through
the existing asynchronous status boundary. This is a storage boundary, not a color
transform or final mixed-scene rendering path. The text fixture through RGBA16F
has [maximum linear channel error 4.89e-4 and display error one byte](evidence/m4-rgba16-text-result.txt) at all three
zooms on llvmpipe. Existing RGBA32F captures above remain their original evidence.
Spatial/depth integration, native presentation and final backend selection stay open.

## M4 offscreen spatial experiment

The [spatial probe](probes/src/bin/spatial.rs) now executes Bevy 0.18.1's real 3D
render graph with an externally supplied wgpu device/queue and manually owned
RGBA16F target. Its device feature mask and default limits match current
`Host::headless`. Selected Bevy features are std, async_executor, multi_threaded,
bevy_asset, bevy_render, bevy_core_pipeline, bevy_pbr and bevy_log; defaults are off.
There is no Winit plugin or primary window. Pipelined rendering is disabled and
Bevy time receives zero manual duration. Entity transforms receive explicit sample
inputs in a reordered sequence; this is not yet native Motion evaluation.

[Captured llvmpipe results](probes/spatial-result.txt) prove:

- Retained entity/mesh/material handles, near/far occlusion, and readable reverse-Z
  depth match the declared perspective camera and cube surfaces.
- Unlit linear HDR values above one survive offscreen output without tone mapping,
  dithering or a display conversion. Clear pixels preserve transparent black.
- Straight StandardMaterial colors with `AlphaMode::Blend` produce premultiplied
  RGBA16F pixels over opaque and transparent backgrounds. An initial experiment
  with `Premultiplied` and straight inputs failed the independent Over reference;
  that mode requires already-premultiplied input and is not the selected route here.
- A 50% gray surface with zero ambient, zero reflectance, roughness one and an
  explicit EV100 8 exposure matches the Lambert gray-card reference within 0.002.
  1000 lux gives 0.5180664, 2000 lux gives 1.0361328, and revisiting 1000 lux agrees.
- Initial asset/pipeline readiness takes two updates; subsequent tested material,
  placement, visibility and light changes reach expected pixels in one update.

This is an executable dependency boundary experiment, not the production adapter.
Bevy reports limited GPU preprocessing and disables SSAO/environment-map generation
under this device request; those capabilities are not exercised or qualified.
Application resource accounting, packed instances, revision-bound publication,
recovery/cancellation, shared text geometry, mixed screen/world composition and
physical desktop acceptance remain open. The spatial probe passes clean Clippy;
this isolated experiment preceded the optional workspace integration below.

## M4 initial workspace spatial adapter

The optional `fold-render/spatial` feature now pins Bevy 0.18.1 in the workspace
with defaults off (std, async_executor, multi_threaded, asset, render, core-pipeline
and PBR features). The adapter installs only the required task/time/transform,
asset/window-resource, image/mesh/camera/light, render/core-pipeline/PBR and diagnostic
plugins. It creates no primary window, app runner, UI, audio or pipelined rendering.
Both headless and desktop `Host` construction retain the original instance/adapter
alongside their device/queue; the adapter borrows that same context.

`fold_render::spatial` exposes CPU-available immutable mesh, material, instance,
camera/light and frame-binding records without Bevy types. Validation checks the
whole request before retained-state edits, including resource references, finite
metrics, geometry/object limits and the camera content hash. The GPU adapter retains
mesh/material handles and object entities, updates changed mesh/material/instance
inputs, and removes resources absent from the next snapshot. Equal reconstructed
mesh payloads do not force uploads. This is initial object rendering; large packed
cloner data is not implemented.

Output is scene-linear Rec.709/sRGB-primary premultiplied RGBA16F. Perspective
cameras supply explicit exposure; unlit materials preserve graphic values and lit
materials use unshadowed directional lights with zero ambient. Bevy time is frozen:
Fold supplies evaluated scene inputs and exact rational frame time. Two preparation
updates settle the render graph for each request; no history-dependent effects or
shutter integration are claimed. Returned frames require the exact document,
revision, complete camera hash, time, duration and dimensions for borrowing/readback.
Application publication still needs to pass its current request through this check.

Output/validation textures use the shared host pool and submission leases, including
cancellation/error/unwind paths. Held frames survive later renders and adapter
teardown. GPU validation rejects nonfinite/invalid alpha and saturated half-float
RGB before readiness succeeds. A native overflow test showed that attachment
conversion can saturate rather than produce infinity: spatial output at either
±65504 endpoint is therefore rejected too, since stored pixels cannot distinguish
an authored endpoint from overflow. The explicit RGBA32F→RGBA16F conversion retains
its separate pre-conversion range check and accepts representable endpoints.
Synchronous Bevy shader compilation shares Fold's compiler lock/error-scope boundary.

[Workspace adapter evidence](evidence/m4-spatial-adapter-result.txt) covers unlit HDR,
premultiplied transparency, calibrated gray-card lighting, geometry/material/placement
edits and resource removal, camera/revision/time identity, held-frame immutability,
cancellation, saturation rejection/recovery, display conversion and device loss.
CPU-only validation tests, the prior precision-boundary tests, desktop compilation,
focused spatial Clippy, formatting and package-boundary checks pass. Clippy retains
five existing shared-render warnings.

This is preparatory implementation, not M4 acceptance or application provider wiring.
Host budgeting currently covers Fold output/validation resources, **not Bevy's internal
allocations**; GPU statistics likewise omit internal Bevy pass costs. Viewer/agent
depth publication and object picking, internal allocation admission/accounting, packed
instances, shared text geometry, Screen/Camera Plane composition, integration, recovery on a
fresh device and physical desktop acceptance remain open. The initial adapter accepts
Vulkan/default-device-limits hosts, positive TRS meshes and the declared lighting
subset; it does not advertise the disabled SSAO/environment-generation capabilities.

## M4 retained depth and native bootstrap follow-up

Each spatial frame now owns a host-budgeted Depth32F copy of Bevy's opaque-surface
depth attachment. The copy is queued before another render can reuse the attachment;
submission callbacks retain its lease until completion. Depth readback checks the
same complete frame binding as color, waits for color validation, accounts padded
transfer/CPU storage, and supports cancellation and a bounded timeout. CPU samples
are row-major reverse-Z in [0, 1]; zero is background. `view_distance` returns
positive camera-space axial distance using that frame's near plane. Blended surfaces
do not write this opaque depth plane; it is not an object-ID picking result.

The camera-update regression found that Bevy 0.18.1 stores `near_clip_plane`
independently of `near`. Changing only `near` left clipping/depth at the default 0.1.
The adapter now sets the aligned clipping plane explicitly from Fold's near value.
Regression coverage includes the frozen scene's 0.01 near plane, changed camera
position and dimensions, independent near/distance references, padded rows, held
frames, transparency, cancellation and device loss. [Depth results](evidence/m4-depth-result.txt)
include the three spatial tests passing together in parallel.

That parallel run also exposed a deterministic native crash in concurrent
`wgpu::Instance::new` calls on this Vulkan host, before adapter selection or Bevy
initialization. A minimal two-host barrier test reproduced it. `Host::create_instance`
now serializes only native instance construction; headless and desktop bootstrap
both use it. No asynchronous adapter/device selection or rendering holds this lock.
The minimal regression passes five fresh-process repetitions, and the spatial suite
passes in parallel. This identifies the failing native API boundary, not a proven
fault in a particular upstream library. Debug instrumentation was removed.

Focused Clippy has only the five existing shared-render warnings. Desktop
compilation, precision regression, formatting, package-boundary and whitespace checks
pass. Internal Bevy resource accounting, object picking, viewer/agent publication,
shared text geometry and complete M4 acceptance remain open.

M0 candidates, 8 October 2026. Owner: rewrite implementer. Selection is provisional
until executable integration evidence passes; M4 owns rendering qualification.
Pinned experiment manifest and lockfile: [probes/Cargo.toml](probes/Cargo.toml).
The experiment is isolated from the application workspace and is not a second
motion implementation.

| Candidate | Features / license | Boundary and remaining experiment |
| --- | --- | --- |
| Bevy ECS/render 0.18.1 | `bevy_ecs`: std; `bevy_render`: no defaults. MIT OR Apache-2.0 | Uses wgpu 27. Test retained entity identity, explicit Fold time, conversion from the same wgpu Device. M4 must add selected camera/core-pipeline/PBR plugins, offscreen scene rendering, shared depth/readback and recovery tests. No Bevy window, UI, audio or project authority. |
| Parley 0.7.0 | std, no system discovery; Apache-2.0 OR MIT | Register packaged font explicitly; shape text and inspect glyphs. M4 must prove content/font identity, cluster mappings and one layout feeding vector, spatial and outlines. |
| Vello 0.7.0 | wgpu, no default backend selection; Apache-2.0 OR MIT | Uses wgpu 27.0.1. Test render into shared device texture, completion and readback. M4 must qualify holes, fill rules, caps/joins, clipping, zoom and linear premultiplied compositing. |
| wgpu 27.0.1 | std, vulkan, wgsl for Linux probe; MIT OR Apache-2.0 | Existing Fold UI and rendering also use 27; the shared-device experiment must compile and execute, not merely compare versions. Other OS backends require separate packaging checks. |
| OpenUSD 26.08 | Bounded native scene composition/geometry/material adapter; no Hydra/OpenExec baseline | Candidate is the official native implementation. Local Python `pxr` import is unavailable. M8 must qualify a narrow C ABI/process boundary, threading/errors and deployment; no handwritten partial USD parser or claim of Rust API compatibility. |

Official sources: [Bevy 0.18.1 rendering crate](https://docs.rs/bevy_render/0.18.1/bevy_render/),
[Parley 0.7.0 API](https://docs.rs/parley/0.7.0/parley/),
[Vello 0.7.0 source](https://docs.rs/crate/vello/0.7.0/source/),
[OpenUSD repository and build instructions](https://github.com/PixarAnimationStudios/OpenUSD),
[OpenUSD 26.08 license and bundled notices](https://github.com/PixarAnimationStudios/OpenUSD/blob/v26.08/LICENSE.txt).
Cargo's downloaded manifests are the authoritative version/feature evidence for
Rust candidates; `cargo info` succeeded for Bevy/render and Parley. Vello's cached
manifest and Bevy render's downloaded manifest both declare wgpu 27. Do not infer
runtime compatibility solely from these manifests.

Rust probe uses no system text library; fonts are packaged data. Vulkan loader and
a functioning ICD are runtime prerequisites. Bevy's transitive native builds and
platform support must be recorded from the actual locked compile. OpenUSD's native
build needs a C++ toolchain/CMake and its selected dependencies; pin that closure
and retain license notices before distributing. Neither a successful Linux probe
nor docs.rs target listings establish Windows/macOS package support.

## Bounded qualification experiments

1. M0: compile and run `probes` against the lockfile, using two build jobs and no
   debug information. Check retained ECS identity, packaged-font layout, device
   sharing, Vello render/submit/completion and exact center/background pixels.
2. M4: construct the frozen mixed scene with selected Bevy render plugins using
   manual render creation from Fold's device/queue. If this fails, measure an
   explicit texture transfer route (bytes, latency, lifetime) and amend the boundary
   before proceeding. No fallback to the old Motion renderer.
3. M4: render the vector fixtures and normalized text branches, depth/color/alpha
   tests, rapid edits and device recovery. Reject stale revision/time/camera output.
4. M8 preparation: bounded OpenUSD native probe imports a reference and variant,
   reads units/up-axis, exports one instanced mesh/camera and two time samples,
   then reopens and checks correspondence. Use a dedicated worker thread/process;
   errors cross the boundary as structured results. Native library/package absence
   is an explicit unsupported capability until this passes. Allocate one clean
   build experiment before choosing bindings; owner remains rewrite implementer.

## Hardware baseline

Reference host available here: x86_64 Intel Core i7-8750H, 6 cores/12 threads,
Rust/Cargo 1.95.0. Within the filesystem/device sandbox, `env -u DISPLAY vulkaninfo --summary` enumerates only CPU Vulkan
llvmpipe, Mesa 26.1.6, LLVM 20.1.2, Vulkan 1.4.354. `nvidia-smi` cannot communicate
with the NVIDIA driver there; default `vulkaninfo` cannot connect to X. Use headless Vulkan
for bounded correctness probes. Do not claim physical GPU throughput, native
presentation acceptance, or 1080p60 from these results. Re-freeze a named hardware
profile before M6 performance measurements if a physical GPU becomes available.

## Executed result

`CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 cargo run --manifest-path docs/rewrite/probes/Cargo.toml --locked --offline`
passed on the reference host. [Captured result](probes/result.txt): retained ECS
entity identity passed; packaged Noto Sans produced 18 glyphs at width 312.096;
Vello rendered through the Bevy-wrapped shared wgpu Device and exact red interior /
transparent background pixel readback passed after queue completion. The final
probe explicitly selects Noto Sans: registering a font alone left the default
generic family unresolved with system discovery disabled. This is a requirement
for the future text service, not permission to substitute fonts.

The first compile exposed Vello's required `pipeline_cache` option (set to None in
the experiment); the corrected locked compile passed. The experiment does not
instantiate Bevy's spatial render graph, qualify text outlines, or exercise HDR
working-color behavior. These remain M4 tests. OpenUSD native execution remains
unavailable locally and is not claimed qualified.

## Physical GPU follow-up during M1

The isolated desktop smoke outside the sandbox exposed NVIDIA GeForce GTX 1070,
Vulkan driver 580.173.02. The existing pinned probe binary was then run outside
the sandbox without rebuilding or changing its fixture:
`docs/rewrite/probes/target/debug/fold-rewrite-dependency-probe`.
[Captured physical-GPU result](probes/physical-gpu-result.txt) passed retained ECS
identity, the same 18-glyph packaged-font layout, shared Bevy-wrapped wgpu device,
Vello submission/completion and exact red/transparent readback. The earlier
llvmpipe results remain valid for the sandbox environment; they did not establish
that the host lacked a physical GPU. Neither probe instantiates the spatial Bevy
render graph or qualifies mixed-scene color/depth. M4 still owns those tests.

## M5 native Outline Text

`outline_text.curves` consumes retained Text geometry and calls the shared outline
service with the native evaluator's cancellation/step-budget checkpoint. It preserves
the same layout and font bytes, logical mappings, exact glyph transforms and shared
paths for repeated glyphs. Curve and command IDs derive from the source glyph and
stable local slots. The node preserves appearance without converting strokes into
solid profiles. Missing/color/bitmap glyphs and non-text input fail explicitly.

CPU observations expose source text/font provenance, contour IDs and retained curve
commands. Bounds are conservative control-point hulls in layout space (then translated
for Screen placement), explicitly distinguished from layout or exact ink bounds.
Glyph counts count represented glyph occurrences per distinct geometry, as with Text;
shared appearance branches of the same geometry are measured once. No raster or
native extrusion publication is implied. The checkpoint stops accepting pen commands
on first error; Skrifa may finish its current internal draw before returning. Shared
segment limits remain enforced; aggregate memory admission remains open.

## M5 explicit metric text extrusion preparation

`fold-render::text::extrude::prepare` consumes the retained planar text snapshot,
an explicit meters-per-canvas-unit mapping, depth in meters and depth segmentation.
It maps layout origin to world origin and canvas Y-down to world Y-up, preserving
per-glyph affine placement separately from the metric solid. It retains the original
planar/layout/font ownership and output-vertex→source-cap-vertex correspondence;
whitespace has no solid and repeated planar meshes share an extruded solid. Cap/side
regions are retained for subsequent material assignment. Authored mapping/depth remain
f64; generated mesh storage follows the existing f32 renderer contract.

The service admits cap copies before allocation and bounds expanded geometry as well
as each unique solid. Both it and the generic extrusion service accept caller-owned
budget/cancellation checkpoints, including depth-segmentation loops. Planar preparation now also exposes caller-owned checkpoints through flattening,
tessellation callbacks and mesh indexing. Native evaluator wiring is qualified for the straight-text mode below; aggregate
preparation-memory accounting remains open. Native World placement is qualified below;
rendered material/placement publication remains open. The existing Vulkan front/back ray oracle
now consumes this explicit metric preparation and retains its declared edge exclusion.

## M5 planar preparation checkpoints

The shared geometry cache and text mesh preparation accept caller-owned checkpoints.
They check cached requests, bounded curve flattening, emitted tessellation vertices
and triangles, mesh indexing, and final publication. The first caller failure stops
accepting geometry and is retained through the tessellator error boundary. Failed
preparation returns no partial result; completed cached geometry can be reused on a
later request. Lyon may finish bounded internal work after a triangle callback stops
accepting output, since that callback has no error return; the next vertex callback
returns failure. Path construction and library-internal work are not preempted at
every instruction. Subdivision/geometry limits now remain typed budget errors for
text consumers, independent of diagnostic wording. Existing raster consumers keep
their public cancellation/error interface over this same preparation path.

## M5 native straight text extrusion

`extrude.straight_text` now connects the native evaluator to the qualified planar and
metric solid services. It accepts Outline Text output and explicit metric conversion,
depth, segmentation and curve-detail controls. Evaluation uses native checkpoints for
all preparation stages and admits generated topology IDs against the element budget.
It retains source geometry/appearance as provenance; the resulting world/meter solid
has no implicit spatial material. Generic profiles remain unqualified; native Material and World placement are qualified below. The graph starter explicitly creates Text → Outline Text → Extrude.

Repeated glyphs share solids, cap/side regions and prototype vertex/face IDs. Occurrence
IDs derive from source glyph IDs. Prototype identity includes indexed topology and
source-vertex correspondence, so metric depth edits preserve identity while segmentation
or tessellation changes invalidate changed topology. IDs describe indexed slots, not
persistent logical anchors across arbitrary reshaping. CPU observations provide 3D
bounds, topology and resource provenance but no rendered frame. The original font/layout
and curves remain shared; no second shaping pass, system font lookup or hidden paint
conversion is introduced. Physical normal/narrow UI checks and full visual handoff are
still required.

## M5 native Material

`material.lit` and `material.unlit` assign collection-wide spatial surfaces to retained
world/meter solids. They preserve geometry identity and shared solid storage; material
replacement does not alter cap/side regions or source provenance. Native authored color
is straight linear Rec.709 in f64, with RGB in [0,65504] and alpha in [0,1]. Lit controls
expose roughness and reflectance in [0,1]. Unlit exposes only color; unused renderer
surface coefficients have deterministic defaults. Negative RGB remains supported for
planar paint but is explicitly rejected by the bounded spatial material contract.

Source signals animate surface values at exact evaluation times. CPU observations
report the assigned surface alongside geometry, while graph creation uses existing
controls and inserts Text → Outline Text → Extrude dependencies. This qualifies native
whole-collection assignment, not per-region selection, referenced material assets,
renderer conversion/publication or physical UI acceptance. Cap/side region metadata
remains available for subsequent selection support.

## M5 native World placement

`place.world` retains explicit f64 metric position, unit quaternion rotation, positive
3D scale and opacity, applying scale → rotation → translation about the geometry origin.
It accepts world/meter solids and preserves geometry/material sharing. Native Placement
now stores an explicit Screen-or-World transform; the former unconditional 2D position
field is removed. Screen semantics remain unchanged and reject world geometry.

Spatial scenes retain ordered Screen/World occurrences with explicit camera and light
controls. CPU observations tag placement mode and compute actual transformed mesh bounds
from vertices, including per-glyph layout mapping before the world transform. Surface
alpha and placement opacity remain separate authored controls. Graph creation uses the
existing catalog and creates explicit Text → Outline Text → Extrude → Material inputs.
Camera Plane placement, reflection/zero scale, renderer conversion/publication and
physical UI/visual acceptance remain open. Positive scale and bounded translation use
the same limits as the shared spatial adapter; author precision remains f64.

## M4/M5 snapshot-bound spatial-pass conversion

`fold-motion::rendering::spatial` converts one immutable project snapshot/output/time
into shared renderer scene inputs and an exact document/project-revision/time/frame-
duration/camera-content binding. It retains the evaluated Motion scene for provenance.
Deterministic object IDs preserve mesh and occurrence correspondence; distinct scene
occurrences keep distinct renderer instances. Mesh storage is now shared by Arc from
solid preparation through conversion, avoiding a mesh copy at this boundary. Authored
material alpha combines with placement opacity, while lit/unlit controls and explicit
camera/light values transfer to the bounded f32 renderer contract. Validation rejects
precision loss that makes the result invalid, such as scale underflow to zero.

Evaluation and conversion have separate bounded work budgets and cancellation checks;
this is not aggregate preparation-memory admission. Conversion charges retained mesh
validation work before final shared-scene validation. The current entry point accepts
spatial-only scenes and explicitly rejects Screen entries, missing active camera or
material, and unsupported glyph affine transforms. Mixed-pass composition, application
frame-provider registration, native-scene GPU acceptance and physical artist handoff
remain open. The existing shared Vulkan extrusion oracle passes after the mesh-ownership
change; it does not by itself qualify native-scene frame publication.


## M4/M5 Motion-document Vulkan evidence

The opt-in `fold-app/spatial` integration test now evaluates committed Motion
snapshots directly into the retained shared adapter. The independent f64 ray oracle
is shared with the renderer test rather than duplicated. O/B/8 coverage and depth
pass for two extrusion depths and front/back views on llvmpipe Vulkan. Mesh sharing,
revision/time/camera bindings and retained frame pixels survive project edits.
Authored lit materials, directional illuminance and camera exposure pass zero-light,
coverage preservation and one-stop linear-radiance checks. This advances the native-
scene GPU gap listed above for opaque text solids only; it does not install an
application frame provider or qualify mixed passes, physical presentation, absolute
text-surface photometry, packed cloners or aggregate Bevy memory budgets.
See [commands and results](evidence/m5-motion-gpu-result.txt).


## M4/M5 native Screen preparation

`fold-motion::rendering::screen` prepares ordered shared vector Drawing inputs from
an immutable snapshot using the same evaluation entry as spatial preparation.
Contours retain collection winding; Text and Outline Text share retained layout and
curve semantics. Per-request path sharing avoids duplicating appearance branches.
The output records document, revision, output, exact time/duration and dimensions;
paint stays straight linear Rec.709, including signed HDR values. Vector float
validation permits those values without claiming an ACES conversion. Publication
must supply an explicit working-space conversion. The current Screen-only and
spatial-only entry points reject mixed inputs; neither installs an application
video provider. [CPU evidence](evidence/m5-screen-preparation-result.txt) proves text/
outline pixel parity and analytic ordered contour composition, not mixed GPU or
physical artist acceptance.


## M4/M5 Screen video-provider registration

Motion now registers Scene outputs through the shared VideoProvider interface,
compiling retained Screen drawings into Vector image IR. The caller retains the
immutable snapshot; native output UUIDs identify ports while labels remain editable.
Playback time is relative to canvas start, with exact rational scene-time mapping.
The shared authored-color service exposes its existing pinned Rec.709/ACEScg matrix
for already-linear paint, avoiding an accidental picker transfer or display transform.
Only ACES paint compilation accepts signed HDR through the existing vector contract;
non-ACES invalid paint rejects instead of clamping.

Application assembly -> registry -> CPU rendering tests qualify this Screen path.
Spatial/mixed publication, image-effect reuse, agent frame observation, GPU end-to-end
output and physical UI acceptance remain open. The existing pinned viewer list uses
provider descriptors, but linked viewer defaults and browser/document placement still
hardcode `video`; resolve those through provider outputs for full native handoff.
[Commands and evidence](evidence/m5-screen-video-result.txt).


## M4/M5 named-output document placement

Provider-declared default output resolution now supplies document insertion and
Project drag/drop references. Timeline drop previews read the chosen output's
metadata, and nested clip validation accepts nonempty named video outputs without
requiring a literal `video` port. The existing video-track/audio-link restrictions
remain. Actual Motion -> Timeline nested CPU rendering and undo/redo pass; comparison
accounts for Timeline's opaque black background. [Evidence](evidence/m5-output-placement-result.txt).
Linked-viewer resolution and compositor document-read validation still need removal
of their output-name assumptions. Physical normal/narrow drop interaction is unverified.


## M4/M5 Compositor Read named-output integration

Compositor Read now retains nonempty provider output IDs instead of requiring the
literal `video` name. Shared dependency/channel/image resolution validates the actual
port. Motion -> Compositor CPU integration passes exact fractional source-time mapping,
half-alpha preservation, named channel membership and transparent inactive ranges;
empty and unavailable ports fail at their respective document/provider boundaries.
[Evidence](evidence/m5-compositor-output-result.txt). Linked Workspace navigation still
constructs conventional video references and needs a provider-aware resolution change.


## M4/M5 linked viewer output resolution

Workspace no longer creates a literal video port from an editor document. The host
refreshes a transient map of provider-declared default output references, including
navigation history and locked inspector locations. Linked viewers and editor-time
matching use it; pinned references retain explicit intent. Metadata absence does not
create a false editor/viewer match. The map is omitted from saved workspace state and
cleared from portable presets. PanelContext uses the same provider descriptors.
Shell refreshes after navigation before applying queued seeks. This closes the named-
output routing gap recorded above for linked viewer transport and context selection.
[Automated platform/UI evidence](evidence/m5-viewer-output-result.txt) passes; physical
artist interaction and end-to-end GPU/agent frame publication remain unqualified.


## M4/M5 registered Screen GPU workflow

Committed Motion Text/Fill/Stroke output now passes through the application's shared
scene-evaluation service to the Vulkan vector renderer at quarter/native/double raster
sizes. CPU/GPU agreement, exact timing, a single display conversion, geometry reuse
across separately compiled requests, and held frames across paint revisions pass.
[Evidence and reviewed captures](evidence/m5-screen-gpu-result.txt) record maximum
linear error 0.00048828125 and one display byte. RGBA16F is an explicit test storage
step after evaluation; the viewer worker's default precision policy is unchanged.
This does not qualify physical presentation, asynchronous consumer freshness, agent
frames, ACES GPU publication or mixed/world output through the application service.

## M4 shared spatial image source

The shared image graph now accepts immutable spatial scenes with exact request
bindings. Its GPU renderer retains a Bevy adapter on the same Host and passes each
RGBA16F image directly into downstream operations with completion dependencies.
Two sources in one graph remain distinct across adapter updates. Region evaluation
preserves camera projection and copies crops in the actual source texture format.
[GPU and CPU contract evidence](evidence/m4-spatial-source-result.txt) covers HDR,
premultiplied ordering, retention, cropping, descendant error propagation and recovery.
This is the shared rendering boundary; Motion mixed-scene compilation and application
publication remain pending. ACES spatial sources currently fail explicitly; no implicit
color reinterpretation or CPU raster fallback is supplied. Aggregate Bevy memory,
picking, Camera Plane and physical/agent acceptance remain open.

## M4/M5 Motion mixed image compilation

Motion now evaluates each requested named Scene output once and compiles ordered
Screen/World image passes through the shared video provider. Consecutive World
entries share camera, lights and depth in one spatial pass. Screen entries delimit
passes and composite in authored order; a subsequent World pass starts fresh depth.
There is no implicit Screen-to-World conversion. All passes share one conversion
budget and cancellation callback, with a bounded image graph. Existing Screen-only
and spatial preparation APIs reuse the same converters.

World and mixed GPU rendering require the `spatial` build feature; provider
inspection reports this availability. The ordinary desktop feature does not yet
enable it automatically. Spatial ACES conversion and Camera Plane remain explicitly
unavailable. Screen ACES paint conversion retains its existing explicit path.
The application scene service accepts the same named output/time mapping for World
and mixed scenes. The acceptance fixture compares Screen background / World text /
Screen foreground against analytic premultiplied Over and retains old snapshot
images. This does not qualify async viewer freshness, public agent frame requests,
physical presentation, aggregate Bevy memory, picking or the full M4/M5 scene.

[Mixed publication evidence](evidence/m5-mixed-publication-result.txt) passes 20
Motion conversion/text tests, four existing provider regressions and the llvmpipe
application-service fixture. All 140800 pixels agree with analytic Over within
0.00001; exact timing, old snapshots and held images are retained. Strict Motion
Clippy passes; application Clippy retains its existing shared warnings.

## M4/M5 spatial working-color conversion

Spatial image sources retain linear Rec.709 material/render semantics. When the
shared graph has an ACES project configuration, its input-processor collection
requests that configuration's linear Rec.709 → ACEScg conversion. The existing
OCIO GPU pipeline converts the completed premultiplied source image before
downstream composition. Source completion/error dependencies and texture leases
remain retained, including saturation errors that a gamut matrix could otherwise
hide. Non-ACES sources retain their direct RGBA16F image path. No new color engine,
CPU image transfer, material reinterpretation or display transform was introduced.

This supersedes the spatial-ACES limitation in the preceding implementation
records. Motion compiles the same World source for either profile; Screen paint
keeps its explicit authored-color conversion. Camera Plane, physical presentation,
agent frame publication, aggregate Bevy memory and full M4/M5 acceptance remain open.

[Spatial ACES evidence](evidence/m5-spatial-aces-result.txt) passes the pinned
bundled profile on llvmpipe: maximum CPU/GPU error 2.98e-8, exact alpha, display
agreement within one byte, retained images and upstream saturation rejection.
Both registered mixed Motion fixtures (Rec.709/ACES) pass analytic composition,
exact timing, working/configuration identity and old snapshot retention.

## M4/M5 desktop spatial and preview identity

The ordinary application `desktop` feature now includes the qualified spatial
renderer feature, alongside native video. CPU-only builds retain their explicit
unsupported-spatial diagnostic. The combined desktop/spatial binary check passes;
this supersedes the optional-desktop-enablement limitation recorded above.

A new asynchronous preview-worker fixture exposed a remaining hard-coded `video`
port in `DesktopState::preview_key`. Preview state now carries a provider-declared
output name, populated from explicit preview requests or default-output resolution
during session refresh. Unresolved state cannot produce a preview key. Shell key
construction and overlay freshness checks use this identity directly; their manual
port rewrites are removed. The native probe declares its fixture's actual video port.

[Desktop spatial/preview evidence](evidence/m5-desktop-spatial-preview-result.txt)
passes exact named-output/time routing through the GPU worker, two-consumer result
publication, direct/worker pixel equality and held-image retention. Seven transport/
scheduling regressions and 110 UI tests pass, with six existing native/GPU tests
ignored. Ordinary desktop compilation and focused Clippy pass. Physical artist
interaction, normal/narrow visual review and full M4/M5 acceptance remain unverified.
