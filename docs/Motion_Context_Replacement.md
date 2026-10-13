# Motion, 3D and Compositing replacement

Status: active experiment. This document owns the replacement described below.
It does not claim that the replacement is complete or artist-qualified.
Current state: shared documents, artist adapters and a retained GPU worker are
implemented and qualified at selected seams. Application assembly now registers
new Motion and independent 3D providers; the desktop source registers the new
Motion panels. App cutover qualification and deletion are in progress. All five
slices remain open; native artist acceptance and full removal are outstanding.

## Current status — 2026-10-12

This summary is the current status. Dated evidence below records the state at
each checkpoint; an older “open” item can have later implementation evidence.
Implemented behavior, seam checks and artist acceptance are distinct.

| Slice | Implemented and checked | Work still required |
| --- | --- | --- |
| Shared 2D document | Native schema-4 shapes/text/paths/groups/scripts/image objects, solid, linear/radial gradients and image-pattern paint, stable IDs, scalar/anchor/handle animation and compatible Linear/Hold/HoldNext and aligned Bézier animated subdivision, guarded artist/agent transactions and undo; declared script image controls and conversion of declared images/full-tile image patterns | Broader image/color support, complete path animation controls and resource ownership |
| fframes integration | Pinned 1.2.0, retained compiled Rust scripts, runtime controls, shared upstream SVG/Skia rendering, native floating-point GPU sharing, precise GPU paint and signed/HDR shader transport; mixed Float EXR parity | Broader image/color workflows and accounting beyond Skia’s budgeted cache; native package artist acceptance |
| Artist tools and agent workflow | Canvas/pen/path editing, persistent point targets and numeric coordinate controls, hierarchy, inspector, group graph navigation, script controls, conversion at explicit time, downstream immutable candidate frame review | Remaining vector workflows and broader animation-review acceptance; Motion normal/narrow visual review and release canvas Bézier interaction accepted by the user |
| Context cutover | New Motion and independent 3D providers registered; basic 3D hierarchy, object creation, transform, geometry/material and camera/light controls, hierarchy copies, independent resources, viewer picking and move/rotate/scale tools, exact animated world-preserving parenting; superseded Motion crate/protocol paths deleted; current package/ownership CI checks pass | Native parenting acceptance, full deletion audit and current checkpoint validation |
| Complete slice | Mixed Motion/3D composition and Timeline save/reopen; candidate/Apply workflow; exact linear Float RGBA EXR parity, including signed/HDR shader values, in premultiplied and straight alpha for the tested fixture | Other delivery/color cases and HDR display, production frame speed, native artist/agent acceptance and complete slice audit |

Latest application 10,000-dot measurement: 42.482ms warm median and 45.123ms p95
at 640×360. Native warm median is 15.962ms after the startup fix. These measure
graph evaluation and display output, not full UI FPS or performance acceptance. Do not turn partial evidence into a completion percentage.

Next implementation priorities are image/resource support and accounting, remaining
vector workflows, 3D workflow acceptance and measured frame-cost reduction. Native artist review remains
a separate required gate; backend and ImGui fixture checks cannot replace it.

## Product contract

- Motion owns 2D paths, text, images, groups, animation and fframes script objects.
- 3D owns spatial geometry, instances, materials, lights and cameras.
- Compositing combines their image outputs with footage. Timeline arranges outputs.
- Artists and agents edit one authoritative document through the same transactions.
  Stable object and path-point IDs survive edits. Undo, protection, revision guards
  and unknown extension data remain shared requirements.
- Native objects support direct vector editing. Script objects expose artist
  controls and own their generated content. A script is one graph object, even
  when it generates thousands of shapes.
- Convert to editable captures script output at an explicit time. It does not
  claim to recover the script's procedural or animation logic.
- The graph projects the document. It is not an independently stored second model.
- fframes executes scripts and renders Motion. Native objects and script outputs
  must use the same SVG-tree/Skia path. Do not write another vector rasterizer.

## Replacement policy

There will be one active implementation of each function. No old document reader,
protocol alias, compatibility shim or fallback renderer may remain in the final
application. Unsupported payloads remain opaque through generic project storage.
Source history lives in Git. Keep unrelated worktree changes and artwork intact.
Keep independently valid project, animation, color, render and control services.
Delete superseded implementation, dependencies, tests and guidance in the slice
that replaces their behavior. Intermediate implementation work is not acceptance.

The old rewrite and architecture documents named in AGENTS.md are missing from
this checkout. This user-authorized replacement supersedes their motion scope;
do not reconstruct old acceptance or resume historical phases.

## Implementation order

1. **Shared 2D document.** Typed objects and path points, group hierarchy, stable
   controls, exact-time animation, graph placement and atomic guarded edits.
   Native serialization and agent edits use the same model as artist tools.
2. **fframes integration.** Pin upstream; run actual native fframes code in a
   worker; retain compiled scripts and runtime parameter state. Render native SVG
   trees and script output through upstream Skia. Qualify transparent floating
   point pixels and exact rational-time adaptation. No UI-thread compilation.
3. **Artist tools and agent workflow.** Canvas selection, shape and pen tools,
   point/handle editing, transforms, ordered object hierarchy, inspector, script
   source, exposed controls and graph projection. Use shared UI SDK and gesture
   transactions. Qualify both directions of artist/agent editing and undo.
4. **Context cutover.** Register only the new Motion provider. Extract a real 3D
   package from independently valid spatial behavior; remove 2D and mixed-context
   behavior there. Delete old Motion graph/evaluator/source language and custom
   2D rasterizer. Update app assembly, CLI/MCP schemas and the public Fold skill.
5. **Complete slice.** Motion and 3D outputs in Compositing and Timeline; save and
   reopen; script controls without recompilation; conversion to editable shapes;
   rendering/preview/export parity; deletion audit and artist review.

## Focused qualification

Use a few tests at real seams, not broad new test infrastructure. During the
experiment check changed crates and run targeted tests. Do not repeatedly run
workspace builds or full suites. At cutover, run the checks needed to prove the
new application builds and that replaced implementations no longer execute.

Qualification scenes: editable Bézier strokes with subpixel motion; animated
typography; 10,000 colored dots controlled by a script; a 3D clone scene; a mixed
composition. Record actual quality, warm frame latency, update/compile latency,
memory and hardware. No performance claim without measurement. GPU interop,
signed/HDR color and alpha must be explicit. CPU pixel upload is not acceptance
for a fast final GPU integration.

## Evidence and open work

- 2026-10-11: studied the upstream `fframes-video` skill (current main) and
  design/API/audio references at the runtime pin. Added the Fold skill's
  motion-craft reference with brief, native-versus-script choice, exposed controls,
  SVG caching, motion design and pixel review. Upstream standalone CLI commands,
  integer-FPS video ownership and AudioMap are not Fold APIs. Full public contact
  sheets, onion frames, timed diagnostics, audio analysis and preview/export
  review remain open. Documentation adoption does not qualify those tools.

- 2026-10-11: inspected current construction/evaluation/render/UI/provider seams.
  Existing screen vectors bypass Bevy. Spatial clone colors allocate materials
  per clone (1,024-material cap). Spatial AA is off; two Bevy updates run per frame.
  These are source findings, not fresh profile measurements.
- Upstream pinned to `b644f22cf5be2333e7098caf7ec064c3d36aeb02` (fframes
  1.2.0). Read native macro, SVG-tree, Skia, Vulkan and frame interfaces from the
  fetched source. No upstream source was copied into Fold.
- Added `fold-motion-canvas`: stable native objects, Bézier anchors/handles,
  rectangle/ellipse/text/group/script content, shared scalar animation, runtime
  controls, expected-value edits, hierarchy protection, snapshot transactions,
  extension preservation and opaque rejection of unsupported document schemas.
  Image objects, path-point animation and script-output conversion remain open.
- Added source generation for ordinary compiled fframes Rust modules. Build
  identity depends on script source and bridge identity; controls and native
  artwork remain runtime data. Compiler process supervision/cache integration
  is implemented below; diagnostics in the editor remain open.
- Added `fold-motion-context`, the host document adapter for the new canvas.
  Creation, inspection, schema discovery, guarded authoring, parameter discovery
  and exact-time scalar animation use existing platform contracts. The command
  stages the same atomic canvas edits used by agent authoring. Numeric controls
  retain their scalar IDs; text, Boolean and color controls retain their control
  IDs. Sampled colors explicitly convert encoded sRGB to straight linear Rec.709.
  Dedicated control edits preserve type, identity, protection and extension data.
  Application registration is now implemented as recorded below; native UI and
  end-to-end renderer acceptance remain required. No new transport aliases or old source-language reader were added.
- Added native vector authoring operations: rectangle/ellipse creation, exact
  cubic splitting (including closed contours), sampled ancestor transforms,
  linked/broken handles and group-aware translation. Moving an anchor retains
  its relative handles; selected descendants of selected groups move once.
  These operations produce the same guarded edits used by the host adapter.
- Added optional Motion UI panels through the shared UI SDK: a single-row
  canvas tool strip with overflow, rectangle/ellipse drawing, a pen path draft,
  anchor/handle editing, object movement and an ordered object list. The
  inspector uses shared animation/color controls, native text settings and
  exposed script controls. Source stays in a separate draft until Apply;
  concurrent source changes retain that draft and show a conflict. Gesture
  ownership, time/revision changes and playback cancel pending previews.
  The desktop source now registers these panels. Native visual review at normal
  and narrow widths remains open. Picking currently supports rectangle/ellipse
  content, selected path anchors/handles and native path fills/strokes. Path picking
  uses adaptive curve geometry in screen space, nonzero/even-odd fill rules,
  implicit fill closure, evaluated stroke width and a minimum pointer target.
  Geometry qualification covers holes, open contours, transparent paint, zoom
  and nonuniform transforms. A focused ImGui input test selects an unselected
  curve, moves it and undoes that move at normal and narrow viewer sizes.
  These are automated interaction checks, not native visual artist acceptance.
  The Pen tool now inserts an anchor on a selected curve through the existing
  exact cubic split and guarded document transaction. Screen-space targeting
  refines the original curve parameter, including collinear nonlinear cubics;
  nearby endpoints remain direct anchor targets. The tool adds no toolbar button
  and keeps the object selected. A focused native ImGui input test verifies one
  undo entry, redo, retained endpoint IDs and no extra object. A geometry test
  verifies transformed collinear curves and closed/open contour edges; the
  existing fill/stroke picking test also passed after the shared flattening change.
  These checks do not replace native visual review. Stroke-cap/join boundary
  precision, text/script picking and full hierarchy authoring remain open.
- Added shared document hierarchy operations for grouping and drawing order.
  The graph and object list now use one membership/placement implementation.
  The object list supports Control-click multi-selection, Group selection
  (Control-G), Bring forward/front, Send backward/back, and contextual object
  deletion. Actions use the existing guarded preview/commit lifecycle; playback
  and pending commits disable authoring. Grouping requires siblings and creates
  an identity group at the frontmost selected position. It preserves each
  object's IDs, transforms, animation and extension data; noncontiguous grouping
  may change overlap with unselected siblings. Drawing-order actions retain
  selected-object order and edit only selected objects, including when moving
  past protected unselected siblings. Focused qualification passed for animated
  ancestor coordinates at three exact times, object/extension preservation,
  group/order guards, all four ordering actions and a single project undo.
  The existing graph/agent transaction test passed after removing duplicate
  hierarchy lookup code. The affected UI crate compiles; an initial test-only
  import failure from extracting the list was fixed. Native list/menu visual
  review, arbitrary reparenting, ungrouping and app registration remain open.
- Added the Motion graph adapter to the shared graph canvas. Each native object
  is one node; scripts remain one node regardless of their generated content.
  Group inputs project the document's ordered child IDs. Connect/disconnect and
  batch rewiring edit that hierarchy; creation, deletion and drawing-order actions
  stage the same guarded canvas transactions. Graph positions are optional until
  authored, so initial layout can be automatic and an authored zero remains exact.
  A separate position edit guard permits layout of protected content without
  changing its artwork or protection. Object-list and graph creation share defaults.
  This is a document/graph implementation; app registration and native graph
  visual review at normal and narrow sizes are still open.
- Added `fold-motion-fframes`, an isolated native worker workspace: native objects
  and script output build one compile-time SVG tree, consumed by the upstream
  Skia renderer. Retains conversion/drawing caches, uses a linear-sRGB RGBA32F
  readback from a linear-sRGB RGBA16F GPU surface for qualification. This is
  now configured by application assembly. Native GPU image-graph
  qualification is recorded below; product Compositor/Timeline workflows remain
  open. Reference readback remains an explicit qualification path.
- Automated: `cargo check -p fold-motion-canvas --offline` passed;
  `cargo test -p fold-motion-canvas --test editing --offline` passed (3 tests:
  artist/agent point edits with stale guards/save/reopen/undo; protected hierarchy
  and cycle rejection; control edits without changing program identity).
- Automated: `cargo test -p fold-motion-context --test authoring --offline`
  passed (one host-contract workflow: create/inspect, artist command then agent
  control edit, stale expected values and changed scalar identities rejected,
  runtime sampling, exact-time animation, undo/redo and protected controls).
  The test compiles the affected document adapter and shared canvas. This proves
  host-contract behavior, not native UI or CLI/MCP acceptance. An initial check
  exposed a missing ID parsing implementation; the adapter now uses the ID's
  existing serialization contract. No workspace build or full suite was run.
- Automated: `cargo test -p fold-motion-canvas --test tools --offline` passed
  (one vector workflow: samples of the original/split cubic agree within 1e-10,
  closed-edge splitting retains IDs, anchors/linked handles use animated group
  coordinates, translations avoid moving selected descendants twice, singular
  transforms reject edits). `cargo check -p fold-motion-context --features ui
  --offline` passed. It reports existing unused YUV members in `fold-render`;
  no new UI warnings remain. These checks do not establish artist acceptance.
- Automated: `cargo test -p fold-motion-context --features ui --lib
  native_canvas_drag_previews_cancel_then_commit_one_undo_entry --offline`
  passed. Native ImGui mouse/key events drive a real anchor drag through the
  host edit contract: transient edits use the preview document, committed
  coordinates remain unchanged during the drag, Escape discards the preview,
  release commits one undo entry, undo/redo retains the anchor ID and handles.
  This is an input/transaction test without GPU artwork or visual acceptance.
- Automated: `cargo check -p fold-motion-context --features ui --offline` passed
  for the graph adapter. `cargo test -p fold-motion-context --features ui --lib
  --offline` passed (the native drag test and a graph/agent workflow: native object
  creation, group connections, script control edits without graph expansion,
  atomic cycle rejection, rewiring, undo/redo, locked-object layout and stale
  position guards). The graph test also checks ordered replacement of multiple
  group slots when removal events erase preceding inputs. Canvas edit/vector
  tests passed after the optional-position model change (four focused tests).
  At that qualification the application still registered the old Motion package;
  those checks do not establish app cutover or graphical artist acceptance.
- Automated: `cargo check --manifest-path crates/motion-fframes/Cargo.toml
  --offline` passed after correcting macro/borrow errors. The generated native
  worker compiled and rendered native typography plus 10,000 procedural dots at
  640 × 360 through actual fframes Rust macros and Skia Vulkan. Three frames
  proved animation changes pixels and a runtime Count edit to 8,000 reduces
  drawn coverage without rebuilding/restarting. Finite float pixels, bounded
  alpha and native-text/script regions were checked. RGBA32F GPU allocation
  failed on this device; the single chosen target is RGBA16F, with RGBA32F readback.
  See [rendered frame](../resources/motion-context/fframes-10000-dots.png) and
  [pixel evidence](../resources/motion-context/render-evidence.json). This is a
  debug-build correctness check, not a latency benchmark or color reference.
  Native dependency download required network access; no workspace test suite
  or desktop build has been run.
- All five implementation slices remain open. Native artist interaction,
  complete quality/color/performance qualification, app/provider cutover, 3D
  extraction and deletion of the superseded implementation remain unverified.
- Added the off-UI-thread retained runtime. Program identity includes exact
  bridge/shared source and lock hashes plus explicit Cargo/Rust compiler version
  output. A successful binary is cached with a checksum and a unique Cargo binary
  name. Source changes invalidate builds; runtime controls do not. The runtime
  retains one worker, sends fonts only on initialization/change, admits one demand
  at a time, validates bounded headers/request IDs/pixel sizes/finite values,
  and accounts request/readback allocations through shared media leases. Readback
  storage is immutable to callers. Final GPU transport remains open.
- Reused the media process supervisor for compiler and renderer children. Unix
  children use a process group; cancellation/timeout/drop kills descendants and
  reaps the leader before joining a blocked pipe reader. Other platforms' child
  containment, worker heap/GPU allocation limits, offline compiler packaging,
  persistent disk-cache eviction and editor diagnostics remain open.
- Automated: affected context check and the three canvas editing tests passed.
  One runtime transport test passed: retained parameter requests use one cached
  binary without compiler invocation; stale response IDs retire the worker;
  cancellation releases pipes held by a descendant; the worker can restart;
  changed bridge source rejects the cached identity. This fixture does not
  qualify fframes pixels or cross-platform process containment.
- Actual native runtime qualification: prepared a fresh 640x360 scene and rendered
  three demands through the runtime (native typography plus 10,000/8,000/10,000
  script dots). Exactly one program-cache directory was produced. The pixel
  verifier passed request IDs, format/dimensions, finite values, alpha range,
  typography/dot coverage, control edits and animation changes. Initial demand
  including compilation took 39.79s; subsequent demands took 237.93ms/283.94ms
  in this debug CPU-readback run. These are diagnostic timings, not performance
  acceptance. Optimized profiling and GPU compositor integration remain open.
- Added explicit debug/release worker profiles to the runtime; the chosen
  profile is part of the compilation identity. Opt-in `FOLD_MOTION_PROFILE=1`
  emits correlated phase timings on stderr, leaving frame bytes unchanged.
  Reports separate document/script construction, SVG conversion, Skia command
  generation/submission, readback and pipe transfer. GPU completion is included
  in readback, not claimed by CPU submission timing.
- Environment evidence changes interpretation of the earlier timings: sandbox
  Vulkan exposes only Mesa llvmpipe (CPU device). Native device access exposes
  GPU0 NVIDIA GeForce GTX 1070 (driver 580.173.02), then llvmpipe. The upstream
  backend selects the first Vulkan device. These environments cannot be compared
  as a renderer speedup benchmark. The optimized dependency build required the
  pinned Skia native package download; Cargo offline alone does not prevent
  dependency build scripts from attempting downloads. Offline packaging remains
  an explicit open requirement.
- Optimized worker/native-device diagnostic run: 640x360, native title and
  10,000/8,000/10,000 script dots. Warm SVG conversion took 12.33/18.61ms;
  command generation 4.15/5.21ms; readback 2.10/2.72ms; pipe transfer
  2.43/2.44ms. The debug parent took 129.10/127.14ms end to end. Replaced its
  per-component float decoding with an accounted float buffer filled directly
  from the pipe, retaining finite-value/alpha validation and explicit little-endian
  handling. A subsequent run took 83.16/89.33ms with the same debug parent and
  optimized worker profile. These three-demand diagnostics are not a statistical
  benchmark, 60fps acceptance or final GPU integration. See
  [phase evidence](../resources/motion-context/profile-evidence.json).
- Focused runtime workflow passed after the reader/profile changes, including
  non-finite-pixel rejection. Actual optimized runtime output passed the pixel
  verifier again. Compiler failures now retain the bounded tail of stderr,
  including a final Unicode error after a long log; the same focused workflow
  verifies this. No workspace build/full suite was run.
- Added the feature-independent `fold-gpu-sharing` module for Linux Vulkan image
  exchange. It exports immutable dedicated RGBA16F memory, matches receiving
  device/driver UUIDs, bounds description/allocation sizes, and transfers exactly
  one CLOEXEC FD through a Unix socket. No CPU pixels cross that socket. Vulkan
  FFI is confined to an explicitly allowed module; importing a producer FD and
  publishing a backend-drawn image have documented unsafe provenance/layout
  requirements. This is a native graphics interface, not a script security sandbox.
- The receiver records the external-image acquire/copy through wgpu's Vulkan
  encoder interface and normal queue submission. Destination creation and memory
  initialization commit before the external write; an initial attempt in the
  same command batch produced blank pixels and was replaced. HAL resource guards
  are scoped before other wgpu calls; an initial nested-lock failure was fixed.
  The imported source and compositor-owned texture remain retained through queue
  completion. Production aggregate GPU accounting, lost-device/timeout recovery,
  pooling and frame snapshot publication binding remain open.
- Actual native check: `share_image` started a supervised separate producer,
  exported a 32x16 image, transferred its FD and copied it into a wgpu-owned
  texture on NVIDIA GeForce GTX 1070. Every verified RGBA16F pixel retained
  `[1.5, -0.125, 0.25, 0.5]`. CPU readback occurred only in the qualification
  example. A run with `VK_LAYER_KHRONOS_validation`, an explicit error logger,
  and captured producer diagnostics passed with no validation errors; reported
  warnings concern an unavailable DRM extension and the requested validation
  layer. See [GPU sharing evidence](../resources/motion-context/gpu-sharing-evidence.json).
- One focused socket test passed outside the tool sandbox: owned CLOEXEC FD
  transfer and rejection of extra descriptors. The sandbox attempt failed while
  setting the socket receive timeout (`Operation not permitted`); native execution
  passed. The shared process supervisor now exposes completion status and bounded
  retained diagnostics for native jobs, instead of duplicating process management.
- The clear-color fixture qualifies signed/HDR transport. The subsequent artwork
  check below qualifies actual fframes rendering through the same GPU boundary.
- Wrapped exportable RGBA16F images as actual Skia render targets. Native objects
  and compiled fframes scripts use the same retained SVG conversion/drawing caches,
  font database and receiving-device UUID. GPU mode publishes one immutable image
  FD per demand and holds its producer allocation until a correlated completion
  acknowledgement. Its JSON response carries no CPU pixels. Standalone reference
  readback is explicit; a GPU transport failure does not switch to CPU transport.
- Skia drawing, flush, readback and context/surface destruction share the owning
  Vulkan queue lock. Image destruction waits for pending queue work, including
  foreign-backend error/unwind paths. Publish failures release Skia access before
  freeing its backing image. Producer work remains in a supervised process;
  aggregate allocation accounting and driver-hang/device-loss recovery remain open.
- Added a compile-only `Runtime::prepare` entry that uses the same checked cache
  and source/toolchain identity as reference rendering. The bridge identity also
  includes the GPU-sharing source. Changed controls/native artwork stay runtime
  data. Worker profiling now names the output mode (`gpu_publish` or
  `cpu_readback`) rather than calling both operations readback.
- Actual final-source native check: `share_program` ran a compiled release worker
  on NVIDIA GeForce GTX 1070 with `VK_LAYER_KHRONOS_validation`. Native typography,
  10,000 script circles, animated output and a runtime count change to 8,000 passed
  the pixel verifier after import/copy into a wgpu-owned texture. No CPU pixel body
  crossed the worker transport and no validation errors were reported. Warm
  requests measured 30.85 and 36.80 ms, including qualification readback; the first
  request measured 1,316 ms. These three requests do not establish app performance.
  See [artwork evidence](../resources/motion-context/fframes-gpu-evidence.json),
  [captured diagnostics](../resources/motion-context/fframes-gpu-diagnostics.txt)
  and [rendered frame](../resources/motion-context/fframes-gpu-frame.png).
- The same target backend's explicit Skia RGBA32F reference readback differs from
  GPU RGBA16F copy/readback by at most 0.00006056 per component in each tested frame
  (qualification tolerance 0.0001). This is not bit-exact equivalence or complete
  preview/export parity. The verifier now records the supplied worker build profile
  instead of always labelling evidence debug; its count edit selects the named
  control instead of relying on map order. No workspace build/full suite was run.
- Next: bind demands to immutable document/time/revision intent, reserve producer,
  import and target memory, and connect the renderer to the application's generic
  image service. Application Motion GPU output, artist acceptance and app cutover
  remain open; no CPU-upload fallback was added to the application.
- Added feature-independent `Host::reserve_shared_image` / `SharedImage::receive`
  ingress behind the render package's `shared-images` feature. Host reservations
  precede native import and destination creation, enforce dimensions/device/driver
  and retain the shared incoming allocation and its distinct RGBA16F copy. Imported
  storage, destination and reservations survive both initialization and copy
  submissions, including caller abandonment and errors after initialization. The
  destination frame owns its reservation through all downstream use. Native FD
  provenance remains the native adapter's responsibility; no unsafe code or
  feature document models entered the render host.
- Shared frames use the existing immutable `GpuFrame` descriptor, exact-time
  attachment, completion/status machinery and explicit reference readback. One
  retained validation pipeline per host checks finite components and alpha range
  on the GPU before readiness; only status/timing bytes are mapped. GPU-local copy
  traffic has its own statistic and is not reported as CPU upload. Color space
  and premultiplied-alpha semantics are explicit native-adapter requirements.
- Focused check: `cargo check -p fold-render --features shared-images --offline`
  passed. The native `share_program` example compiled and ran the final host path
  with Vulkan validation on GTX 1070. The artwork pixel verifier passed again.
  The same workflow checked oversized budget and wrong-device rejection, signed
  HDR preservation, invalid-alpha rejection through GPU status, and zero retained
  host accounting after completed and abandoned frame drops. Existing unused YUV
  members remain compiler warnings. No workspace build/full suite was run.
  See [host evidence](../resources/motion-context/fframes-host-evidence.json),
  [diagnostics](../resources/motion-context/fframes-host-diagnostics.txt) and
  [frame](../resources/motion-context/fframes-host-frame.png). This qualification
  uses a debug host/reference-readback example and a release fframes worker;
  end-to-end timings include that CPU reference work and are not preview timings.
- Still open: reserve producer memory before rendering (the incoming reservation
  currently starts after export), account retained Skia/driver allocations, bind
  a retained application worker to this host ingress, and publish only the current
  immutable document/time/revision demand. The generic frame path is implemented;
  application/provider registration, image-source graph integration, preview/export
  parity, native artist review and old-system removal remain open.
- Added the retained application-side `runtime::gpu::GpuRuntime`, sharing the
  checked compiler/cache with explicit reference rendering. Script source changes
  retire the old process and prepare/start a new program; controls/native artwork
  remain bounded runtime requests. Font bytes transfer on initialization/change.
  The worker uses the actual receiving host UUID and a private Unix socket. Each
  request has a monotonically increasing transport ID; headers, dimensions,
  zero CPU payload and allocation descriptors are checked before native import.
  Source/copy reservations and GPU validation use the generic host ingress above.
  The child is acknowledged only after GPU completion/status succeeds. Normal
  runtime output remains GPU-resident; reference readback is confined to the
  qualification example. No GPU failure switches to a different transport.
- Motion GPU snapshots decode supported immutable project documents and attach
  epoch, demand generation, document ID/revision, exact time and dimensions.
  A private completed result exposes its frame only through `into_current`, which
  rejects changed intent and rechecks readiness/device failure. Generation must
  change for transient previews and changed runtime inputs, including fonts and
  resources. Frames attach exact rational timestamp/duration. This implements the
  publication guard; calling it from active application preview/export is open.
- Native check: `render_gpu_program` on GTX 1070 completed eight demands with one
  retained worker, including the 10,000-dot script and runtime count change. It
  rejected stale epoch, revision, generation, time and dimensions and retained
  zero GPU accounting after dropped results. The three recorded requests took
  1,014 ms (initialization/cache demand), 63.60 ms and 72.83 ms excluding reference
  readback, with a debug host and release worker. The pixel verifier passed.
  These are fixture measurements, not artist acceptance or app performance.
  See [retained-runtime evidence](../resources/motion-context/retained-gpu-evidence.json)
  and [frame](../resources/motion-context/retained-gpu-frame.png).
- One focused native lifecycle workflow passed: a stale response ID was rejected;
  a worker and descendant held the GPU socket while the reader reached FD receive;
  retirement killed/reaped them and joined the reader within two seconds rather
  than waiting for its 30-second socket timeout. Cancellation/deadline/error paths
  retire the process before joining IO and preserve bounded diagnostics. Native
  FD import is confined to a small explicitly allowed module; other Motion context
  code denies unsafe operations. Scripts are trusted native code, not sandboxed.
  The affected GPU example compiled; existing unused YUV warnings remain. No
  workspace build/full suite was run.
- Next: producer preallocation reservations and retained Skia memory accounting;
  provider/image-source and application preview/export binding; remaining native
  vector tools and conversion; independent 3D package extraction and cutover.
  All five slices remain open. Application registration is now implemented below;
  native app acceptance and full old-engine/protocol removal remain open.
- Added exact GPU footprint admission before Motion compilation or frame demands.
  The generic Vulkan module queries the same unbound dedicated image description
  and device-local memory type used by the producer, without allocating image
  memory. The render host reserves that shared source, its separate copy and
  validation/timing buffers as one complete admission. Failed admission releases
  partial reservations. Motion sends the exact producer allowance through the
  required private `allocation_limit` field; the native renderer enforces it.
  Changed allowance checks also cover a retained reference target. No optional
  old protocol fields, aliases or fallback transport were added.
- Host source retention now survives the whole producer lifetime. GPU completion
  is acknowledged, then the producer drops its image and confirms the same request
  ID. The host releases source retention only after that confirmation; errors or
  cancellation keep it through process reaping/IO retirement. An abandoned/unwound
  demand retires its worker before another admission. Incoming descriptors must
  exactly match the pre-reserved description, including device, driver, allocation
  size and memory type. Generic copy/status/destination lifetimes stay unchanged.
- Actual native qualification on GTX 1070 passed: eight demands used one release
  worker, the pixel verifier retained typography/dot animation/count edits, and
  every completed/dropped result released host accounting. An oversized demand
  with changed script source was rejected before compilation/worker start; the
  worker-start and completed-frame counts remained one/eight. The existing native
  lifecycle workflow was extended to hold the producer allowance through stale
  response and blocked-FD retirement; it passed in 0.43 s, with retirement under
  two seconds and zero final accounting. The first permission review timed out;
  the permitted retry ran successfully. No safety finding was reported.
  Native bridge check and affected GPU example compilation passed; existing YUV
  warnings remain. No workspace build/full suite was run. See
  [preallocation evidence](../resources/motion-context/preallocated-gpu-evidence.json)
  and [frame](../resources/motion-context/preallocated-gpu-frame.png).
- Producer target preallocation is now implemented. Retained Skia resource-cache
  and driver allocation measurement/accounting remain open; the host's logical
  image budget is not a total physical VRAM cap. Application/provider/image-source
  binding, preview/export parity, remaining artist tools, independent 3D and final
  old-system removal are still outstanding. All five slices remain open.

## Current development commands

### 3D extraction in progress

Added the independent `fold-three-d` package (`fold.3d.scene`). Native spatial
documents own shared geometry/material IDs, instances, group hierarchy, cameras,
directional lights and identity-bearing transform parameters. Transform tracks
reuse the valid shared keyframe implementation and exact rational time. Evaluated
hierarchy retains shear through affine composition; Bevy/ECS state remains behind
the existing Fold spatial render adapter. The provider creates a lit box scene,
describes its timed output and compiles an immutable spatial image-graph fragment.
It is now registered in application assembly; its native artist panels remain open.

Moved valid box/sphere mesh preparation and its tests out of generic render
infrastructure into the 3D package. The old mixed Motion package temporarily
imports that single implementation until cutover deletes the old package; there
is no compatibility alias or duplicate primitive implementation. Focused primitive
and scene tests passed, along with affected-crate checks. Scene qualification
covers resource sharing, exact-time group animation, extension serialization,
cancellation, duplicate ownership, cycles and invalid hidden/inactive light/camera
data. Guarded edits now cover insertion/replacement/deletion, transform parameters,
hierarchy placement, shared geometry/material resources and active-camera selection.
The 3D provider exposes inspect, typed authoring schemas and one worker command
that stages the same edits into an undoable project transaction. Protection covers
ancestors, protected parameters and resources referenced by protected instances.
Final batch checks prevent hierarchy/resource edits from bypassing those guards
and preserve surviving object, parameter, key and resource extension data.
Hierarchy ownership is indexed once for batch protection checks and subtree
deletion; it is transient evaluation data, not a second stored document.

Two focused package-interface tests passed: command/agent edits with stale-value
and stale-project rejection plus undo, and atomic batch protection/extension
preservation. One initial compile failed because shared graph parameters lacked
a generated schema; added the schema derive to that valid shared type. No wire
aliases or duplicate parameter model were introduced. These are automated
authoring checks; native artist controls have not been qualified.

The 3D provider now exposes Position, Rotation and Scale through Fold's shared
parameter and animation capabilities. Stable parameter IDs identify full typed
values and their component controls; displayed paths remain labels. Sampling uses
the same exact-time evaluator as rendering. Ancestor/object/parameter protection
sets control editability. Component curves project the shared coupled typed keys;
curve edits use the existing shared track implementation and guarded transactions.
Clearing the supplied component curves captures the current typed pose as a static
value, subject to protection and extension-preservation checks.
One focused package-interface control test passed for component sampling, units,
coupled key identity, curve edits, protection, repeated-target rejection and static
pose capture. The affected crate check passed. This qualifies the control contracts,
not visible native panels or artist acceptance.

The new 3D package has now passed a focused native GPU image-graph check on the
GTX 1070. Its public package video capability compiled a lit, rotated box with a
native position track at three exact rational times. The GPU output contained
finite float color, valid alpha and visible coverage; animation changed pixels.
No CPU raster adapter or working-image readback was used before the explicit
qualification readback. Inspected the middle frame to confirm visible box faces.
Evidence: `resources/motion-context/three-d-gpu-evidence.json` and
`three-d-gpu-frame.png`. The first native check required a cold five-minute Bevy
dependency build; the rotated fixture rerun reused that build. Existing render
YUV dead-code warnings remain. These debug measurements include readback and do
not qualify performance or final lighting/art quality.

3D panels and resource controls, native animation UI qualification, procedural authoring depth,
preparation caching, app registration
and old-package deletion remain open. This is foundation work, not 3D acceptance.

### Motion image graph

Image-graph integration now has a Motion video provider and a generic immutable
image-source operation. Feature document models remain outside render infrastructure.
Compilation captures document state without starting a native process. Evaluation
retains one worker per provider, supports cancellation while waiting for that worker,
and retires it on receiving-host changes. Fonts remain immutable per provider.
The graph checks host/device/color, carries GPU completion ownership, uses shared
color conversion and handles regional requests through full-frame evaluation.

Focused native qualification on the GTX 1070 passed through `VideoRegistry` and
Fold's normal GPU image evaluator: native text, 10,000 scripted dots, animation,
runtime count edits and a downstream 0.5 opacity operation. Each graph rendered
the captured scene after a later empty-document commit. All three output frames
exactly matched the previously qualified GPU reference multiplied by 0.5. No CPU
upload or working-image readback occurred before explicit qualification readback.
Evidence: `resources/motion-context/graph-evidence.json`,
`graph-pixel-evidence.json` and `graph-gpu-frame.png`.
The affected crate and example checks passed; existing render YUV dead-code warnings
remain. Debug-host measurements include qualification readback and are not performance
acceptance. Application registration, Compositor/Timeline product workflows, ACES
and region pixel qualification, outer application publication guards and native
artist review remain open. All five slices remain open.

```sh
cargo test -p fold-motion-canvas --test editing --offline
cargo test -p fold-three-d --test scene --offline
cargo test -p fold-three-d --test editing --offline
cargo test -p fold-three-d --test controls --offline
cargo run -p fold-three-d --features gpu --example render_scene --offline -- /tmp/fold-3d-context
cargo test -p fold-three-d --test primitives --offline
cargo test -p fold-motion-context --features ui --lib picks_curves_and_fills_with_holes_at_screen_scale --offline
cargo test -p fold-motion-context --features ui --lib unselected_path_is_picked_and_moved_with_one_undo_at_two_view_sizes --offline
cargo check --manifest-path crates/motion-fframes/Cargo.toml --offline
cargo run -p fold-motion-canvas --example prepare_program -- /tmp/new-motion-program
cargo run -p fold-motion-context --example render_program --offline -- /tmp/new-motion-program /absolute/path/to/cargo /absolute/path/to/rustc
python3 scripts/qualify_motion_context.py /tmp/new-motion-program --readback /tmp/new-motion-program/runtime-frames.bin
# Optimized native worker; the parent example remains a debug build.
FOLD_MOTION_PROFILE=1 cargo run -p fold-motion-context --example render_program --offline -- /tmp/new-motion-program /absolute/path/to/cargo /absolute/path/to/rustc release
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo run -p fold-gpu-sharing --example share_image --offline
cargo test -p fold-gpu-sharing --lib --offline
cargo run -p fold-motion-context --example prepare_renderer --offline -- /tmp/new-motion-program /absolute/path/to/cargo /absolute/path/to/rustc
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo run --manifest-path crates/motion-fframes/Cargo.toml --example share_program --offline -- /tmp/new-motion-program
python3 scripts/qualify_motion_context.py /tmp/new-motion-program --readback /tmp/new-motion-program/gpu-frames.bin --worker-profile release
cargo run -p fold-motion-context --features gpu --example render_gpu_program --offline -- /tmp/new-motion-program /absolute/path/to/cargo /absolute/path/to/rustc
python3 scripts/qualify_motion_context.py /tmp/new-motion-program --readback /tmp/new-motion-program/retained-gpu-frames.bin --worker-profile release
cargo run -p fold-motion-context --features gpu --example render_graph_program --offline -- /tmp/new-motion-program /absolute/path/to/cargo /absolute/path/to/rustc
python3 scripts/qualify_motion_context.py /tmp/new-motion-program --readback /tmp/new-motion-program/graph-frames.bin --scaled-reference /tmp/new-motion-program/retained-gpu-frames.bin --reference-scale 0.5 --worker-profile release
cargo test -p fold-motion-context --features gpu --lib rejects_stale_headers_and_retires_blocked_socket_with_descendant --offline -- --ignored
```

The generated script entry is
`pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String>`.
Use `input.number/text/boolean/color` for exposed controls and `input.time` for
exact time (`input.seconds()` only at the numerical animation boundary). The
worker accepts a JSON request line with `id`, `canvas`, `time`, `dimensions`,
required producer `allocation_limit` and
optional font bytes (required on initialization). Standalone reference mode returns
a correlated JSON header and little-endian premultiplied linear-sRGB RGBA32F body.
Native GPU mode requires `FOLD_MOTION_GPU_SOCKET` and the receiving device UUID in
`FOLD_MOTION_GPU_DEVICE`; it returns a correlated zero-body header, transfers the
RGBA16F image FD and waits for the request ID acknowledgement, releases its image,
then confirms that request ID. The reference runtime
enforces timeout, cancellation and CPU memory accounting. GPU process supervision
and transport are qualified by the native example; application revision/time guards,
retained-cache/driver accounting and host publication binding remain required
before this becomes an application capability.

### Application registration in progress

Application assembly now registers only the new Motion document/video provider
and the independent 3D provider, alongside the existing Timeline, Composition
and Compositor packages. Desktop assembly registers the new Motion panels.
Project > New exposes Motion and 3D separately. Composition actions now create
`motion` and `3d` documents at the composition's exact dimensions, rate and
duration; the old mixed `scene` action is rejected without an alias. Motion is
an object graph; 3D is not advertised as a graph until a real graph editor exists.

The Motion video provider publishes output metadata in every build. Pixel
execution requires the qualified Linux GPU feature; other builds return that
requirement rather than selecting another renderer. Application GPU features
now activate the retained fframes worker. Assembly configures release worker
programs, bundled Noto Sans, compiler/frame deadlines and compiler paths without
starting compilation or performing filesystem I/O on the UI thread. The
experiment defaults to the checkout's native bridge and target directories;
`FOLD_MOTION_BRIDGE`, `FOLD_MOTION_TARGET` and `FOLD_MOTION_CACHE` configure these
locations. Cache defaults use the user cache directory. Installed bridge/toolchain
packaging is still open.

Focused app evidence: Project > New creation/undo/network capability checks
passed for all five contexts. A composition workflow passed for creating Motion
and 3D outputs, published metadata, provider validation and single-transaction
undo. The fixture initially compared the uncommitted document revision; it now
compares the committed baseline. Production dependency inspection excludes the
old `fold-motion` crate. At this checkpoint, superseded tests still depended on
that crate; the later retirement evidence below records their replacement.

Build friction: the application's existing source-identity script recursively
hashed the new native workspace's generated `target` tree. Stopped that build
script, excluded generated target/Git directories and stopped registering whole
directory trees as source inputs. The queued focused app test then built and
passed with the corrected scan. Desktop compilation, native artist review and
application GPU preview/export acceptance are separate requirements below.

Desktop compile evidence: `cargo check -p fold-app --features desktop --offline`
passed. This compiles desktop panel assembly and the GPU-enabled Motion provider,
including the existing spatial adapter. It does not prove native visual artist
acceptance or actual application preview/export pixels. Focused app creation and
composition tests passed without a workspace/full-suite run. Full-suite retirement
and deletion checks remain open with the superseded test fixtures.

The app's candidate checks exposed a missing scoped-proposal capability on the
new providers. Added one shared native authoring adapter over the existing
Prepared/inspection/publication service and provider-owned record guards for
Motion and 3D. Requests use the native expected-value record and the same typed
edits as artists; derived output/input descriptions added by app inspection do
not participate in the expected-value guard. Whole-document proposals now use
baseline protection/extension checks also used by typed edit batches. Graph
positions remain independently editable for protected Motion artwork.

Focused evidence: app cancellation tests now pass with new Motion documents.
A single app candidate workflow passed for Motion and 3D typed authoring,
inspection, stable affected object IDs, publication, preserved unknown document
extensions, undo, stale expected values and rejection of protection changes.
Concurrent native document changes require inspect/prepare again; idempotent
reconciliation remains allowed. Granular merging and object-scope proposal
inspection are still open. Existing canvas and 3D editing seam tests passed after
sharing baseline guards. These are host-contract checks, not external CLI/MCP
or native visual acceptance. The initial missing proposal capability and test
fixture field/revision errors were fixed; no fallback engine was added.

Final affected checks for this registration/proposal change: the Motion graph/
agent guard-and-undo test passed with the shared baseline gate; desktop compile
passed again after the proposal adapters (`cargo check -p fold-app --features
desktop --offline`, 10.46 s warm). No native window review, full workspace suite
or application GPU pixel acceptance was claimed. Old dev fixtures/dependency,
protocol/SDK guidance, 3D artist panels, remaining vector tools and conversion,
performance and preview/export acceptance remain open.

### Engine and wire retirement evidence

Deleted the retired `crates/motion` implementation, workspace membership and app
dev dependency. Removed its obsolete source/evaluator/mixed-scene acceptance
fixtures and maintenance example. Preserved generic candidate, retry, session,
parameter-placement, composition and provider tests using current native models.
The lockfile no longer contains `fold-motion` or Rhai. User artwork remains intact.
The independent Compositor still uses shared vector coverage for Roto and masks;
this active consumer needs its own backend decision before claiming complete
custom-rasterizer removal.

The closed wire schema now accepts provider-owned native edit requests and
project transactions. Removed source/patch routes and stale discovery limits.
The real CLI/MCP subprocess test passed native document inspection, a parameter
proposal, candidate inspection, Apply and exact replay. Generic session/retry,
provider capability, composition and exact-time placement tests passed after
native fixture replacement. Motion geometry parameters now report canvas-distance
units; numeric script controls retain scalar units.

`cargo test -p fold-app --test current_contracts --offline` passed all five tests:
retired records remain opaque and cannot execute, current provider documents
save/reopen, and retired wire routes are rejected. `cargo check --workspace
--all-targets --offline` passed (4.38 s warm); existing render YUV dead-code
warnings remain. These checks do not qualify pixels, performance or native UI.
The Fold skill now describes the verified native workflow and preserves the
lossless JSON guard and historical artwork links. Further protocol cleanup,
installed packaging, 3D artist panels, remaining vector tools, conversion,
public review tools and application preview/export qualification remain open.

Cutover validation in progress: `cargo test --workspace --offline` is compiling
the app/test targets; no suite result is claimed. The process was confirmed live
with active compiler children. Full-suite result and Clippy remain pending.
Corrected stale MCP tool metadata that claimed file writes/rendering were absent:
it now describes save-copy and capability-gated frame observations. Added the
current `observe_frame` request shape and display-pixel interpretation to the
public guide, explicitly separating schema evidence from pending native Motion
pixel qualification.

The packaging source inventory also recursively included generated native-worker
target files. Updated it to prune `target` and `.git` before scanning Rust/WGSL
and Cargo manifests. A direct inventory check found 577 source inputs, included
the native bridge manifest and canvas model, and confirmed generated trees were
excluded. No release package was built; bridge/toolchain delivery remains open.

Added contextual Motion inspector controls for stroke cap/join, even-odd path
fill and per-contour closure. They edit existing native Paint/Contour records
through the same Replace guard and gesture/undo lifecycle; no new document or
toolbar layer was introduced. Formatted the source and checked the diff. UI
compilation and normal/narrow native visual review remain pending while the
cutover test build runs. This does not qualify path-point animation or complete
the remaining vector tool work.

### 2026-10-12: strict document scopes and checkpoint validation

Removed the unused definition/object proposal scope fields and their rejected
routes from the platform schema, dispatch, provider adapters and current
fixtures. Removed the now-redundant scope helper and duplicate candidate scope
check. Current providers expose document scopes; typed edits retain object IDs
inside provider-owned requests. Added strict rejection of the removed scope
fields (including null values) to the existing wire retirement test. Removed the
missed `agent_images` test for the deleted legacy image-cloner demo; generic
image sampling retains independent platform validation.

Motion UI compilation passed with `cargo check -p fold-motion-context --features
ui --offline` (10.71 s), including the new cap/join/fill-rule/closure controls.
After the scope cleanup, `cargo check --workspace --all-targets --offline` passed
(17.35 s). Introduced unused-variable warnings and a stale candidate-scope
reference were corrected. Existing render YUV dead-code warnings remain.

The first workspace test run compiled in 27m06s and stopped at app unit tests:
36 passed, one private Unix socket test was denied by the sandbox. The next
socket-enabled run exposed the stale scope reference, now fixed. The current
socket-enabled `cargo test --workspace --offline` is running; no full-suite pass
is claimed. Clippy, native visual review and actual application Motion frame
observation/preview/export qualification remain pending.

Checkpoint follow-up: the socket-enabled suite passed the app unit tests and
reached the codec integration file, where three concurrent tests hit aggregate
media storage pressure. `cargo test -p fold-app --test video_workflow --offline
-- --test-threads=1` passed all five codec tests (3.01 s execution). A serial
socket-enabled workspace run is now active; its complete output is retained at
`/tmp/fold-motion-cutover-tests.log`. No full-suite pass is claimed yet.

Native review access: the documented computer-use runtime is reachable with
local IPC access, but reports `observation.screenshot: false`. This does not
establish native visual acceptance; normal/narrow review remains open. The
attempt to start the review app was rejected when automatic approval review
timed out; read-only capability inspection worked without restarting it.

Upstream version check on 2026-10-12: the implementation uses fframes 1.2.0 at
the exact `b644f22cf5be2333e7098caf7ec064c3d36aeb02` source pin. GitHub's
[release list](https://github.com/dmtrKovalenko/fframes/releases) still marks
1.2.0 latest. The integration does not automatically follow upstream main.

Source-pin clarification: the release tag points to
`055bb6b9dcbbcca6532206847d43ea8e81fa2a0b` (2026-10-03). Our pinned
`b644f22` is a later upstream commit (2026-10-11), whose packages still declare
version 1.2.0. Local cached upstream Git confirms the release commit is its
ancestor. Thus “1.2.0” describes the package version; Fold uses post-release
source, not the exact release tag. The pinned commit includes the upstream
timeline segmentation fix (#208). Neither the package version nor the latest
release listing proves that this pin is the newest upstream main commit.

Completed default-workspace cutover test execution with local socket access and
serial shared-budget use: `cargo test --workspace --offline --no-fail-fast --
--test-threads=1` reported 581 passed, 1 failed and 71 ignored across 135 result
groups. The real MCP native proposal/Apply test and retired wire/scope rejection
test passed. The only failure is the unchanged Compositor native-authoring
inventory test: it asserts 24 operators while its unchanged registry contains
25. Both files match HEAD; this is reported as a separate baseline failure.
Ignored native/GPU/runtime gates are not covered by this result. The full log is
`/tmp/fold-motion-cutover-tests.log`. Workspace/all-target Clippy is now running
with output retained at `/tmp/fold-motion-cutover-clippy.log`.

Workspace Clippy stopped at an unchanged feature-dependent native-video loop in
`crates/render/src/gpu/preparation.rs:99` (`clippy::never_loop` with native-video
disabled). This file matches HEAD; the error is a separate baseline issue and
prevents claiming a clean workspace Clippy checkpoint. Existing lint warnings
are retained in `/tmp/fold-motion-cutover-clippy.log`. Native visual and pixel
acceptance remain open independently of these build/test results.

### 2026-10-12: native application MCP frame qualification

Extended the existing real MCP subprocess workflow with an opt-in GPU frame
check. The GPU test compiled successfully, then
`cargo test -p fold-app --features gpu --test agent_mcp --offline
mcp_native_motion_frames_match_candidate_and_committed_artwork -- --ignored
--exact --nocapture` passed (31.75 s execution) with local socket/Vulkan access.
Hardware provenance identifies NVIDIA GeForce GTX 1070, driver 580.173.02.

At exact time 1/2 s and 640×360, moving the native rectangle from x=0 to x=123
changed the expected interior pixels. Candidate pixels exactly matched the
committed frame after Apply; rendering the committed revision before Apply
still matched the original frame. Exact Apply retry returned the same result.
All three frame observations reported zero CPU adapter nodes and zero evaluation
readback bytes. Display extraction itself returns opaque RGBA8 through the
shared viewer transform; this is not HDR/alpha export qualification.

Raw public responses are retained at `/tmp/fold-motion-mcp-frames.json`; decoded
before/candidate/after PNGs use `/tmp/fold-motion-mcp-*.png`. Inspected the actual
after image. Persistent provenance, transfer counts and pixel hashes are in
`resources/motion-context/mcp-frame-evidence.json`, alongside `mcp-before.png`,
`mcp-candidate.png` and `mcp-after.png`. Updated the public skill with the verified
limits of this route.
Native UI acceptance, rich motion quality, preview/export parity and the other
remaining complete-slice requirements remain open.

### 2026-10-12: shared evaluated SVG tree for editable capture

Separated upstream SVG evaluation into `fold_motion_fframes::Conversion`, owning
the explicit font database and retained usvgr conversion cache. The renderer now
uses this same evaluator. Callers can obtain the evaluated upstream tree without
constructing a Vulkan device; there is no second SVG parser or rasterizer.
The isolated bridge `cargo check --manifest-path
crates/motion-fframes/Cargo.toml --offline` passed (1.79 s).

Inspected the pinned usvgr tree API: groups carry relative affine transforms,
group opacity, blend/isolation, clips, masks and filters; paths retain fill,
stroke and paint order; nodes also include text, images and fast shapes. Native
capture must preserve these semantics or reject unsupported conversion before
any document mutation. In particular, copying group opacity onto each child
would change overlapping artwork. The current native model still needs these
representational decisions before a complete conversion transaction can ship.
Script capture, its public command and artist control remain open. The focused
native MCP rerun passed (8.56 s execution), preserving the candidate/committed
pixel checks and zero evaluation readback/CPU adapters after extraction.

### 2026-10-12: native affine capture support

Added an editable, animatable Shear scalar to native transforms. Shared
`Transform::affine` evaluates translation, rotation, scale and the independent
off-diagonal coefficient; both canvas tools and fframes rendering use it.
`Transform::from_affine` decomposes upstream matrices without baking geometry,
including reflections, parallel columns and collapsed transforms. This avoids
changing stroke semantics during capture. Shear uses the existing discovered
scalar, inspector, protection and transaction paths; no extra toolbar was added.

The native document schema is now 2 because its required transform shape changed.
Schema 1 is unsupported and remains opaque through generic project storage;
there is no old transform reader or migration shim. Historical render evidence
retains its original schema and provenance.

Focused vector tests passed (3 tests), covering captured affine point mappings
and their use in the canvas hierarchy plus existing curve/group operations.
Fixed a test helper typo found by the first compile. The isolated fframes bridge
check passed (1.08 s), and Motion UI check passed (2.74 s). Normal/narrow native
visual review remains unverified. The existing native MCP GPU test now includes
a sheared rectangle and checks a pixel excluded specifically by shear; that
focused run passed (13.19 s execution) on the native Vulkan host, with candidate
isolation, exact candidate/committed pixel equality and zero evaluation readback
or CPU adapters. Inspected the decoded sheared shape. Persistent provenance and
pixel hashes are in `resources/motion-context/mcp-shear-frame-evidence.json`,
alongside `mcp-shear-before.png`, `mcp-shear-candidate.png` and
`mcp-shear-after.png`; the earlier unsheared evidence was preserved.
The expanded opaque-storage contract test also passed: unsupported schema 1
cannot execute and survives save/reopen unchanged. Full script conversion and
other paint/effect support remain open.

### 2026-10-12: evaluated-tree editable capture

Added an all-or-nothing upstream-tree converter in the isolated fframes bridge.
It creates fresh native object/contour/point identities, retains hierarchy and
group opacity, decomposes affine transforms, converts quadratic segments exactly
to cubic handles, and preserves solid fill/stroke, fill rule, cap and join.
Text captures its evaluated font outlines rather than claiming to reconstruct
editable text or animation. Ellipses and equal-radius rounded rectangles remain
native primitives. Other supported outline geometry becomes editable paths.
Conversion has explicit object, segment and depth bounds. Unsupported gradients,
patterns, images, clipping, masks, filters, blends/isolation, custom miter limits,+dashes and paint/rendering modes fail before any document mutation.

The bridge check passed (1.72 s). The first native release qualification exposed
316 differing pixels (maximum float-channel error 0.27115) when the ellipse was
captured as an outline. Corrected this by preserving native primitives, then
reran the same scene and added a rounded rectangle: all 640×360 RGBA32F pixels
matched exactly (11 captured objects, zero differing pixels, maximum error 0).
The scene includes overlapping children under group opacity, affine shear,
reflection/nonuniform scale, quadratic/cubic closure, even-odd fill, text
outlines, ellipse and rounded rectangle. Gradient conversion was rejected.

Command: `cargo run --manifest-path crates/motion-fframes/Cargo.toml --offline
--release --example capture -- /tmp/fold-editable-capture`. The initial release
example build took 1m50s because Cargo compiled its development dependencies;
subsequent focused builds took 3.11 s and 1.49 s. This was explicit native GPU
qualification readback, not an application fallback path or performance result.
Persistent device UUID, hashes and result are in
`resources/motion-context/editable-capture-evidence.json`, with the captured
canvas and original/editable PNGs. Inspected the actual editable PNG.
The worker capture request, guarded conversion transaction, public command and
artist controls are still open, as is the remaining paint/effect representation.

### 2026-10-12: guarded conversion transaction

Moved the static captured-tree payload into the native document package and
added one typed `Capture` edit. It replaces the script content with a group and
attaches the captured local output while retaining the authored object's ID,
hierarchy position, graph placement, transform, paint and extension data. Source
and unknown control data are retained as inert provenance on a new captured
child; there is no secondary script execution or legacy reader. Exact capture
time is recorded. Undo restores the original script through project history.

Conversion rejects stale expected objects, invalid time/hierarchy, overlapping
object IDs, embedded scripts, protected objects/controls and excess object,
point or depth counts. It adds the validated tree in bulk as one edit, rather
than thousands of Insert operations or multiple project commits. Thus the
provider's per-request edit count does not split the conversion transaction.
The existing baseline protection and global identity checks remain shared.

Focused editing tests passed (4 tests), including identity/provenance retention,
one undo/redo, stale-source rejection and protected-control rejection. The Motion
UI check passed (3.44 s); no new UI control was added or visually qualified.
After the final bounds/ownership cleanup, all 4 editing tests passed again and
the isolated fframes bridge check passed (1.16 s). Diff whitespace review passed.
The worker capture request and application orchestration remain open: this
transaction consumes captured local script output, not a whole-document render.
Do not claim artist or public automatic conversion acceptance yet.

### 2026-10-12: local script capture worker

The private worker demand now requires an explicit Render or Capture operation;
runtime callers and program preparation emit the new format without an alias or
default for old demands. Capture evaluates the selected script using its actual
canvas/object context and exact time, resolving inherited paint while excluding
the authored root transform/opacity. The conversion transaction retains those
root properties, so capture does not apply them twice. It returns a correlated,
bounded JSON native-tree payload or an error with no payload. Capture does not
construct/query a GPU device. Font retention is shared across operation types;
each evaluator rebuilds when the latest explicit font data changes.

`cargo run --manifest-path crates/motion-fframes/Cargo.toml --offline --release
--example capture_worker` passed (8.50 s build). This exercises the actual worker
subprocess with compiled fframes script logic: exact 1/2 s, local versus converted
world coordinates, inherited paint, changed runtime control, rejected out-of-range
time and a successful subsequent request, all without GPU access. It qualifies
the worker protocol, not the application's source compiler/capture orchestration.
The isolated bridge check passed (1.35 s), and Motion context all-target/GPU check
passed (8.87 s) before the final shared-font cleanup. Native MCP render regression
qualification passed against the final worker format (13.69 s execution):
candidate isolation, exact candidate/committed pixels and zero evaluation
readback/CPU adapters remain valid. The capture worker evidence is retained in
`resources/motion-context/capture-worker-evidence.json`. Diff review passed.
App-side capture transport, public orchestration and artist controls remain open.

### 2026-10-12: application runtime capture transport

Added `Runtime::capture` to the same source compiler/cache and supervised retained
worker used by the explicit reference runtime. Render and capture demands share
one bounded channel, cancellation/deadline loop, request IDs, font identities
and process retirement. Capture replies use strict correlated headers and
bounded JSON payloads with a retained transport allocation lease. Capture does
not invoke the reference pixel path or a GPU fallback; the native GPU renderer
remains separate from this metadata transport.

Motion all-target/GPU check passed (0.89 s). Ran actual Rust source compilation,
worker capture and the guarded native conversion via `cargo run -p
fold-motion-context --features gpu --offline --example capture_program --
/tmp/fold-capture-runtime /home/rgame/.cargo/bin/cargo
/home/rgame/.cargo/bin/rustc`. It passed for exact time 1/2 s and two runtime
control values, preserving local output plus the original object's transform
exactly once. No pixel readback was requested. Measured this run's cold
compile/capture at 4463.33 ms and retained control/capture at 37.53 ms; these are
metadata-capture observations, not frame-performance acceptance. Host example
build took 30.03 s. Evidence and resulting canvas are retained in
`resources/motion-context/capture-runtime-evidence.json` and
`resources/motion-context/runtime-converted-canvas.json`.

Public candidate orchestration, artist controls and app-integrated acceptance
remain open. The application must bind capture to its immutable document/time
intent and publish a guarded candidate before Apply; this runtime method itself
does not mutate project state.

The existing retained-runtime regression test also passed (2.36 s execution),
covering runtime control reuse, stale render replies and cancellation of blocked
pipes after the shared request/reply change. No new broad suite was run.

### 2026-10-12: immutable conversion preparation service

Added a configured Motion conversion service with explicit fonts and one retained
capture runtime. It accepts an immutable document/object/time intent, checks
object/ancestor and script-control protection before compiling, waits for its
runtime with cancellation/deadline checks, captures local script output and
returns a validated document through the shared atomic conversion edit. It does
not publish project state; the host still owns guarded candidate inspection and
Apply. Runtime/font configuration is explicit rather than a global environment
lookup in the document provider.

Updated the existing real-source qualification to exercise this service. The
run passed for two control values, exact time 1/2 s, unchanged input document,
retained document ID/revision/extensions and transform applied exactly once.
This run measured cold preparation at 4452.27 ms and retained preparation at
34.37 ms; these are metadata preparation observations, not frame-performance
acceptance. Evidence is in `resources/motion-context/capture-service-evidence.json`
and `service-converted-canvas.json`. Host example build took 5.03 s.

Native replacement now clones the original envelope and changes only validated
payload bytes; proposal publication uses that same method instead of duplicating
envelope reconstruction. The first extended test incorrectly assumed resource
declarations were supported: `Canvas::from_document` explicitly rejects them.
This is not evidence of lost valid resource data. Corrected the test to verify
rejection of unsupported declarations and preservation of supported document
extension data. Resource declaration support remains open. Motion all-target/GPU
check passed (2.86 s); all 4 corrected focused edit tests passed (0.05 s). Provider
registration of the service, the public conversion request and artist controls
remain open.

### 2026-10-12: registered capture proposals and real MCP Apply

Motion's document provider now owns configured proposal preparation. The
application assembles conversion and video providers from the same explicit
worker configuration and fonts. Motion's native request adds Capture with
document, expected inspection, script object and exact time. Unconfigured
document-only registries advertise the existing Author request. The standard
prepared-proposal implementation still owns candidate inspection, baseline
guards, reconciliation and publication; no parallel candidate store or Apply
route was added.

App all-target/GPU check passed (12.43 s). Extended the existing real MCP
subprocess workflow with source-authored script conversion. The first test used
an extra internal wrapper and was rejected (`missing field op`); corrected it to
the public native request body. The corrected native GPU run passed (14.72 s
execution). At 640×360 and exact 1/2 s, original script, immutable candidate and
committed-after-Apply display pixels matched exactly. Committed state stayed
unchanged before Apply; exact Apply replay was stable. The original object ID
became a group with editable children and no active script objects. All frame
observations reported zero CPU adapters and zero evaluation readback bytes.

Command: `cargo test -p fold-app --features gpu --test agent_mcp --offline
mcp_capture_prepares_editable_shapes_without_changing_pixels_or_committed_state
-- --ignored --exact`, with local socket/native Vulkan access. Raw frame responses
are in `/tmp/fold-mcp-capture-frames.json`; persistent provenance/hashes are in
`resources/motion-context/mcp-capture-evidence.json` and its decoded after PNG.
Updated the Fold public skill with the verified capture request and its limits.
Inspected the decoded after PNG. Motion context all-target/GPU check passed
after updating its document-only test fixtures to construct the provider explicitly.
Artist conversion controls, native visual review, unsupported paint/effect
representation and the other complete-slice requirements remain open.

### 2026-10-12: artist conversion action

Added Convert to editable to the script inspector, using the Fold UI skill and
existing command/pending-edit lifecycle. It submits exact current document time
to a worker command. The host handler uses the same configured scoped proposal,
expected native inspection and validated publication gate as MCP; generic host
revision checks own the final commit. The action retains one undo entry. It is
hidden when the host lacks the command and disabled for playback, gestures,
pending work, protected objects or unapplied source drafts. Cached source drafts
sync before this check, preserving conflicts rather than discarding artist text.

Added lightweight command availability to the shared desktop contract. Motion
owns the command registration; app assembly supplies its host orchestration
handler explicitly. The first run caught an invalid foreign command registration
under the app package, now corrected. No separate conversion executor or project
publication path was added.

App/Motion UI build check passed (11.47 s). An initial check mistakenly requested
a nonexistent app `ui` feature; corrected to the Motion dependency UI feature.
The real artist-command test exercises background preparation, unchanged input
before completion, converted native content and one Undo/Redo restoring the
script/shapes. The default sandbox-cache run did not commit; rerunning the same
compiled test with `FOLD_MOTION_CACHE=/tmp/fold-artist-conversion` passed (4.35 s).
Actual command: `FOLD_MOTION_CACHE=/tmp/fold-artist-conversion
target/debug/deps/agent_mcp-9bd75e936d8f0b42 --ignored --exact
artist_conversion_command_runs_in_background_and_restores_script_with_undo`.
The final Motion UI check passed (4.57 s) after draft synchronization cleanup.
Native visual and interactive acceptance at normal/narrow widths remain open;
command execution is not evidence that the button layout or artist flow is
visually accepted. All five replacement slices retain their remaining gates.

### 2026-10-12: conversion cancellation and request-specific errors

The shared desktop contract now exposes the failure for an exact extension
request (command, base revision and argument bytes). The session clears this
record on project replacement and captures failures at job completion. The
Motion inspector can show its own worker failure rather than a generic message
or another job's status. Cancel conversion uses the existing worker cancellation
route and keeps pending admission until the worker acknowledges cancellation.

Focused qualification passed: `FOLD_MOTION_CACHE=/tmp/fold-artist-conversion
cargo test -p fold-app --features gpu,fold-motion-context/ui --offline --test
agent_mcp artist_conversion_command_runs_in_background_and_restores_script_with_undo
-- --ignored --exact`. This compiled the affected UI and app (1m 50s) and the
single test passed (5.20s). It verifies background conversion, unchanged input,
Undo/Redo, stale-request rejection, exact error correlation and cancellation
without a project mutation. Existing render YUV dead-code warnings remain.
The earlier build observation handle was missing and no matching process was
live, so this was a fresh run rather than a duplicate live build. Diff whitespace
review passed. Native normal/narrow visual and interactive acceptance remains
open; this test does not establish layout quality or complete-slice acceptance.

### 2026-10-12: focused group graph navigation

Motion groups now open through the shared graph's existing open/back actions,
double-click and narrow-width menu. A focused group view includes the group and
its direct children; nested groups open separately. The document overview remains
available. Selection is filtered for the visible graph without changing the
shared artist selection. Graph view identity includes the group so the shared
editor retains separate navigation state. This is transient view state, not a
second document or graph model.

Creating an object in this view inserts it into the current group through the
same guarded Insert transaction. Changing documents or removing the focused
group clears the scope; Undo of a child preserves it. Existing wiring, order,
protection and graph-position edits retain their shared document semantics.

Focused UI/GPU unit checks passed: the existing graph/agent/guards/Undo test with
open/back assertions (58.60s build, 0.02s test), and a creation/Undo scope test
(15.16s incremental build, 0.01s test). No full suite was repeated. Diff whitespace
review passed. Native visual review, large-scene interaction measurement and
normal/narrow artist acceptance remain open. This change gives group inspection
a focused view; it does not establish complete graph scalability or full rewrite
acceptance.

### 2026-10-12: hierarchical graph overview

The main Motion graph now projects only document roots. Group contents appear
when the artist opens the group, using the same focused projection described
above. Group summaries show their direct object count; their Add object input
remains available for moving a visible sibling into the group. Hidden child
ports and wires are absent. Projection looks up visible objects and enumerates
membership ports only for the opened group, rather than scanning all objects
or all closed-group children. No separate hierarchy or stored graph was added.

Updated the existing graph/agent test to verify root and focused views across
reparenting, agent parameter edits, wiring, protection and Undo/Redo. All five
artist UI tests passed serially (15.84s build, 3.22s execution). The first parallel
run passed both graph tests but two canvas tests hit Dear ImGui's
ContextThreadConflict; serial execution resolved that shared-context contention.
After removing the closed-group membership scan, the focused graph test passed
again (11.08s build, 0.02s execution). Native visual acceptance and measured
large-scene performance remain open. This updates the earlier overview behavior:
the overview is hierarchical rather than an all-object view.

### 2026-10-12: selection reveals the corresponding graph scope

Single-object selection through the canvas/outliner now reveals the object's
owning group in the graph. A changed host selection uses the same behavior,
including selection received while changing documents. The opened group's own
membership anchor remains visible when selected. Empty or multiple selection
preserves the current view. Explicit graph open/back navigation is retained
until another surface changes selection; ordinary polling does not force the
artist back into the selected object's group.

This only changes transient view state and publishes the existing shared
Selection command. It does not modify artwork, document hierarchy or history.
Extended the focused group test with canvas/outliner selection, host selection,
anchor/empty selection and explicit navigation assertions. All five artist UI
tests passed serially (12.06s incremental build, 3.09s execution), with the same
existing render warnings. Diff whitespace review passed. Native interactive
acceptance remains open; these tests qualify state transitions, not visual focus
or layout quality.

### 2026-10-12: stroke quality and retained warm-frame measurement

Extended the existing native/script qualification scene with four open cubic
Bezier strokes (0.5, 1, 1.5 and 2.25 canvas pixels), moving by 0.25 pixels per
frame, alongside native Noto text and 10,000 animated script dots. Corrected an
initial example typo (`line_cap`/`LineCap` to the existing `cap`/`Cap`). The
generated scene contains six authored objects. Actual retained GPU execution
and pixel qualification passed on the NVIDIA GeForce GTX 1070, with a release
fframes worker and debug Fold host. Inspected the decoded frame: the curve
strokes and typography are visible. All three frames have finite linear sRGB,
premultiplied float pixels, transparent background and more than 4,200 partial
alpha stroke pixels. Subpixel stroke motion and the runtime count edit change
pixels as expected. This is rendered-artwork evidence, not native UI acceptance
or proof of all stroke styles, HDR or export parity.

Added an optional bounded warm-frame count to the existing retained-GPU example.
With 120 warm requests and the first 10 discarded, the 110 measured frames had
p50 75.716 ms, p95 90.203 ms and max 118.152 ms. Timing includes immutable
snapshot preparation, render, publication, release and host polling, excluding
qualification readback. One worker completed all 128 requests (including the
existing stale-publication fixture); runtime upload/readback remained zero and
host reservations were released. This speed is not accepted as the final motion
experience. Debug-host/validation-enabled results are not release-app throughput.

Two shorter 20-frame diagnostic runs investigated this result. Exposed the
existing bounded process diagnostic tail through GpuRuntime, and recorded host
phase timings only in the qualification example. In the final run, median
snapshot preparation was 0.509 ms, render 71.598 ms, publication/release 0.220 ms
and host poll 0.031 ms. Release-worker median tree conversion was 13.922 ms,
draw-command construction 6.254 ms and submission 0.861 ms. Render demand still
contains substantial time outside those worker phases; localize admission,
program/font identity, transport/import and completion before selecting a fix.
Do not infer that replacing the vector renderer would remove this gap.

Commands: `cargo run -p fold-motion-canvas --offline --example prepare_program
-- /tmp/fold-motion-quality-20261012`; `cargo build -p fold-motion-context
--features gpu --offline --example render_gpu_program`; native-GPU execution of
`target/debug/examples/render_gpu_program /tmp/fold-motion-quality-20261012
/home/rgame/.cargo/bin/cargo /home/rgame/.cargo/bin/rustc 120` with
`VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`, followed by
`python3 scripts/qualify_motion_context.py /tmp/fold-motion-quality-20261012
--readback /tmp/fold-motion-quality-20261012/retained-gpu-frames.bin
--worker-profile release`. Diagnostic runs used `FOLD_MOTION_PROFILE=1` and 20
frames. Updated example builds passed; Python compilation and diff whitespace
checks passed. Existing render warnings remain; no workspace suite was repeated.

Persistent evidence: [frame](../resources/motion-context/motion-quality-frame.png),
[120-frame measurement and pixel checks](../resources/motion-context/motion-quality-evidence.json),
[host/worker phase samples](../resources/motion-context/motion-quality-profile.json),
and [bounded worker diagnostics](../resources/motion-context/motion-quality-diagnostics.txt).
Raw float output and generated source are in `/tmp/fold-motion-quality-20261012`;
the final diagnostic run replaced the temporary output, while the original
120-frame measurement is preserved in the persistent evidence and temporary
`baseline-retained-gpu-evidence.json`. Performance acceptance remains open.

### 2026-10-12: remove repeated font hashing from warm requests

Used the diagnosing-bugs loop against the actual retained GPU example. The
existing 75.716ms median failed an interim 33.333ms feedback threshold (not a
replacement for final performance acceptance). Removing text and native paths
left a 74.417ms median; removing unused fonts from the remaining 10,000-dot
script reduced it to 35.924ms. Ranked repeated font identity work, GPU
transfer/completion and SVG conversion as causes. A single targeted probe
confirmed repeated font hashing consumed 34.877ms per warm request in this
debug host.

Runtime now owns one content-checked font identity cache, shared by reference,
capture and GPU transport through the existing compiler/runtime. Equal bytes
reuse the digest; changed bytes, order or font boundaries invalidate it. The
cache owns its bytes rather than trusting caller pointers, reserves payload and
vector storage through the shared working-memory budget, and bounds admission
to the protocol request allowance. Worker startup still receives fonts even
when the digest is cached. Temporary timing-probe code was removed.

The original six-object scene was rerun for 120 warm requests with native GPU
validation and profiling enabled. Of 110 measured frames, p50 was 41.860ms,
p95 48.427ms and max 57.553ms. Warm font checks fell to 0.071ms. All three
reference RGBA32F transport frames were byte-for-byte identical to the saved
pre-fix run, and the pixel qualifier passed again. Admission, stale-publication,
zero runtime pixel upload/readback, one-worker reuse and release checks passed.
The original baseline had profiling disabled; the after run had profiling
enabled. These remain debug-host measurements, not release-app throughput or
final acceptance. The remaining frame cost is still too high to claim the
requested fast motion experience.

Two focused runtime tests passed (5.06s build, 2.10s execution): content/boundary
and in-place font invalidation with equal-data cache reuse, plus retained
control/stale-frame/cancellation behavior. No new timing-sensitive unit test or
full suite was added. Real native feedback checks require warm font identity
below 1ms, total median improvement of at least 20ms and identical frame bytes;
all passed. Example builds and diff whitespace review passed; existing render
warnings remain. The shortened reproductions and raw timing probes remain in
`/tmp/fold-motion-latency-minimal-20261012` and `/tmp/fold-motion-font-*-20261012.jsonl`.
Persistent [before/after evidence](../resources/motion-context/motion-font-cache-evidence.json)
and [worker diagnostics](../resources/motion-context/motion-font-cache-diagnostics.txt)
record the remaining limits. All complete-slice gates remain open.

### 2026-10-12: localize remaining latency and qualify optimized host timing

The post-font-cache scene still failed the interim 33.333ms feedback threshold.
Ranked import/copy completion, per-request setup and upstream conversion as
remaining causes. A temporary boundary probe in the same native GPU scene
measured warm medians: admission 4.340ms, program preparation 0.285ms, font
identity 0.062ms, serialization 1.067ms and worker wait 33.174ms. Within the IO
worker: producer/header+FD wait 26.994ms, native import 0.623ms, copy/validation
submission 2.810ms, completion 1.420ms and release acknowledgment 0.952ms.
These distinguish the producer's work from consumer transfer; they do not prove
that all producer wait is CPU conversion. The earlier worker phase samples
identify tree conversion as its largest measured CPU phase. Saved
[phase samples](../resources/motion-context/motion-ingress-profile.json), then
removed all temporary probe code. Diff whitespace review passed.

Built the affected retained-GPU example once with optimized host dependencies:
`cargo build -p fold-motion-context --features gpu --offline --release --example
render_gpu_program` passed (1m24s). No workspace tests or UI build were repeated.
The same 120-frame scene on the GTX 1070 measured p50 38.068ms, p95 44.524ms,
max 51.996ms with the Vulkan diagnostic layer enabled. An otherwise identical
run without `VK_INSTANCE_LAYERS` measured p50 37.495ms, p95 43.311ms and max
45.576ms. Both used optimized Fold host and release fframes worker, discarded
10 warmup samples, retained Fold's GPU status/pixel validation and passed the
existing GPU ownership, stale-publication and oversized-admission checks.
All three reference float frames remained byte-for-byte equal to the saved
pre-font-cache run. This rules out debug-host code and the external diagnostic
layer as explanations for most of the remaining latency.

Commands used `target/release/examples/render_gpu_program
/tmp/fold-motion-quality-20261012 /home/rgame/.cargo/bin/cargo
/home/rgame/.cargo/bin/rustc 120`, first with
`VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`, then with that variable removed.
Persistent [optimized validated measurement](../resources/motion-context/motion-release-validated-evidence.json)
and [optimized normal-setting measurement](../resources/motion-context/motion-release-evidence.json)
include host phase samples. These are sequential retained-runtime throughput,
not full app FPS, UI responsiveness or preview/export parity. Performance
acceptance remains open; the next optimization must address measured producer
conversion/drawing or admission work while preserving pixels and ownership.

### 2026-10-12: adopt upstream retained text-outline cache

Inspection of the pinned upstream implementation found a missed configuration:
`usvgr::Cache::default()` has no text-outline cache. Upstream fframes initializes
its converter using `Cache::new_with_text_cache` with the Skia pipeline's bounded
text capacity. Fold's shared Conversion now uses that same upstream default
(10 layouts), retaining its explicit font database and common render/capture
tree path. No independent text shaping or alternate rasterizer was introduced.
Font changes still replace the conversion instance through the existing worker
font-data comparison, so outlines are not shared across different font databases.

Extended the existing native capture qualification rather than adding a separate
test harness. On the GTX 1070, cached repeated typography was byte-identical;
changing text changed pixels and restoring it restored exact pixels. Original
and editable captured artwork still had zero differing RGBA32F pixels and zero
maximum float error, with 11 captured objects. The final small mixed fixture
measured 0.232ms cold conversion and 0.015ms median over 20 warm conversions.
These timings are for that fixture, not the 10,000-dot scene.

The original 120-request stress scene with optimized host/worker and no external
Vulkan diagnostic layer measured p50 35.656ms, p95 43.819ms and max 48.241ms.
Its three reference frames remained byte-for-byte identical to the saved
pre-font-cache output. All existing ownership/stale/admission checks passed.
The earlier comparable median was 37.495ms, but its p95 was lower; these runs do
not establish a broad throughput improvement or final performance acceptance.
The dot scene remains dominated by other conversion/drawing and admission work.

Bridge check passed (0.97s). Release capture-example builds passed (6.24s, then
1.54s after adding text-edit assertions); native qualification executions passed.
No host rebuild was needed for the scene: the existing optimized runtime
fingerprinted and compiled the changed native bridge through its normal source
cache. Commands used `cargo build --manifest-path crates/motion-fframes/Cargo.toml
--offline --release --example capture`, then
`crates/motion-fframes/target/release/examples/capture
/tmp/fold-text-cache-capture-20261012`, and the existing optimized retained-GPU
command with 120 frames. Persistent [text/editable capture checks](../resources/motion-context/text-cache-capture-evidence.json)
and [stress-scene measurement](../resources/motion-context/motion-text-cache-evidence.json)
record the limits. Existing render warnings remain. Diff whitespace review
passed; native artist review, broader typography cases and final performance
acceptance remain open.

### 2026-10-12: real mixed-context MCP workflow after save/reopen

Extended the existing real MCP subprocess qualification with a complete linked
project: a sheared fframes script rectangle, a native 3D box, a Compositor Over
graph reading both published video outputs, and a Timeline document clip reading
the Compositor output. The native source documents remain independent; shared
project storage retains their explicit dependency references. Saved and reopened
the four-document project before launching the session; complete document state
matched exactly.

Through discovered public MCP tools at 640×360 and exact 1/2 s: inspected Motion,
prepared an immutable native parameter edit, inspected its candidate, observed
candidate Motion pixels and the unchanged committed Timeline, then applied and
replayed the receipt. Candidate Motion matched committed Motion after Apply.
The committed Timeline matched Compositor and differed from each isolated
Motion/3D source, proving that both contexts contributed visible pixels. All six
observations reported zero CPU adapter nodes and zero evaluation readback bytes.
Decoded and inspected the mixed frame. These are opaque display pixels, not
linear/HDR/alpha export or native artist acceptance.

The first fixture used a head-on box and passed (12.12s). Rotated it with the
native spatial rotation binding to make its 3D faces visible; the final check
passed (10.60s). The spatial-enabled app/CLI/test build passed (2m51s), followed
by a focused incremental build for that fixture change (1m12s). Existing render
warnings remain. No full suite was repeated. Command:
`FOLD_MOTION_CACHE=/tmp/fold-mixed-context-cache
FOLD_MOTION_MCP_EVIDENCE=/tmp/fold-mixed-context-frames-20261012.json
target/debug/deps/agent_mcp-64d2103d5387a478 --ignored --exact
mcp_motion_and_3d_outputs_survive_save_reopen_compositing_and_timeline`, with
native GPU/local socket access, built by `cargo test -p fold-app --features
spatial --offline --test agent_mcp
mcp_motion_and_3d_outputs_survive_save_reopen_compositing_and_timeline --no-run`.

Updated the Fold skill's public frame-review guidance with this verified linked
workflow. A scoped Motion candidate currently observes Motion itself; downstream
candidate Timeline/Compositor observation remains a missing public capability.
Committed downstream outputs can be observed before/after Apply, as qualified
here. Persistent [evidence](../resources/motion-context/mixed-context-evidence.json),
[mixed frame](../resources/motion-context/mixed-context-after.png),
[Motion source](../resources/motion-context/mixed-context-motion.png) and
[3D source](../resources/motion-context/mixed-context-spatial.png) preserve the
checks; raw public responses are in `/tmp/fold-mixed-context-frames-20261012.json`.
Diff whitespace review passed. This qualifies linked save/reopen and committed
preview behavior; export parity, candidate downstream review, native artist
acceptance and the remaining complete-slice gates stay open.

### 2026-10-12: downstream immutable candidate review

Candidate observation targets now require both the candidate handle and the
document to view. Frame observation selects that document from the complete
immutable candidate snapshot, so a scoped Motion edit can preview its downstream
Compositor/Timeline output before Apply. The output-sampling request uses the
same explicit target shape and document resolution. Missing candidate documents
fail explicitly. Proposal scope, guards, prepared snapshots, receipt checks and
publication remain unchanged; viewing a document does not grant write scope.
The previous handle-only target is rejected rather than retained as an alias or
implicit fallback. Current discovery supplies the updated version-2 schema.

Extended the existing mixed-context real MCP check: observed both edited Motion
and downstream Timeline from the same candidate; candidate Timeline matched
committed Timeline after Apply and differed from committed-before pixels.
Committed Timeline remained unchanged during preparation. Save/reopen, exact
Apply replay, isolated-source checks and zero CPU adapter/evaluation readback
checks still passed. Native execution passed (10.96s) after a focused app/CLI/test
build (1m16s). The required-document request-shape test passed (38.75s test build,
0.00s execution); it rejects the previous handle-only form and accepts explicit
selection. Existing render warnings remain. No workspace suite was repeated.

Updated the Fold public skill with the verified candidate Timeline request.
Commands reused the spatial mixed-context test, writing raw responses to
`/tmp/fold-mixed-candidate-frames-20261012.json`; the shape check used `cargo test
-p fold-app --features spatial --offline --lib
candidate_observation_requires_an_explicit_document`. Persistent
[candidate evidence](../resources/motion-context/mixed-candidate-evidence.json)
and [candidate Timeline frame](../resources/motion-context/mixed-candidate-timeline.png)
record the result. Decoded and inspected the frame. Diff whitespace review
passed. This closes the prior downstream candidate-frame capability gap;
numerical output sampling with linked native contexts is not qualified by this
pixel test. Export parity, native artist acceptance and other complete-slice
requirements remain open.

## 2026-10-12: mixed-context Float EXR parity and Timeline alpha

Added one native integration check using the existing mixed-document fixture:
an animated, sheared half-opacity fframes rectangle over the rotated 3D cube,
through Compositor Over and a Timeline clip. Evaluated frames 15 and 16 at exact
30/1 time, exported both through the actual application `export_frames` API as
premultiplied Float RGBA EXR (Zip1, Linear Rec.709), reopened them through the
media decoder, and compared every component with linear GPU preview pixels.
The animated frames differ; both decoded exports match their preview exactly.
Preview evaluation reports zero CPU adapter nodes and zero pixel readback bytes
before explicit review readback. Delivery legitimately reads pixels for EXR
writing; this does not introduce a Motion renderer fallback.

The first check found an actual alpha defect: Timeline started with an opaque
black solid, so every output pixel had alpha 1 even when linked sources were
transparent. Changed its empty background to transparent. Empty regions now
have zero coverage; an opaque matte must be authored as source content. This
changes Timeline gap/background alpha intentionally, without adding a setting,
alternate renderer or document migration. Strengthened the existing half-open
source-range check to require a transparent gap.

Final native export check passed (23.92s); frames contain 204,511/205,095 fully
transparent pixels and 13,393/12,810 half-opacity pixels respectively, with alpha
range 0..1 and maximum preview/export component error 0. The existing real MCP
mixed-context save/reopen, candidate review and Apply workflow still passed
(10.38s). The focused Timeline source-range/gap test passed (34.29s build, 0.00s
execution). Focused app test builds were repeated while correcting the check
and fixing the discovered defect; no workspace suite was repeated. Existing
render YUV warnings remain. Diff whitespace review passed.

Run the ignored `mixed_motion_and_3d_float_exr_matches_linear_preview` test from
`cargo test -p fold-app --features spatial --offline --test agent_mcp`, with a
writable `FOLD_MOTION_CACHE` and native Vulkan access. Optional
`FOLD_MOTION_EXPORT_EVIDENCE` writes the result; the checked-in
[export evidence](../resources/motion-context/mixed-export-evidence.json) came
from `/tmp/fold-mixed-export-evidence-20261012.json`. EXR files are temporary
qualification outputs, not saved project assets.

This qualifies linear Float EXR parity and preserved transparency for this mixed
fixture. Solid-paint opacity passes through upstream Skia's 8-bit paint-alpha
conversion, so the half-opacity presence check allows 0.01 tolerance; exported
pixels still require exact equality with preview. Signed/HDR source values,
configured color transforms, other codecs/precisions, background-dependent
effects, native UI appearance and artist acceptance remain unqualified. All five
slices remain open; this evidence does not complete the export or quality gates.

## 2026-10-12: retire stale CI dependency and proposal policies

The deletion audit found two active CI checks still describing the removed
`fold-motion` package. The dependency check failed against the actual workspace:
new Motion/3D dependencies and GPU sharing were absent from its policy. The
proposal check passed misleadingly because it scanned the deleted Motion
directory and did not inspect the new Motion or 3D proposal modules.

Updated package identities to Motion Canvas, Motion Context, independent 3D and
neutral GPU sharing. Motion Context may depend on its Canvas model; Canvas stays
limited to project, animation and foundation services. Motion and 3D cannot
import one another, and infrastructure cannot import either feature's models,
even transitively. The transitive check now uses each package's explicit allowed
dependencies instead of a blanket exception for Composition. Optional shared UI
SDK and headless UI isolation checks remain active.

The source ownership check now scans current Motion Context, 3D, Compositor,
Composition and Timeline proposal modules and rejects current feature imports
from generic agent dispatch. Replaced old-language test fixtures with current
native model names; extended existing dependency tests for disabled/aliased
cross-context imports and external bridges hiding feature dependencies.

Both actual Cargo-metadata boundary checks (all features and headless) pass.
Actual proposal-source scanning passes. Nine boundary tests and four ownership
tests pass in milliseconds. Existing CI already invokes these scripts and their
tests, so no additional CI job or Rust build was needed. Diff whitespace review
passed. These are dependency and lexical ownership checks, not proof against
macro-generated coupling or full rewrite/deletion acceptance. Other retirement,
packaging, renderer, performance and native artist gates remain open.

## 2026-10-12: qualify straight-alpha mixed-context EXR delivery

Extended the existing mixed-context export check, without another fixture or
renderer, to write both premultiplied and straight Float RGBA EXR sequences.
Decoded both animated frames in both modes. Premultiplied components match the
linear preview exactly. Straight RGB matches preview RGB divided by coverage
exactly for nonzero alpha; alpha itself is unchanged. Every fully transparent
straight pixel has zero RGB, and all decoded values are finite. This exercises
the application's actual delivery setting and its transparent-pixel handling.

The focused native test passed in 14.05s after one affected app-test build
(1m16s). Existing render YUV warnings remain. Updated the existing
[export evidence](../resources/motion-context/mixed-export-evidence.json) from
`/tmp/fold-mixed-export-alpha-evidence-20261012.json`; it records both alpha modes
and per-frame straight-RGB error 0. No runtime fallback, compatibility path or
new export implementation was added. Diff whitespace review passed.

Added the current-status table above to distinguish today's implemented seams
from historical checkpoint gaps. Straight/premultiplied Float EXR is now
qualified for this mixed fixture; signed/HDR source color, configured color
conversion, other formats and artist acceptance remain open. All five slices
remain open.

## 2026-10-12: basic native 3D artist panels

Added independent 3D hierarchy and inspector panels through the shared UI SDK.
Registered their descriptors with the 3D package and enabled/registered their
optional UI feature in the desktop application. Each editor instance owns only
selection/gesture state and a hierarchy projection; the Scene document remains
authoritative. The list retains native group order, caches rows by document and
revision, and uses ImGui clipping for off-screen rows. Neither panel clones the
whole Scene on every draw.

Artists can select scene objects, change visibility, edit Position (meters),
Rotation (XYZ degrees) and Scale, and choose an active camera. Shared numeric
controls and edit responses feed the existing `fold.3d.edit` command, guarded
native edits, immutable preview and project undo. Escape, playback, selection,
time and revision changes cancel an active gesture. Pending commands disable
new edits and report their own errors. Object/ancestor and parameter protection
remain enforced by the UI and native edit validation.

Transforms retain native Literal/Track bindings. Auto-key and property-label
context menus add/update keys at exact rational time; existing key and track IDs
and extension data survive updates. Existing replacement tracks update or add
the current key. Removing a key cannot empty the track; removing animation keeps
the sampled pose. Add/Multiply tracks are shown read-only here with an explicit
route to Animation, rather than silently rewriting their combination semantics.
XYZ angles are a display/edit projection; quaternion storage stays authoritative.
Added the inverse XYZ conversion alongside the existing explicit-order Euler
constructor, with round-trip checks including both gimbal-lock signs.

`cargo check -p fold-app --features desktop --offline` passed (17.00s, then 1.75s
after final UI refinements). Two focused UI/backend seam tests passed (0.02s
execution, final test build 7.30s): real package staging and project edit sessions
prove preview isolation, cancellation, one-step undo, playback cancellation and
unknown object data retention; transform checks prove rotation equivalence,
exact key times and key/track extension retention. The same panel instances were
constructed/drawn in ImGui at 640px and 260px widths without mutating artwork.
This is construction coverage, not visual or artist acceptance.

Focused 3D Clippy passed with nine pre-existing warnings in editing/evaluation;
the new UI warnings were corrected. Existing render YUV warnings remain. Current
Cargo-metadata package/headless checks, proposal ownership scanning and diff
whitespace review pass. No workspace suite was repeated.

This provides the first basic 3D artist controls. Object creation/deletion and
hierarchy operations, geometry/material/light/camera-content controls, viewport
picking/gizmos, graph workflows, normal/narrow native visual review and artist
acceptance remain open. No native screenshot was available. All five slices
remain open.

## 2026-10-12: shared 3D geometry and material controls

Extended the same 3D inspector with native box Width/Height/Depth and sphere
Radius controls in meters, plus the shared color picker, Roughness, Reflectance
and Unlit material controls. Geometry/material values remain shared resources:
the inspector shows the instance count when a resource has multiple users, and
disables edits when any protected instance uses it. Permission checks call the
same native resource guard as agent edits. Imported meshes show vertex/triangle
counts; this step does not add a mesh editor or clone their vertex buffers just
to draw the inspector.

Geometry, material and object edits use one existing guarded preview/commit
transaction. Draft resource differences remain in the gesture even if their
inspector section is closed; ending a drag commits one undo entry. Resource IDs,
extension records and the native document schema are retained. This is resource
editing, not a new material model, renderer or agent protocol.

Corrected the shared authored-color control: its numeric RGB fields now permit
HDR values in linear Rec.709 as well as ACES. Alpha retains its 0..1 limit; the
SDR picker presentation stays bounded and unchanged draws retain stored values.
Owning document validation still controls legal color values. The existing SDK
test now draws both color-space cases with separate stable widget scopes.

Focused 3D UI/backend checks passed (14.00s build, 0.03s execution). Extended the
existing gesture test to stage geometry and an HDR/half-alpha material together,
prove committed-state isolation, commit both, restore them with one undo, and
reject edits to resources shared with a protected object. The normal/narrow
ImGui construction check still passes with the added sections. The shared color
draw test passed (0.00s execution; final focused build 17.95s), preserving signed,
HDR and precise color values on draw. Actual interactive picker/drag pixels are
not qualified by these checks.

Affected 3D/UI check passed (2.83s); final desktop app check passed (4.41s).
Package/headless boundaries, proposal ownership and diff whitespace review pass.
Existing render YUV warnings remain; no full workspace suite was repeated.
Native visual/artist acceptance, object and hierarchy authoring, camera/light
content controls, viewport interaction and graph workflows remain open. These
checks do not prove native HDR rendering or delivery. All five slices stay open.

## 2026-10-12: native camera and light inspector controls

Added Camera projection, perspective field of view in degrees, orthographic view
height in meters, Exposure in EV, and expandable local look/up and clipping
controls. Projection changes use explicit native defaults (45-degree perspective
or 2-meter orthographic view height) and remain undoable. Light controls expose
RGB color, Illuminance in lux and expandable local direction. These are native
content edits in the existing object transaction; no renderer or protocol model
was added. Camera/light content fields remain static in the current schema;
transform keyframes do not imply animated lens, exposure or illuminance support.

Added RGB-only authoring to the shared color control through the same working
color conversion and numeric editor used by RGBA. RGB pickers and their context
menus omit alpha, so a light does not present a control it cannot store. Signed,
HDR and precise stored RGB values remain unchanged on draw; owning documents
still enforce their native color bounds.

The existing 3D gesture check now previews camera projection/exposure and light
illuminance/color in one transaction, verifies committed-state isolation and
one-step undo, and rejects a degenerate camera look target before publication.
It draws all three native object types (instance, camera, light) at both 640px
and 260px inspector widths without changing the project. Both focused 3D checks
passed (14.61s build, 0.04s execution). The shared color draw test also exercises
the RGB-only control and passed (20.70s build, 0.00s execution).

Affected 3D/UI check passed (1.07s); final desktop check passed (4.02s).
Package/headless boundaries, proposal ownership and diff whitespace review pass.
Existing render YUV warnings remain; no full workspace suite was repeated.
These are transaction and ImGui construction checks. Native visual/interactive
artist review, content animation, object/hierarchy creation, viewport gizmos and
graph workflows remain open. No native screenshot was available, and all five
slices remain open.

## 2026-10-12: active-camera selection preserves the current draft

Interaction review found that the new Use camera button submitted its own
preview/commit request before the inspector finished the current object gesture.
That separate request could replace a pending transform draft. Removed the
separate publication path: the button now contributes an ActiveCamera edit to
the same guarded object/resource transaction and finishes that gesture once.
Its expected camera comes from the committed baseline; the current object value
comes from the draft. Escape and validation still apply to the complete edit.

Extended the existing 3D seam test with a second native camera and an actual
ImGui mouse press/release on Use camera while its transform draft is active.
The widget commits both the moved camera and active-camera selection, and one
undo restores the original documents. Existing preview, resource protection,
content edits, rotation/key and normal/narrow construction checks still pass.
The two focused tests passed (6.38s build, 0.06s execution). The affected desktop
check and diff whitespace review pass. No full suite was repeated.

This qualifies this widget's transaction routing in the ImGui fixture. It does
not establish native application visual review, complete artist acceptance or
viewport behavior. Object/hierarchy creation and the other listed gates remain
open; all five slices remain open.

## 2026-10-12: native 3D creation through one Add menu

The 3D editor now offers Box, Sphere, Group, Camera and Light through the shared
UI SDK Add menu. A selected group receives the new child; otherwise creation
uses the scene root. Names are unique. Instances receive independent geometry
and material resources. New scenes and artist creation share the same native
object and resource defaults, with no separate model or protocol path.

Creation uses guarded native edits and one project transaction. Protected
parents reject insertion. The new object becomes selected only after successful
publication is observed, and only if the artist has kept the same selection.
Pending edits and playback disable creation.

The existing focused 3D fixture now verifies all five creation recipes, unique
names, independent resources, group insertion, protected-parent rejection,
publication selection and preservation of a newer artist selection. One undo
restores each original scene, including resources. Both focused UI tests pass
(0.22s build, 0.14s execution). The desktop check passes. Existing render YUV
warnings remain; no full workspace suite was repeated.

These checks exercise native project transactions and ImGui construction. The
Add menu itself has not received native desktop click or visual acceptance.
Hierarchy reordering, viewport picking/gizmos, graph workflows and the other
listed gates remain open. All five slices remain open.

## 2026-10-12: 3D object names are artist-editable

Replaced the read-only inspector heading with a Name field. The local text
buffer commits through the existing guarded object transaction when editing
ends; it does not publish each keystroke. Escape restores the current name.
The buffer follows document/object selection and accepts external updates when
clean. If an agent changes the name while the artist has a different draft,
the inspector retains that draft and reports the conflict instead of replacing
the agent's change. Protected objects and pending edits retain their existing
control guards. Native name validation remains authoritative.

Extended the existing project seam check: rename preview leaves committed data
unchanged, publication changes the name, one undo restores the scene, and an
empty name is rejected without a project mutation. Both focused UI tests pass
(5.59s build, 0.13s execution), including normal/narrow construction and the
existing camera button interaction. Desktop check passes. Native keyboard
interaction and visual acceptance of the Name field remain unverified. No full
suite was repeated; all five slices remain open.

## 2026-10-12: native 3D hierarchy deletion

Added Delete object to the hierarchy row context menu. The action submits the
existing native Delete edit, removes the complete subtree, and removes only
geometry/material resources used by that subtree and no surviving instances.
It leaves unrelated resource-library entries intact. The scene's active camera
cannot be deleted; native scene validation also rejects a group deletion that
would remove it. Descendant/parameter protection is checked by the native edit
service before any preview or publication.

One transaction owns the deletion and resource changes. Selection removes
missing objects only after successful publication, without taking over a newer
artist selection. Playback and unfinished edits disable the action.

The existing focused fixture now verifies group/subtree deletion, orphan
resource removal, selection cleanup, exact restoration with one undo,
protected-parent rejection and active-camera rejection. Both focused UI tests
pass (6.50s build, 0.15s execution). Affected crate and desktop checks pass.
The context-menu click and native visual review remain unverified. No full
workspace suite was repeated. All five slices remain open.

## 2026-10-12: 3D multi-selection and native grouping

The 3D hierarchy now uses the same Control-click selection pattern as Motion.
Group selection is available in the selected row's context menu and through
Control-G while the hierarchy has focus and text input is inactive. Grouping
requires siblings, preserves their hierarchy order, and inserts a native
identity group at the earliest selected position. Objects retain their IDs,
transforms, animation, protection and extension data. The recipe generates
existing Insert/Move edits; there is no separate hierarchy model or protocol.

The group becomes selected after successful publication. Pending work,
playback and protected members retain the native guards. Mixed-parent selections
receive an explicit error rather than an implicit reparent/transform conversion.

Extended the existing native project fixture with reversed, noncontiguous
selection: it verifies ordered group membership, unchanged member objects and
resources, mixed-parent rejection, selection after publication and exact undo.
Both focused UI tests pass (9.16s build, 0.17s execution). Affected crate and
desktop checks pass; diff whitespace review passes. No full workspace suite was
repeated. Native Control-click/Control-G input and visual artist acceptance are
still unverified. Arbitrary reparenting, viewport tools and all five slice gates
remain open.

## 2026-10-12: actual ImGui hierarchy selection and grouping input

Extended the existing UI fixture to drive the real 3D Editor at 640px and
260px widths. It Control-clicks the last hierarchy row, verifies multi-selection,
presses Control-G, observes selection of the published native group and checks
its ordered members. One undo restores the exact original project documents.
This exercises the controls rather than calling the grouping recipe directly.

The first fixture attempt did not deliver the click. Explicitly focused its
window and allowed the pointer/modifier events to settle before mouse-down;
the real control then passed at both widths. No application workaround or
alternate input path was added. Both focused UI tests pass (12.35s build,
0.18s execution). Diff whitespace review passes. Production code was unchanged,
so the desktop build was not repeated.

This advances automated keyboard/pointer interaction evidence. It does not
qualify an installed native desktop session, screenshots, visual quality or
complete artist acceptance. All five slices remain open.

## 2026-10-12: one shared artist text-draft service

Removed the feature-local text-draft implementation from Motion and the duplicate
3D name synchronization logic. The UI SDK now owns TextDraft; Motion object
names, fframes source editing and 3D names use it. Clean buffers follow agent
updates and undo. Dirty buffers retain artist text when committed data changes.
Conflict detection, explicit reload/reset and acknowledgement after publication
share one implementation. Motion name input also uses the same explicit Escape
reset path as 3D. Document transactions and source Apply remain feature-owned.

A focused shared-service check verifies external updates, retained dirty text,
conflict, reload, publication acknowledgement and undo synchronization; it
passes (11.82s build, negligible execution). All five existing Motion UI checks
and both 3D UI checks pass (35.82s shared build, 3.12s and 0.18s execution).
Desktop check passed after the service extraction (2.87s). Final changed
Escape handling compiled and passed the focused checks. Diff whitespace review
passes. No full workspace suite was repeated.

Direct native text-input conflict/Escape interaction and visual artist acceptance
remain unverified. All five slices remain open.

## 2026-10-12: editable custom miter limits through fframes

Native Paint now stores an explicit miter_limit. The inspector exposes Miter
limit only for miter joins, using the shared numeric gesture control with a
reset to four. Native validation rejects nonfinite values and limits below one.
Rendering supplies stroke-miterlimit to the upstream SVG tree. Editable capture
preserves upstream stroke limits and no longer rejects custom miter limits.
Dashed strokes and clipped-miter joins still reject capture explicitly.

The required native paint field advances Motion document schema to 3. There is
no schema-2 reader, implicit field default or migration shim; older payloads
remain unsupported opaque project documents. New documents, artist and agent
edits use the same current model and guards.

Extended the existing native capture fixture with miter limits one and ten.
Both survive capture. Original script artwork and editable native artwork have
zero differing float pixels and zero maximum component error at 640 x 360.
Inspected the exported image: the high-limit corner stays pointed and the
low-limit corner clips. This run used a Mesa Vulkan device, not GTX1070 hardware
performance evidence. See [evidence](../resources/motion-context/miter-capture-evidence.json),
[captured document](../resources/motion-context/miter-capture-canvas.json) and
[rendered image](../resources/motion-context/miter-capture.png).

The save/reopen and artist/agent point-edit seam preserves a custom limit of
six and rejects an invalid limit (2.90s build, 0.24s execution). The graph/agent
UI seam passes (12.11s build, 0.02s execution). Native worker release build and
desktop check pass (6.16s and 1.96s). Diff whitespace review passes. No full
workspace suite was repeated. Native inspector interaction, exact stroke-join
picking boundaries, gradients/images and complete artist acceptance remain
open; all five slices remain open.

## 2026-10-12: vector picking includes authored miter tips

The path picker previously tested centerline distance and stroke half-width,
which missed the visible extension of sharp miter corners. It now also tests
the native miter wedge in object coordinates, using endpoint tangents and the
authored miter limit. Endpoint tangents include Bézier handle collapse cases.
Closed contours include the join at their first point. The existing inverse
transform applies the same test through reflection, shear and nonuniform scale.
This is interaction geometry only; artwork still renders through fframes/Skia.

A focused check verifies a point inside the long visible tip, a point beyond it,
low-limit clipping, nonuniform reflected transforms, authored endpoint handles,
closed-path joins, collinear segments and zero width. All three picking checks
pass (7.53s build, negligible execution), including the existing curve/fill and
insertion-parameter checks. Affected crate check and desktop check pass. Diff
whitespace review passes. No full workspace suite was repeated.

The existing five-screen-pixel pointer target remains deliberate. This change
qualifies tip inclusion; it does not claim exact butt/square caps, round/bevel
join boundaries, singular-transform picking or native artist acceptance. Those
limits and all five slices remain open.

## 2026-10-12: remove unused text shaping and vector-to-spatial bridges

The retirement audit found an inactive second text pipeline in fold-render:
Parley layout, font loading, Skrifa outlines, planar glyph meshes and text
extrusion. No current feature/application source called it. Its callers were
its own tests and the superseded planar/extrusion qualification fixtures.
Removed that pipeline, the unused planar/extrusion modules, their exports and
those obsolete tests. Removed Parley, Skrifa and i_overlay dependencies and
regenerated both application and isolated native-worker lockfiles. There is no
replacement wrapper or dormant compatibility surface.

Retained the independently valid camera-plane mapping service and its spatial
checks. The GPU camera-plane fixture now supplies an explicit rectangular Mesh
instead of using the removed vector tessellation bridge. Its independent
projection, coverage and depth assertions remain. Other spatial GPU checks
remain intact. Compositor Roto still owns an active shared vector-coverage
consumer; this change does not claim complete custom-rasterizer removal.

Renderer all-target check with spatial/shared-images/native-video passes
(63s). The focused Roto feather/hole/channel/animation roundtrip check passes
(55.69s build, 0.11s execution). Package/headless boundaries and proposal
ownership checks pass. Linux native-worker metadata and both lockfiles contain
none of the retired text/overlay packages. Unfiltered offline metadata initially
requested an uncached encode_unicode package for another platform; filtering
to the current Linux target passed without a download. No full suite was run.

Desktop check passes (59.69s), and diff whitespace review passes. Removed-code
reference searches are clear in renderer source/tests. The modified camera-plane
GPU fixture has not been executed; all-target compilation does not
establish its native pixels. Full installed-build/removal audit, the Roto backend
decision and all five slice gates remain open.

## 2026-10-12: native linear-gradient paint and editable capture

Replaced solid-color-only paint arrays with one explicit PaintSource model:
Solid or Linear. Linear gradients own start/end coordinates, an affine mapping
to object-local coordinates, spread mode and ordered color/alpha stops. Stops
have stable IDs and extension data. Validation checks coordinates, nonsingular
mapping, stop count/order, color ranges and global identity uniqueness. Artist
and agent replacements use the same expected-value transaction; retained paint
and stop extension data cannot be discarded by an edit.

Fill and stroke controls now offer Solid or Linear gradient. The gradient
controls edit endpoints, spread, stops and an advanced transform section through
the shared UI SDK. Stop insertion creates one new ID; existing IDs remain. New
stop colors interpolate the adjacent authored colors. Removed the feature-local
paint controls from the inspector into its focused paint adapter. Paint remains
static; gradient animation and direct canvas gradient handles remain open.

Native rendering emits linearGradient/stop nodes into the same fframes SVG-tree
and Skia path. Solid objects emit no empty definition containers. Capture
preserves normalized upstream gradient coordinates, transforms, spread and stop
alpha, including fill/stroke opacity. No separate parser or rasterizer was
introduced. Radial gradients and patterns still reject capture explicitly.
The new tagged paint representation advances Motion schema to 4; old paint-array
payloads have no reader or compatibility conversion and remain opaque project
data under the existing unsupported-document policy.

Extended the actual upstream/captured artwork fixture with a transformed,
reflecting three-stop gradient on fill and stroke, translucent stops/stroke,
and an object-bounding-box gradient. Original and captured 640 x 360 float frames
have zero differing pixels and zero maximum component error. Inspected the
exported gradient image. This run used Mesa Vulkan, not hardware performance
qualification. See [evidence](../resources/motion-context/linear-gradient-evidence.json),
[captured canvas](../resources/motion-context/linear-gradient-canvas.json) and
[image](../resources/motion-context/linear-gradient-capture.png).

The artist/agent edit and save/reopen fixture now edits a gradient, preserves
its IDs/extensions and rejects removal of a retained stop's unknown data; it
passes (3.82s build, 0.05s execution). All eight existing Motion UI/picking checks
pass (3.14s execution). New normal/narrow gradient control construction preserves
precise values and stop IDs without a mutation (10.51s build, negligible
execution). Affected all-target checks and desktop check pass. Native release
capture build and final pixel comparison pass; diff whitespace review passes.
No full workspace suite was repeated.

This establishes native linear-paint/capture and transaction seams. It does not
qualify direct inspector gestures, a new public MCP gradient workflow, radial or
pattern paint, exact alpha-aware gradient picking, gradient animation, HDR or
native desktop visual acceptance. All five slices remain open.

## 2026-10-12: public MCP gradient script-to-vector editing

Expanded the existing real CLI/MCP capture check into a complete public workflow.
The client inspects a native shape, proposes its replacement with a compiled
fframes gradient script, inspects and Applies it, captures output at 1/2 second,
then inspects the converted vector and proposes one gradient stop color edit.
The test uses discovered schemas and real fold_request calls over the local
project socket; this authoring phase does not mutate saved payloads directly.

Original, captured candidate and captured committed display pixels match
exactly. The gradient edit changes candidate pixels while committed pixels stay
unchanged until Apply. The edited candidate and committed result match, all
stop IDs remain, and both phases report zero CPU raster adapters and zero
working-image readback during evaluation. Explicit frame observation still
performs its legitimate review readback. Public save_project_copy produces a
project whose reopened documents match. Artist Undo commands restore the stop
edit, then the original script, then the original native shape.

The first run failed in the test harness after the stop edit: it read a revision
from the wrong Apply response field, producing a null observe_frame revision.
Added method-aware/final-job failure reporting and changed post-Apply reads to
fresh discovery. No renderer or protocol fallback was added. The complete check
then passed (28.60s build, 19.48s execution). The focused test target check and
diff whitespace review pass. Existing GPU-only Mixed-variant and render YUV
warnings remain; no workspace suite was repeated.

Updated the Fold agent skill's public workflow with the verified native gradient
edit, stable-ID/unknown-data handling and post-Apply discovery. Removed its stale
statement that script-output conversion still needed public qualification.
See [MCP evidence](../resources/motion-context/mcp-linear-gradient-evidence.json),
[captured frame](../resources/motion-context/mcp-linear-gradient-captured.png) and
[edited frame](../resources/motion-context/mcp-linear-gradient-edited.png).
Inspected the edited image: the sheared gradient visibly changes to yellow at
its edited stop. These are opaque display pixels, not HDR/alpha export evidence.

Native inspector/convert-button artist acceptance, gradient animation, radial
and pattern paint, image resources and all five slice gates remain open.

## 2026-10-12: native radial-gradient paint and capture

Schema 4 now supports radial center, focus, radius, transform, spread and stable
stop IDs through the shared native document, fframes SVG/Skia renderer and paint
inspector. Switching gradient types preserves stop IDs and extension data.
The artist/agent transaction fixture checks radial paint, save/reopen, invalid
radius rejection and undo; the desktop check passed. The focused paint-inspector
fixture passed at normal and narrow widths with unchanged precision and stop IDs.

The native capture fixture covers transformed repeating radial fill and stroke
with opacity and object-bounding-box radial paint. Capture initially misplaced
ellipse gradients: moving geometry to its local origin also requires moving
the gradient transform. The fix applies to both gradient kinds. The native
640×360 floating-point comparison passes the existing tolerance: 33 differing
pixels, maximum component error 0.00048828125. This is not exact pixel equality.
Evidence: `resources/motion-context/radial-gradient-evidence.json` and
`radial-gradient-canvas.json`. Mesa Vulkan is correctness evidence, not hardware
performance acceptance. Patterns remain explicitly rejected. Public MCP radial
authoring, native artist review and gradient animation remain unqualified.

## 2026-10-12: qualify the native upstream image path

Added the focused `motion-fframes/examples/images.rs` executable. The first
experiment used an inline PNG data URI in `href`; conversion silently produced
no image, detected by the capture assertion. The native compile-time SVG path
accepts an `Arc<usvgr::PreloadedImageData>` in the image href. The revised
fixture passes through the current retained Vulkan/Skia renderer with opaque
red/blue, half-alpha green and transparent white. Samples confirm premultiplied
linear output, including zero RGB for transparent white. Two draws match exactly.
Build and execution passed; evidence is
`resources/motion-context/image-path-evidence.json`. This is a 2×2 SDR input on
Mesa Vulkan, not decode/color-management or performance acceptance.

The implementation seam must resolve immutable project AssetIds and fingerprints
on a worker, retain decoded upstream image data, and pass it to native objects
and scripts without embedding bytes or file paths in the authored object model.
Current `Canvas::from_document` rejects asset declarations and `capture::tree`
rejects image nodes. Both remain open, along with native image objects, asset
transactions, ownership/accounting, import UI and save/reopen qualification.
A data-URI or file-path escape hatch would not meet that resource contract.

## 2026-10-12: retained named image snapshots

`Conversion` and `Renderer` now accept complete immutable `Images` snapshots.
SVG hrefs resolve explicit names through upstream `Options::image_data`. Before
publication, validation checks nonempty resource names/identities, unique image
identities, dimensions, exact RGBA storage length, premultiplied byte validity
and a caller-supplied decoded-byte allowance. Invalid snapshots leave retained
resources unchanged. Equal snapshots preserve warm caches; changed snapshots
reset upstream conversion/drawing caches to avoid stale resource references.

The focused native image executable now uses named resources. It passed color
and alpha samples, exact repeated pixels, over-budget and malformed-data rejection,
unchanged pixels after rejection, replacement with changed pixels under the same
resource name/identity, and release of removed image data (checked with Weak).
Release build and execution passed; the image-path evidence file is updated.
This bounds decoded snapshot storage only. GPU image-cache accounting, worker
transport, project AssetId/fingerprint resolution, native image content and
import/capture/save/reopen remain required. No document or public authoring
capability is claimed from this renderer seam.

## 2026-10-12: private worker image transport

The strict private worker request now includes an optional complete image
snapshot: explicit resource name, immutable identity, dimensions, premultiplied
encoded-sRGB RGBA8 storage and decoded-byte allowance. None retains resources;
an empty snapshot releases them. Worker validation is atomic and shared with
the renderer. Render and capture receive the same retained resource map.
Malformed resource updates return a correlated error with no pixel body, and
the next request can use the previous valid snapshot. No old request reader
or protocol alias was added. Existing host requests explicitly send no images.

The image executable now launches the actual persistent worker and checks five
requests: initialize, retain, replace, reject malformed storage, then retain
the replacement. All checks passed. Release example build and the affected
Motion context GPU check passed; only the existing render YUV warnings remain.
Image-path evidence records worker retention, replacement and error recovery.
Project AssetId/fingerprint loading and host-side resource leases are still
required; this transport does not yet expose native image editing or import.

## 2026-10-12: native image objects and project declarations

Added schema-4 native image content referencing a project AssetId, with stable
width/height scalar IDs and Contain/Cover/Stretch fit. Native rendering emits
upstream SVG image nodes whose href names are asset IDs. The shared creation
helper takes an imported asset and dimensions. Document asset declarations are
sorted/deduplicated from image references; replacements update them. Loading
requires declarations to match content. Generic project transactions reject
missing assets. Deleting an image removes its document declaration while
preserving the project asset library. Unknown image metadata stays protected.

The graph labels images, canvas selection/guides use their transformed bounds,
and the inspector uses existing size/transform/animation controls plus Fit.
Irrelevant paint controls are hidden. No separate image graph/model was added.
Normal/narrow native artist acceptance and import UI remain open.

The actual worker fixture now renders a native image object, forbids script
evaluation, checks an opaque-red output sample, retained/replaced resources and
error recovery. It passes. The focused project seam checks missing-asset
rejection, guarded size/fit edits, unknown metadata, save/reopen, undo/redo and
declaration removal on deletion; it passes. Its first fixture incorrectly
added unknown metadata during Replace; the existing protection guard rejected
it, and the corrected fixture preserves metadata present before editing.
Motion context UI/GPU check and release image example build passed. Evidence
in image-path-evidence.json is updated. Project file decoding/fingerprint
resolution, host leases and renderer resource upload from the project are still
required. Native images currently fail explicitly when their asset data is
not resolved, rather than silently disappearing. Public MCP image authoring,
image capture and native artist import are not qualified.

## 2026-10-12: project image resolution on the render worker

Motion's video provider binds referenced Asset records from the immutable
project snapshot without file I/O. GPU frame evaluation loads those sources
through the existing shared still-image service, including snapshot fingerprint
verification and cancellation, then sends premultiplied encoded-sRGB RGBA8 to
the native image worker. Decoded transport and worker-held resource storage
retain shared working-budget leases. Missing bindings fail explicitly.
Resources are resolved before native compilation and target allocation.

Current qualification is SDR: 8-bit sRGB raster inputs without embedded ICC
profiles, plus existing PPM sources. Unsupported input color/alpha assignments,
ICC profiles, higher precision and out-of-range samples fail explicitly; they
are not silently clamped or reinterpreted. Wider color support remains required.

The focused project-source decode test passed for exact red/green/blue byte
samples, source fingerprint change rejection and unsupported interpretation
rejection. Affected Motion context GPU compilation passed. End-to-end project
asset-to-GPU graph rendering is not yet verified. The reference-readback and
script-capture host paths are not connected to project image resources.
This first host path converts/sends the resource snapshot for each frame; warm
resource update caching is required before performance acceptance. Native artist
import, image capture and public MCP image workflows remain open.

## 2026-10-12: warm image updates and native project-to-GPU qualification

The GPU worker cache now compares immutable Asset records and shared source
file stamps on every demand. Unchanged snapshots skip decode/conversion and
resource serialization. Changed records or stamps resolve a new snapshot. The
worker resource key and owned storage lease update only after a successful
frame acknowledgement. Worker replacement always initializes resources again;
failed/cancelled demands do not establish a successful cache entry. Added an
image-snapshot counter to the existing runtime statistics. No second decode
cache was introduced.

Added the focused `render_image_asset` native GPU example. Its actual project
asset and native image object pass through bound immutable snapshots, the
retained compiled worker and shared GPU image import. On NVIDIA GTX 1070:
opaque red/blue samples passed; two initial frames matched exactly and sent
one resource snapshot; evaluation reported zero CPU upload/readback bytes
before explicit qualification readback. A changed file without a new
fingerprint was rejected. Updating the Asset fingerprint rendered new black
pixels. Guarded native deletion rendered a fully transparent frame and sent
an empty resource snapshot. Total successful updates: three.

Affected GPU compilation, the example build and native execution passed.
Evidence: `resources/motion-context/project-image-gpu-evidence.json`. This
qualifies the direct project snapshot/native GPU runtime seam, not the full
VideoRegistry composition/export pipeline or public MCP image workflow.
Native artist import, image capture, wider color/precision support, GPU image
cache accounting and production-size latency remain open. The earlier
per-frame resource serialization limitation is now removed for unchanged
snapshots; file-stamp checks remain intentionally active.

## 2026-10-12: shared browser image placement in Motion

Motion now registers the existing browser Placement capability. Verified still
asset metadata creates a native image through guarded Insert, fitting down to
the document size and centering it. Graph drops preserve graph presentation
position and can insert into the open group; group centering uses the shared
inverse authored transform. Protection and transaction validation remain shared.
Document outputs, audio, timing trims and overwrite/track placement are rejected
rather than mapped to unsupported Motion behavior.

The Motion editor reuses SDK Project payload drops and native FileDrop regions
on the viewer and graph. File import and placement use the existing application
worker and one atomic project proposal; no media I/O or chooser was added to
the UI thread, and no extra toolbar row was added. Drops are withheld during
pending commands or active artist edit gestures.

Expanded the existing application import-and-place seam to Motion. The real
Session browser command imports a PPM, creates a named native image at the
requested graph position, saves/reopens exactly, undoes asset plus image in one
action, redoes, and places the same imported asset again without duplicating
it. The focused test passed across the existing destinations plus Motion.
The Motion UI/GPU compile check passed. Native OS/Project drag delivery, normal
and narrow artist visual acceptance and automatic selection remain unverified.
Viewer placement is centered rather than at the cursor; positioning at the
drop point and multi-image arrangement remain required interaction work.
Public MCP image import/review and full composition/export qualification remain
open. The first import workflow does not close the artist-tools slice.

## 2026-10-12: independent canvas drop coordinates

Added an explicit optional image-space center to the shared browser placement
request, separate from graph presentation coordinates. Motion advertises this
capability; other destinations reject it rather than silently ignoring it.
Viewer Project drops convert the current pointer into authored image space.
Native file drops use the shared SDK screen-to-image transform at delivery.
When canvas coordinates are supplied, the SDK leaves graph coordinates intact.
Group placement still uses the native inverse parent transform.

Batch import offsets successive canvas centers by 24 pixels in both axes; graph
layout offsets remain independent. This is a simple initial arrangement, not
a qualified layout/distribution tool. Fixed viewer drop admission during active
artist gestures in addition to pending commands.

The application import-and-place seam passed with graph position [123,456]
and independent canvas center [320,200], plus its existing atomic undo,
save/reopen and asset-reuse checks. The shared FileDrop fixture passed for
transformed canvas delivery without changing graph position, and for clearing
hidden/old targets. Affected Motion and Timeline UI/GPU compilation passed.
Native OS drag delivery, normal/narrow artist acceptance, automatic selection
and batch-arrangement acceptance remain open. The earlier centered-only viewer
placement limitation is now removed in source and at the checked seams.

## 2026-10-12: placement selects native images without taking focus

Placement providers can now report newly created stable native object IDs from
the before/after document pair. Motion reports them in document hierarchy order.
The application does not parse Motion content to infer selection. Existing-asset
placement selects those objects only when the artist is already in the target
document. Import-and-place proposals carry the same provider-owned selection hint.

Background jobs retain the originating shared artist selection. After successful
publication, import applies its hint only when the current selection still
matches the origin and target document. Changing selection while an import
runs preserves the newer choice. Project epoch, cancellation, revision and
active-edit checks remain in the existing publication path. No navigation or
focus change is added.

Expanded the application placement seam: imported Motion images select themselves;
a synchronous existing-asset drop selects its new image; undo restores the prior
document; and changing selection before asynchronous completion prevents focus
theft. The focused test passed, together with its existing save/reopen, asset
reuse and atomic undo checks. App test-target compilation passed in default and
GPU configurations. Native artist drag delivery and normal/narrow visual
acceptance remain open; this verifies selection at the application contract.

## 2026-10-12: compatible image relinking and shared source checks

Motion now implements the shared Relink capability for referenced native image
assets. It validates the document/reference and qualified still-image profile.
The existing ingest worker retains fingerprint probing, compatibility checks,
atomic publication and undo. Relinking changes the Asset record; native object
IDs, authored dimensions/fit/transforms, animation and document payload stay
unchanged. This uses the existing compatible-source policy, including matching
profile/dimensions; broader replacement policy remains open.

Extracted one shared Motion image-source qualification service for placement
and decode boundaries. Placement now rejects unsupported color/alpha assignments
before creating an image, rather than committing an object whose render will
fail. Decoding still validates actual file data, range and fingerprint. No
second decoder or renderer was added.

The focused application relink seam passed for source replacement, exact
document/identity/edit preservation, unknown asset metadata, undo/redo, and
rejection of unsupported input assignments without placement changes. The
existing worker decode/fingerprint/assignment test also passed after extraction.
Affected Motion UI/GPU compilation passed. Native relink chooser delivery and
artist visual acceptance remain unverified. The earlier GPU asset-fingerprint
replacement fixture is separate pixel evidence, not a native chooser test.

## 2026-10-12: shared image control visibility and targeting

Native objects now provide one semantic scalar-control list, separate from the
complete stored-scalar list used for document validation. The image inspector,
parameter discovery/sampling and animation discovery/editing use this same list.
Image stroke width is excluded because it has no rendering effect. Guarded
scalar edits also reject that target. Image dimensions, transforms and opacity
keep their existing stable IDs and controls. Full storage validation, including
unused paint scalar data, remains intact; no alternate document format was added.

The focused authoring seam passed: width is discoverable and sampleable, image
stroke width is absent and cannot be sampled or edited, and an invalid stored
stroke scalar still fails document validation. Motion UI/GPU compilation passed
(2.33 s), with existing render YUV warnings. The first test compile exposed a
fixture constructor typo, corrected before the passing run. Native artist visual
acceptance and complete public MCP image workflow qualification remain open.

## 2026-10-12: native image edits through real MCP and candidate pixels

Added one image case to the existing subprocess workflow instead of a separate
protocol harness. Its project contains an actual two-color PPM asset and native
image with shear. The agent inspects the document, proposes a guarded position
scalar edit through `fold-cli mcp`, reviews candidate pixels, verifies committed
pixels remain unchanged during preparation, inspects the receipt and Applies.
Candidate and committed frames match exactly; the edit changes pixels. Evaluation
reports zero CPU adapter nodes and zero evaluation readback bytes (pixel
observation remains an explicit readback). Save/reopen retains exact state;
artist Undo restores the original native canvas and Redo restores artwork and
asset records. Revision counters intentionally advance across undo/redo.

The focused GPU test passed in 3.91 s after correcting a fixture assertion that
compared project revision counters after Redo. Initial sandbox execution could
not bind the local Unix socket; the approved native run used the same compiled
test with local socket and Vulkan access. No engine fallback was added. Existing
YUV and unused Mixed-test-variant warnings remain. Curated evidence is in
`resources/motion-context/mcp-image-evidence.json`, with before/after PNGs; the
after image was visually inspected. The Fold public-workflow reference now
includes the verified native image parameter edit. Agent image import/creation,
fit-change public workflow, and native artist visual acceptance remain open.

## 2026-10-12: agent image creation with editable native identity

Extended the same real MCP image seam to begin with an empty Motion canvas and
an existing project Asset. The subprocess proposes a native Insert with stable
object/scalar IDs, inspects its receipt and candidate pixels, confirms committed
pixels remain empty during preparation, and Applies. Creation candidate and
committed frames match exactly. Fresh discovery/inspection then edits the same
object through the existing guarded scalar request. Save/reopen and two-step
artist Undo/Redo cover both creation and later edit. Undo creation removes the
native object and derived document asset reference while preserving the source
asset library record. No new authoring protocol or alternate object model was
needed.

Focused native MCP execution passed in 6.89 s; compilation passed with existing
YUV and unused Mixed-variant warnings. Updated the public Fold workflow with
schema-driven image Insert guidance and the curated image evidence. Agent file
import, fit-change public qualification, broader image formats/color and native
artist visual acceptance remain open. This fixture supplies its initial asset;
it does not qualify file import through MCP.

## 2026-10-12: authored stroke caps and joins in vector selection

Path pointer geometry now separates finite stroke strips from authored endpoint
caps and joins. A wide butt stroke no longer inherits an unintended round end;
round caps use the endpoint disk, square caps use the endpoint tangent and
square extension. Closed contours have no endpoint caps. Join selection now
covers round and bevel geometry, and miter-limit overflow falls back to bevel
instead of losing that visible region. Existing cubic endpoint derivative
handling and ancestor-transform inversion remain in use. Artwork still renders
only through fframes/Skia; this is pointer geometry, not a second rasterizer.

Four focused picking tests passed, including cap differences at high zoom under
nonuniform/sheared/reflected transforms, both open endpoints, closed contours,
bevel/round/miter-limit fallback, existing miter tips, Bézier insertion and filled
holes. Build took 33.10 s; test execution completed below 0.01 s. Existing YUV
warnings remain. Five-screen-pixel minimum stroke targets remain deliberate;
curve flattening retains its quarter-screen-pixel tolerance. Degenerate/cusp
strokes and gradient-alpha-sensitive picking remain unqualified. Native artist
pointer and normal/narrow panel acceptance remain open.

## 2026-10-12: artist 3D parenting through shared native Move

The hierarchy context menu now exposes `Parent (keep local transform)` with
Scene root and eligible group destinations. It excludes the current parent,
self, descendant groups and protected destinations. The artist action uses the
existing guarded native Move edit, validates the complete candidate, previews
through the shared extension path and commits once. IDs, object payload,
transform/animation, geometry/material resources and selection remain intact.
Destination placement appends to that parent's ordered children. Busy gestures
and playback gate the command. No independent hierarchy state or authoring
protocol was added.

This action explicitly preserves authored local transforms; a transformed parent
can change world position. World-preserving reparenting, including animated and
nonuniform/sheared transforms, remains open. It must be designed and qualified
rather than silently approximated through this local-transform operation.

The existing focused 3D artist contract passed (9.69 s build, 0.22 s execution),
extended with move out of a group, move back, exact native object/resource and
selection preservation, self-cycle rejection and undo of both moves. It also
retains the prior normal/narrow ImGui hierarchy checks. Native context-menu
interaction and visual review of the new menu remain unverified. Existing render
YUV warnings remain. All replacement slices remain open.

## 2026-10-12: exact static world-preserving 3D parenting

Added a native `move_world` edit with expected placement and destination parent.
Artist world-parenting submits this same edit; agent authoring discovers it in
the existing provider schema. It generates guarded Move/Parameter edits within
the same candidate transaction, retaining scalar/object IDs, metadata, protection
and resources. The previous local-transform operation remains a distinct explicit
choice. Neither operation creates a new hierarchy model.

World parenting reuses native transform sampling, composes complete affine
ancestry, inverses the destination world transform and decomposes the required
local transform only when TRS can represent it within floating-point tolerance.
Shared affine inverse/TRS helpers verify reconstruction. Static translated,
rotated and scaled ancestry is supported. Animated source/destination ancestry
and required local shear are rejected before publication; animation baking or
an approximate matrix fit is not substituted. Full animated/world-affine
parenting remains open.

Focused evidence passed: affine inverse and TRS round trips, including 180-degree
rotations around all axes and shear rejection; 3D native world evaluation parity
at three rational times after changing parents; stable IDs/resources; semantic
edit equivalence and stale placement rejection; rejection of animated ancestry
and required shear. The artist extension/undo fixture passed after exercising
world parenting out of a group and local parenting back (15.30 s build, 0.22 s
execution). Existing YUV warnings remain. Native menu visual/interaction review
and real MCP subprocess qualification of `move_world` remain open.

## 2026-10-12: contour direction editing with retained Bézier identity

Added Reverse direction inside the selected path's existing Contours inspector
section. It reverses anchor traversal and swaps each anchor's incoming/outgoing
offsets, preserving positions, point/contour IDs, closed state and unknown data.
The artist submits the same guarded native object replacement used by agents;
no second edit state or protocol operation was added. Reversal changes winding
and can intentionally change holes under nonzero fill; the control explains
this consequence in its tooltip. No toolbar row was added.

The focused native edit check passed: cubic samples match the reverse-parameter
samples of the reversed contour, including its closing segment; metadata and IDs
survive; reversing twice restores the exact contour; shared project Undo restores
the original canvas. Build took 5.74 s, execution below 0.01 s. Affected Motion
UI/GPU compilation passed (9.62 s), with existing YUV warnings. Native control
normal/narrow review and real MCP reversal pixel qualification remain open.

Path-point animation still requires its own complete model/evaluation/editor
phase. Current anchors and handles are unanimated coordinate arrays; scalar
animation targets do not yet include path coordinates. Adding renderer-only
animation would leave artist picking and editing inconsistent, so this turn did
not add a separate animation representation or rendering-only path.

## 2026-10-12: choose the first anchor of a closed contour

Artists can right-click a nearby anchor on a selected closed path and choose
Make first anchor. The first anchor has a small ring in the existing selection
guide. The operation rotates the ordered anchor list; it preserves winding,
Bézier handles, coordinates, IDs, closed state and metadata. It submits a guarded
native replacement through existing preview/commit, so agents can express the
same document edit and shared Undo restores it. Current first-anchor requests
are no-ops. Open contours and protected artwork reject the operation. No new
panel or toolbar row was added.

The focused model/edit check passed (1.24 s build, below 0.01 s execution),
verifying exact ordered point records, no-op behavior, stale expected-object
rejection, restoration through the inverse rotation, and open/protected rejection.
Motion UI/GPU compilation passed in 0.94 s with existing YUV warnings. The native
right-click delivery, ring appearance at normal/narrow sizes, and MCP pixel
qualification remain open. This is traversal-origin editing; it does not add
path-point animation, trim paths or change the path's geometric shape.

## 2026-10-12: native anchor animation with one coordinate evaluator

Path anchors now carry an optional native typed two-component position track,
reusing the shared graph binding Track model (exact rational keys, interpolation,
key identity and extension data). Tracks use replacement canvas coordinates;
malformed types, nonreplacement tracks and identity collisions reject at document
validation. Static coordinates remain the authored base; absent tracks serialize
as before. This is an additive native field, not a second document or old-schema
reader.

Point position sampling now supplies the native fframes path renderer, path
picking, pen segment targeting, selection guides, first-anchor targeting and
handle editing. Sampled contours retain point identity without cloning animation
tracks. Dragging an animated anchor inserts/updates its current-time key. Auto
Key creates a position track for a static anchor. Handles remain offsets from
the sampled anchor, and ordinary handle editing retains that position track.
Guarded Point and object replacement preserve track/key unknown metadata and
track identity. Contour reversal and first-anchor rotation retain tracks.
Splitting a segment with animated endpoints explicitly rejects until an
animation-preserving subdivision is implemented.

Eleven affected native model/tool tests passed, including a new anchor test for
rational midpoint sampling, Auto Key creation, current-key edits, stable track
and existing key IDs, metadata guards, sampled handle offsets and serialized
native document round trip. A fixture import typo and one constructor omitted
by the initial field update were corrected before passing checks. Motion UI/GPU
test-target compilation passed (1.81 s), and the isolated native fframes bridge
check passed (1.02 s). Existing YUV warnings remain. Updated both dependency locks
for the shared binding dependency.

This implements anchor position animation, not complete path animation
acceptance. Parameter/curve-channel discovery and curve-editor integration,
animated handle offsets, animation-preserving splitting, native rendered pixel
qualification, real MCP animation workflow and artist interaction review remain
open. No renderer-only animation path or static-coordinate editing fallback was
added.

## 2026-10-12: anchor parameters and shared coupled curve channels

Native path anchors now appear in parameter discovery as canvas-space vectors
with X/Y component descriptors. Their stable parameter identity is the existing
point ID under its owning path object. Contour/anchor order provides presentation
labels only. Sampling calls the same position evaluator used by rendering and
picking; object/ancestor protection controls editability.

Anchor tracks now project into the shared Animation capability through the
existing typed-track curve service used by 3D. X/Y curves share native key IDs;
curve edits group components back into one point track and submit guarded Point
edits. Scalar channels retain their existing targets. Forged/repeated anchor
channel targets reject. Clearing projected anchor animation captures its pose at
the explicit time as static coordinates, subject to the same unknown-metadata
guards. No independent curve storage or per-anchor inspector rows were added.

The focused authoring seam passed: vector and component values at rational time,
protected descriptor state, channel discovery, X edits preserving Y values,
stable point/track/coupled key IDs, inconsistent and repeated target rejection,
and static pose after clearing curves. Existing script-control and image-control
seams also passed during iteration. Corrected a test's lowercase component name
assumption (shared curves use X/Y) and promoted the shared animation dependency
from test-only to normal use. UI/GPU compilation passed (1.08 s), with existing
YUV warnings. Native curve-editor interaction/visual review, rendered trajectory
pixels and real MCP anchor-animation qualification remain open.

## 2026-10-12: native rendered anchor trajectory qualification

Added a focused native bridge executable, `examples/anchor_animation.rs`, which
renders a translucent stroked cubic with animated first anchor and an authored
shear. It checks five exact-time poses: 0, 1/2, 1, 2 and a return to 0 seconds.
Each animated frame is compared against an independently specified static anchor
coordinate, not a static pose produced by the animation sampler. All Float RGBA
pixels match exactly. Repeated warm frames also match, moving poses change
pixels, endpoint extrapolation holds and returning to the first time restores
its pixels. No script evaluation is permitted by this fixture.

`cargo run --manifest-path crates/motion-fframes/Cargo.toml --example
anchor_animation --offline --release -- /tmp/fold-anchor-animation-evidence`
passed; optimized compilation took 22.40 s. Existing YUV warnings remain. The
curated `resources/motion-context/anchor-animation-evidence.json` records exact
rational times, coordinates, device UUID and Float RGBA hashes. Beginning/end
PNG previews were exported from the qualified samples and visually inspected.
This is direct native fframes Vulkan renderer evidence, not full application,
worker transport, export or MCP trajectory acceptance. Those wider checks and
native artist curve/handle interaction remain open.

## 2026-10-12: real MCP anchor animation, pixels and parameter sampling

Extended the retained MCP subprocess workflow with a native cubic anchor case.
The agent inspects a static point, proposes a typed position track, inspects its
receipt and candidate pixels, confirms preparation leaves committed pixels
unchanged, and Applies. Fresh discovery finds the point's stable vector target;
public parameter sampling at 1/2 second returns [120,100]. A later guarded Point
edit changes the endpoint key without replacing the point or track identity.
Candidate/committed frames match exactly after both Applies; the key edit changes
the midpoint pose to [160,120]. Source coordinates remain local to the sheared
path object.

The full application route exercises the native Motion worker and shared GPU
image evaluation, with zero CPU adapter nodes and zero evaluation readback bytes
before explicit observation. Save/reopen retains exact project state. Artist
Undo restores the prior animation pose with the same point/track IDs, then the
original static path. The focused native MCP run passed in 27.07 s after a single
72 s affected application build; existing YUV and unused Mixed-variant warnings
remain. It used approved local Unix socket/Vulkan access.

Curated evidence and before/after PNGs are under
`resources/motion-context/mcp-anchor-evidence.json` and `mcp-anchor-*.png`; the
after image was visually inspected. The Fold public workflow now documents
schema-driven position-track creation and later guarded point edits. This adds
actual public protocol/pixel evidence to the prior direct native-render and
parameter/curve contract checks. Animated handle offsets, animated subdivision,
native artist curve/gizmo acceptance and exported trajectory parity remain open.

## 2026-10-12: animated Bézier handle offsets in the native point model

Added optional incoming/outgoing offset tracks beside the anchor position track.
All three use one typed vector evaluator, exact rational time and canvas units.
Native path rendering and selection guides consume the same sampled contour;
viewport handle hit targets and edits now use sampled offsets. Auto Key creates
handle tracks, animated handle edits update the current key, and linked handle
editing keys the opposite offset in the same guarded Point edit. Static handle
edits retain their prior behavior.

Document validation covers every coordinate track and global track/key identity.
Point/object guards preserve track/key extension data and identity. Reversing
contours swaps both handle offsets and their tracks, preserving the original
animation identity. Sampled geometry carries no cloned tracks. The source
handles remain offsets relative to the animated anchor. Native captured/static
point constructors initialize all tracks explicitly; absent tracks remain
omitted in saved JSON. No second path animation representation was added.

Eleven affected model/tool checks passed (3.53 s build, 0.90 s execution), including
linked handle Auto Key, later handle-key interpolation, stable handle track ID,
sampled contour offsets and guarded reversal with track identities crossing
incoming/outgoing roles. Motion UI/GPU test-target compilation and the isolated
native bridge check passed. Existing YUV warnings remain.

This is the data/evaluation/direct-edit prerequisite for animated cubic
subdivision. Handle parameter/curve-channel publication, rendered handle-motion
pixels, MCP handle workflows and native artist interaction remain open. Animated
subdivision still rejects before mutation; it must derive moving handles and
retain the curve for all times rather than freezing a sampled pose.

## 2026-10-12: curve-preserving subdivision with linear coordinate animation

Pen insertion now subdivides animated anchors and handle offsets without freezing
the current pose. It derives the new point and handles from the union of exact
source key times. Fixed-parameter de Casteljau is affine in these coordinates, so
linear source intervals remain exact linear intervals after subdivision. Existing
endpoint handle tracks are scaled while retaining track/key IDs and extension
data; only the inserted point receives fresh tracks and keys. Static straight
segments retain zero handles.

The operation uses the explicit editing time. When animated handles both collapse
and picking sees a line, its line fraction is converted to the corresponding cubic
parameter, keeping the inserted anchor at the clicked location. Non-linear source
intervals and unions exceeding the native 4096-key limit reject before mutation.
The final key's unused interpolation does not prevent subdivision. No sampled
pose fallback, baked approximation or second path model was introduced.

Five focused tools tests passed (1.10 s build, 0.02 s execution). The new trajectory
check compares both split cubics against the original across 81 rational times
and 101 curve positions, with different source key schedules, endpoint holds,
collapsed handles, stable IDs and retained extension data. Hold timing rejects
without changing the document. The earlier static test also covers closing-edge
subdivision. Motion UI/GPU test-target compilation passed (5.92 s); existing YUV
warnings remain. Diff whitespace review passed. Native artist insertion, animated rendered-pixel/export parity,
non-linear timing and public MCP subdivision qualification remain open.

## 2026-10-12: shared parameter targets for native path handles

Motion now discovers incoming/outgoing offsets beside each anchor position,
including whole-vector and X/Y component targets through the shared parameter
service. Stable provider property keys combine the existing native point ID
with the fixed incoming/outgoing role; readable contour/anchor labels are only
paths, not identity. No extra stored handle IDs or parallel animation model were
introduced. Handle values remain offsets in path-local canvas coordinates, not
absolute handle locations. Sampling uses the same exact-time evaluator as native
rendering and editing, for both static and animated offsets. Descriptor editability
uses the same object/ancestor protection as anchors.

The focused authoring seam passed with six handle descriptors, static incoming
sampling, animated outgoing midpoint vector/component sampling, and the existing
anchor curve identity/edit checks. Final test build took 0.80 s and execution
0.01 s; whitespace review passed. This qualifies provider discovery/sampling,
not real MCP discovery, artist controls or handle curve editing. Handle curve
channel publication and grouped point edits remain the next shared-control gap;
public workflow instructions will be added after the actual MCP route qualifies.

## 2026-10-12: shared curve channels for path handles

Incoming/outgoing offset tracks now publish X/Y curves beside anchor-position
curves through the shared animation provider. Handle channel identities use the
same native point ID and fixed role as parameter targets. Artist labels remain
separate from identity. Component curves still project coupled typed vector keys;
the shared graph curve service owns timing, key coupling and value replacement.

Animation edits group requested components by object, point and coordinate role,
then generate one guarded Point edit per point. A combined anchor/handle change
therefore commits atomically without competing expected-point snapshots. Existing
track/key IDs and extension guards remain authoritative. Clearing a coordinate
track captures its evaluated offset/position at the explicit edit time; other
point properties remain intact. Protection still rejects before document mutation.

The focused authoring seam passed with simultaneous anchor-X and outgoing-Y
changes, preserved outgoing X, retained handle track/key IDs, grouped clearing
at the midpoint, protected handle edits and protected discovery. Final focused
build took 0.85 s, execution 0.01 s. No broad workspace build was run. Native curve
editor interaction, handle-motion pixels/export parity and real MCP handle
creation/edit/discovery still require qualification; no public workflow is yet
claimed for those unverified routes.

## 2026-10-12: native rendered anchor and handle animation

Extended the existing native anchor fixture with outgoing and incoming offset
tracks on different key schedules. Independently specified static poses now
cover all moving coordinates at 0, 1/2, 1 and 2 seconds, then return to time zero.
The sheared, translucent cubic renders through actual fframes/Skia Vulkan.
Every animated frame matches its specified static pose exactly in Float RGBA;
warm repeats match exactly, each successive moving pose changes pixels, and
returning to the initial time restores exact initial pixels. Script execution
is prohibited by the fixture. The later handle endpoint and earlier incoming
start also exercise endpoint holding independently of the anchor track.

One optimized example build/run passed (10.56 s compilation). Existing YUV
warnings remain. `resources/motion-context/handle-animation-evidence.json`
records exact rational times, specified positions/offsets, device UUID and raw
Float RGBA hashes. Beginning/end PNG previews were exported from the qualified
samples and visually inspected. The device UUID identifies the available Mesa
renderer; this is correctness evidence, not hardware speed acceptance. Prior
anchor-only evidence remains separate and unchanged.

Command: `cargo run --manifest-path crates/motion-fframes/Cargo.toml --example
anchor_animation --offline --release -- /tmp/fold-handle-animation-evidence`.
This qualifies direct native handle sampling/rendering and cache time changes.
It does not qualify worker transport, real MCP handle authoring, subdivision
raster parity, exported trajectories or native artist curve/handle interaction.
Those wider checks remain open.

## 2026-10-12: real MCP outgoing-handle animation and identity

Extended the retained native anchor MCP workflow with outgoing offset animation.
The actual CLI/MCP subprocess creates position and outgoing tracks in one guarded
Point edit, reviews and Applies its immutable candidate, then uses fresh public
parameter discovery to sample the outgoing vector at 1/2 second: [60,10]. A later
Point edit updates both endpoint keys, producing [80,30] outgoing offset and
[160,120] anchor position at the midpoint. Existing point/track/key identities
survive the later edit and first artist Undo; the second Undo restores the exact
original static canvas. Save/reopen preserves exact project state.

Both preparations leave committed pixels unchanged. Candidate/Apply frames match
exactly through the native Motion worker and shared GPU evaluation. Evidence
records zero CPU adapter nodes and zero evaluation readback bytes before explicit
observation. One affected GPU test build passed (27.78 s), then the approved local
Unix-socket/Vulkan workflow passed (21.41 s). Existing YUV and unused Mixed-variant
warnings remain. No full workspace build was run.

Curated `resources/motion-context/mcp-handle-evidence.json` records frame hashes,
transfers, sampling and persistence/Undo checks. Before/after PNGs retain prior
anchor-only evidence separately; the after image was visually inspected. Updated
the Fold public workflow with typed handle-track authoring and discovered handle
parameter sampling. This qualifies outgoing-track creation and later editing via
the real protocol, not incoming-track MCP authoring, public curve edits,
subdivision raster parity, export trajectories or native artist interaction.

## 2026-10-12: guarded semantic subdivision for agent authoring

Added native `split_segment` to the discovered Motion edit schema. It takes the
owning object and inspected expected object, contour/start IDs, segment fraction
and explicit editing time. The provider invokes the same subdivision operation
as Pen insertion, then applies its guarded replacement through the shared edit
service. Generated point/track/key IDs belong to the resulting candidate;
subsequent edits inspect that candidate or committed document. Standard request
UUID retries must reuse the original request, not generate another candidate.
No second subdivision algorithm, alternate animation model or compatibility
alias was added.

Five focused tools checks passed. The trajectory check now round-trips the
semantic JSON command and compares its sampled positions/handles against artist
tool output across all 81 tested times, including collapsed handles. Stale object
and protected-object requests reject. One project transaction Undo restores the
exact original canvas; Redo restores the exact committed document and generated
identities. The final focused Undo/Redo addition passed (0.49 s build, 0.03 s
execution). Affected Motion UI/GPU test-target compilation passed (1.54 s);
existing YUV warnings remain.

This qualifies schema/provider semantics and shared transaction behavior. Real
MCP subdivision candidate/Apply pixels, subdivision raster parity and native
artist insertion still require qualification. Public agent workflow examples
will be added after that actual protocol route passes.

## 2026-10-12: subdivision raster measurement exposes remaining edge differences

Extended the retained native animation fixture to render the semantic subdivided
curve at the same five poses. Warm subdivided output repeats exactly. The prior
independently specified animated/static pose comparison still passes exactly.
However, original versus subdivided raster output is not exact: distinct poses
change 11, 6, 4 and 10 of 6144 pixels, with maximum Float component errors
0.35229492, 0.35229492, 0.17626953 and 0.34936523 respectively. Returning to time
zero repeats its 11-pixel difference. The changed samples occur at several curve
edges, not only at the inserted anchor. Alpha changes are quantized in roughly
0.176 steps for this translucent stroke; the cause is not yet established.
Geometry equality from the trajectory check does not prove raster equality.

One optimized build/run completed (10.03 s compilation). Curated
`resources/motion-context/subdivision-render-evidence.json` records measured
errors and both raw-frame hashes, explicitly marking raster acceptance false.
Beginning original/subdivided PNGs were exported; the subdivided image was
visually inspected. No tolerance was relaxed to turn the measurement into a pass.
This is direct native Mesa/Vulkan correctness investigation, not hardware speed,
full application or artist acceptance.

Next: identify upstream Skia stroke/antialias behavior behind the edge differences
and qualify an appropriate render policy without retaining an alternate renderer
or changing the authored geometry. Real MCP subdivision and native artist
insertion remain open independently of this raster investigation.

## 2026-10-12: subdivision cache hypothesis rejected for initial pose

Added a fresh-device-context/render-cache comparison to the native fixture's first
subdivided pose. Its Float RGBA pixels exactly match the retained renderer output.
The original/subdivided differences repeat with identical pixel counts and maximum
errors across all five poses. This rejects retained renderer/cache dependence for
the tested initial pose; it does not establish the stroke/coverage cause.

One focused optimized example rebuild/run passed (1.62 s compilation), retaining
the fresh-versus-retained assertion. The existing raw-frame comparison explicitly
fails exact original/subdivided equality, so the issue still has a repeatable
failure signal. The curated subdivision evidence records the fresh-renderer probe.
Source inspection confirms Fold wraps a single-sample Float RGBA Vulkan target
and upstream draws through Skia's antialiased stroke paint; this does not by itself
explain quantized edge coverage. Next investigation is upstream path conversion
and stroke rasterization. No render policy or acceptance threshold was changed.

## 2026-10-12: CPU raster probe isolates segment-dependent stroke differences

Added an explicitly diagnostic CPU Skia surface to the native example only. It
uses the same upstream Conversion trees, renderer stroke paint and linear Float
RGBA color policy, with fresh caches. No application, worker or production render
fallback was added. At time zero, original versus subdivided CPU output differs
at 88 pixels, maximum Float component error 0.093566895. GPU output repeats its
11-pixel, 0.35229492 maximum difference. The mismatch therefore is not confined
to GPU rendering, although the error patterns differ by backend.

Upstream path conversion copies tiny-skia control coordinates and cubic verbs
straight into `skia::Path::raw`; no intermediate flattening is present in that
conversion. Together with shared geometry trajectory equality and the fresh
cache probe, the evidence points toward segment-dependent stroke rasterization.
It does not yet prove whether stroke approximation, float control rounding or
coverage selection dominates the discrepancy. Exact raster invariance remains
unproven and is not marked accepted.

The focused optimized example passed (1.85 s compilation). Curated subdivision
evidence includes the CPU probe. It remains diagnostic code in the example,
not a second application renderer. Next probe: Skia GPU path-renderer options,
then choose a policy based on edge quality and measured cost, without trading
one for the other silently. No production rendering policy changed this turn.

## 2026-10-12: GPU tessellation option changes coverage but does not solve parity

Temporarily disabled Skia's tessellation path renderer in the Vulkan context and
ran the retained native animation/subdivision fixture. With that one option,
original/subdivided output differs at 88, 107, 47 and 57 pixels for distinct poses;
maximum Float component errors are 0.093566895, 0.18457031, 0.15405273 and
0.20361328. The initial GPU error pattern now exactly matches the CPU probe's
count and maximum; the default GPU pattern differs substantially. This supports
GPU path-renderer selection as a contributor to edge coverage, while confirming
that disabling tessellation does not give subdivision invariance.

The first probe compile exposed private ContextOptions fields; using a mutable
Default value and setting the public option resolved it. The focused optimized
fixture then passed its existing pose/warm assertions (2.80 s compilation), but
subdivision raster parity remains false. Curated subdivision evidence includes
all option-probe measurements and frame hashes. No performance acceptance was
attempted for this configuration.

Restored the exact default `make_vulkan(&backend, None)` source immediately after
the probe. No option switch, environment override, alternate backend or new
production policy remains. The release example executable from this experiment
contains the probe setting and must be rebuilt from restored source before any
further production-default measurements. Next compare edge quality against a
higher-resolution reference and measure cost before choosing any render policy;
exact subdivision parity alone is not a sufficient quality/performance criterion.

## 2026-10-12: higher-resolution edge reference clarifies quality tradeoff

Extended the diagnostic example with 8x and 16x CPU Skia rendering, box-reduced
in linear premultiplied Float RGBA. At the initial pose, 8x versus 16x differs by
maximum component 0.02454019 and total absolute component error 1.88035. Original
versus subdivided 16x references differ by maximum 0.00847930 and total 0.38106.
These are converging raster references, not analytic ground truth or a production
supersampling policy.

Against the original 16x reference, default GPU original/subdivided errors are
maximum 0.23268527/0.25370729 and total 27.44352/27.52245. CPU original/subdivided
errors are maximum 0.18755192/0.18755192 and total 15.38568/15.70165. The tested
GPU total is roughly 78% greater than CPU total for the original first pose.
Combined with the prior no-tessellation probe, this identifies a meaningful edge
quality tradeoff in backend selection; neither exact pixel invariance nor a
single maximum alone is a sufficient acceptance measure. The reference's own
uncertainty is recorded rather than hidden.

One focused optimized build/run passed (2.93 s compilation) from the restored
default Vulkan source; its subdivision measurements repeat the original GPU
pattern. Curated subdivision evidence contains all reference comparisons.
Only example diagnostics changed. No CPU fallback, production supersampling,
quality setting or render policy was introduced. Remaining work: assess supported
GPU coverage options and frame cost on representative native hardware before
choosing a final quality/performance policy; native artist edge review remains open.

## 2026-10-12: eight-sample internal GPU coverage improves tested edge accuracy

Temporarily set Skia ContextOptions.internal_multisample_count to eight, retaining
the default tessellation renderer. The Mesa first-pose error against the 16x
reference drops from total 27.44352 to 16.98598 (about 38%), maximum component
0.23268527 to 0.13150680. Subdivision differences remain; their counts and maximum
errors are recorded rather than treated as exact parity. The option is supported
only as hardware capabilities permit; this is not a universal device guarantee.

Ran both eight-sample and restored-default fixtures with approved native Vulkan
access on the same distinct physical-device UUID. Native first-pose total errors
are 16.95935 with eight samples versus 27.41689 at default (about 38% reduction),
maximum 0.13192052 versus 0.23561496. Both configurations retain exact specified
static-pose and warm parity. The curated subdivision evidence contains native and
Mesa configuration probes, device UUIDs, frame hashes and reference comparisons.
Native results corroborate the quality improvement without establishing broader
stroke, paint, image, text or device acceptance.

Eight-sample compilation took 2.92 s; restored-default compilation took 2.95 s.
Both focused native runs completed successfully. Source and release example now
use the restored default context, with no override or alternate production path.
No complete-frame speed claim follows from fixture wall time, which includes CPU
reference generation and explicit readbacks. Next measure warm GPU/frame cost on
representative scene sizes before deciding whether to adopt eight-sample coverage.

## 2026-10-12: measured stroke cost and experimental eight-sample default

Added a focused native cost example with 128 and 1024 translucent curved strokes
at 640x360, animated translations, retained geometry/target and exact rational
time. It discards ten frames and records forty warm frames with conversion,
commands and submit CPU stages. Each frame includes explicit Float readback to
synchronize completion; these timings are not isolated GPU time or full-app FPS.

On the same native device UUID, default versus eight-sample median/p95 times are
4.394/4.918 ms versus 4.424/4.681 ms for 128 strokes, and 20.521/27.087 ms versus
19.077/22.064 ms for 1024 strokes. This single paired order shows no clear overall
slowdown; the apparent speedup is not asserted as a performance gain. Readback,
CPU work and run variation can mask GPU-only cost. Both native runs completed
successfully; targeted optimized builds took 1.22 s and 2.34 s. Evidence is in
`resources/motion-context/stroke-cost-evidence.json`.

Based on the prior approximately 38% first-pose edge-error reduction on native
hardware and Mesa, and this initial cost check, adopted a single experimental
Skia policy: request eight internal multisamples, hardware permitting. The shared
Float output remains single-sample and GPU-owned. All native/script paths use the
same context options. There is no user-visible mode, environment override, CPU
fallback, parallel renderer or legacy policy. Backend source now sets this option;
the cost executable already exercised the identical option before its probe
variable was renamed and explanatory comments added.

This supersedes the earlier restored-default checkpoint. Wider paints/text/image
pixel qualification, installed worker rebuild/packaging, resource accounting,
representative hardware/scene cost and artist edge review remain open. Eight
samples improves the measured coverage but does not prove exact subdivision
raster invariance or close the rendering slice. Previously captured default-policy
artifacts remain historical evidence, not current-policy pixel acceptance.

## 2026-10-12: semantic animated subdivision through the real MCP route

Added a subdivision variant to the retained native anchor/handle MCP workflow.
After creating typed coordinate tracks and publicly sampling them, the actual
CLI/MCP subprocess inspects the animated object and proposes `split_segment`
with contour/start IDs, fraction 0.35 and time 1/2 second. The candidate contains
three native points; the inserted point has position and both handle tracks.
The source point/track/key identities survive subdivision and artist Undo.
Source midpoint position stays [120,100]; its outgoing offset scales to [21,3.5].

Preparation leaves committed pixels unchanged. Candidate/Apply display pixels
match exactly, including the retry of Apply. Save/reopen preserves exact project
state. First Undo restores the prior two-point animated curve and original
coordinate tracks; second Undo restores the exact original static canvas.
All measured frames report zero CPU adapter nodes and zero evaluation readback
bytes before explicit observation.

The focused native test passed in 20.65 s with approved local Unix-socket/Vulkan
access. Affected application builds took 24.57 s and 30.24 s: the second corrected
the inherited changed-pixel assertion so subdivision no longer requires a raster
difference as a passing condition. Existing YUV and unused Mixed-variant warnings
remain. No full workspace build was run.

Curated `resources/motion-context/mcp-subdivision-evidence.json` records frame
hashes, transfers and identity/persistence checks. Before/after display frames
have 90 differing pixels of 230400, maximum RGBA8 component difference 75.
This is protocol/candidate/Apply qualification, not exact original/subdivided
raster acceptance. Updated the Fold public workflow with the semantic command
and explicit linear-timing/raster limitations. Native artist insertion and
export trajectory parity remain open.

## 2026-10-12: exact Hold-timed animated subdivision

Extended the single native subdivision operation to uniformly Hold-timed
coordinate tracks as well as uniformly Linear tracks. The union of source key
times partitions Hold inputs into constant poses, so the derived inserted point
and handles remain exact piecewise-constant tracks. Derived first and subsequent
keys use the source timing class; endpoint handle tracks still retain their own
IDs, key metadata and timing. Source key schedules can differ. Final unused key
interpolation and one-key tracks do not impose an interval mode.

Mixed Linear/Hold intervals, HoldNext and Bézier timing reject before mutation.
They must not be silently converted or sampled into a frozen pose. The semantic
agent command and artist Pen tool both call this same operation; no protocol
alias, bake path or alternate animation representation was added.

Five focused tools checks passed (1.81 s build, 0.04 s execution). The existing
trajectory check now also samples Hold sources with distinct schedules at 161
rational times and 101 curve positions, covering exact key boundaries and nearby
poses plus endpoint holds. The mixed Linear/Hold rejection remains atomic.
Whitespace review passed. Public skill wording now distinguishes unsupported
timing from all non-Linear timing; its actual MCP evidence still covers Linear
tracks only. Real MCP/native-pixel Hold subdivision and artist acceptance remain
unqualified; Bézier/mixed timing support and export trajectories remain open.

## 2026-10-12: compatible mixed Linear/Hold intervals in subdivision

Replaced the whole-timeline timing restriction with interval-level analysis on
the union of source key times. Each derived interval keeps the timing of its
moving source coordinates. Endpoint-held or constant coordinates contribute no
timing constraint. A Hold source changing at the interval's right boundary
requires Hold; its unchanged interior contributes a constant. Therefore a curve
can hold in one interval and move linearly in the next without baking or losing
its trajectory. Derived keys retain the corresponding interval modes.

Moving Linear and Hold sources within the same interval still reject: one native
vector key interval cannot represent both continuous motion and an endpoint jump
exactly. HoldNext and Bézier remain unsupported. Source interval lookup uses
binary partition over validated sorted keys and compares authored endpoint values;
it does not repeatedly validate/sample tracks merely to classify timing. This
avoids adding dense sampling cost or a second representation to Pen insertion.

Five focused tools checks passed (final build 1.16 s, execution 0.06 s). The
trajectory test now includes synchronized Hold-to-Linear source tracks, retaining
its independent geometry comparisons at 161 rational times and 101 curve
positions. Different-schedule uniform Hold and Linear cases still pass, while
conflicting mixed intervals reject atomically. Whitespace review passed.
This extends provider/artist-tool semantics; actual MCP/native-render mixed timing,
Bézier support, export and native artist interaction remain open.

## 2026-10-12: stream sampled path coordinates without cloning editor records

Native path rendering no longer creates a sampled Contour and cloned Point records
just to write SVG path commands. It samples each point's position and handle
offsets through the existing shared evaluators into a small stack geometry value,
retains only first/previous values, and streams the same line/cubic/closing commands.
This removes the per-contour sampled point-vector allocation and metadata cloning
from the render loop. Document identities and extension data stay in the immutable
source model; they are not needed in rendering scratch. Editor/capture models and
the canonical animation representation remain unchanged.

One affected optimized native example build passed (9.85 s), then the approved
Vulkan five-pose fixture passed its static-pose and warm assertions. All ten raw
Float RGBA frames (five original animated and five subdivided) match the prior
native eight-sample output byte for byte. Curated
`resources/motion-context/path-streaming-evidence.json` records device UUID and
frame hashes. This also confirms the new sampling route retains the measured
subdivision edge limitation rather than concealing it.

Whitespace review passed. No new test framework or broad build was added, and no
frame-rate gain is claimed from structural allocation removal. Closed-contour
command handling uses the same first/last sampled coordinates, but wider closed
path pixels and native artist review were not separately qualified by this open
cubic fixture. Broader performance, resources, export and cutover gates remain open.

## 2026-10-12: persistent native point target for precise artist controls

Canvas anchor/handle picking now retains a local editor target consisting of the
owning object ID and native point ID, separate from object selection and gesture
state. The target survives native revisions while the point remains in the
selected path, so agent edits and renaming do not detach the artist's target.
Object/document selection changes, missing snapshots and removed points/objects
clear it. Clicking object artwork clears the point target; clicking an anchor or
its handle chooses the same native point. No project selection payload, stored
point model or protocol alias was added.

Selection guides outline the targeted anchor with a second square, distinct from
the first-anchor circle and existing handle guides. This is the prerequisite for
showing compact numeric controls for one selected anchor, rather than listing
all points in the inspector. Those numeric inspector controls are not yet added.

Affected UI/GPU test-target compilation passed (1.96 s). A focused editor-state
check passed (12.38 s build), covering a stable point across native rename/revision,
object deletion, Undo restoration and object deselection without a revision
change. Whitespace review passed; existing YUV warnings remain.

Native click/drag visual review at normal and narrow sizes remains open. The
Fold UI skill requires "Inspect the affected UI at normal and narrow panel sizes";
source/fixture checks do not satisfy it. Available native-window inspection still
does not support that review. The next implementation step is numeric anchor and
handle controls tied to this stable target and the existing guarded point edits.

## 2026-10-12: compact animated numeric controls for the selected path point

Added one contextual Anchor inspector section for the persistent native point
target. Position, Incoming offset and Outgoing offset each use the shared
AnimatedProperty X/Y widget, canvas units and existing Auto Key/keyframe menus.
The section does not list every path point or add a toolbar mode. A short scope
note identifies path-local coordinates and relative handle offsets. Labels and
UI scopes use the point's stable ID and fixed role.

Component curves project the existing typed vector track. New one-axis keying
creates a coupled vector key with the other component's authored value. Existing
edits use the shared graph curve replacement service, preserving track/key IDs
and extension data. Removing animation on a component clears the coupled vector
track and captures both evaluated coordinates at the explicit time. The owning
object replacement remains guarded by the shared native edit service, including
unknown metadata and protection. No second point/animation model was added.

The inspector still uses the existing preview/commit/cancel lifecycle and one
Undo transaction per gesture. Coordinate conversion errors cancel the draft
rather than committing other partial property changes; Escape cancellation is
combined with that error state. Auto Key operates on the same model as dragging
and agent edits.

Affected UI/GPU test-target compilation passed (0.67 s). The focused numeric
conversion seam passed (5.35 s build), covering single-axis track creation,
unchanged other-component values, existing track/key metadata and identity,
current-pose capture on clearing, and static numeric edits. Whitespace review
passed; existing YUV warnings remain. These checks do not prove native widget
click/drag, cancel/Undo interaction or normal/narrow visual layout. The Fold UI
skill's required native review remains open, along with wider export and artist
acceptance gates.

## 2026-10-12: numeric handle edits through editor preview, cancel and Undo

Added a focused transaction check around the numeric point conversion and actual
Motion editor State/registered native package. It creates an outgoing coordinate
track at the host's exact 1/2-second time, previews, cancels, then previews twice
and commits through the normal extension route. Committed artwork remains exact
original state throughout preparation. Cancel removes the preview and restores
the original editor canvas. Repeated numeric updates retain track/key identity.
The commit preserves the selected native point, creates one Undo step, and Redo
restores the exact committed document and generated identities.

The final focused run passed (5.08 s build, 0.01 s execution). This adds real
editor/provider/project transaction evidence to the previous pure conversion
check. Whitespace review passed; existing YUV warnings remain. It does not drive
native ImGui mouse/keyboard widgets or qualify visible normal/narrow layout.
Native numeric gesture/animation-menu interaction and artist review remain open;
all five replacement slices remain open.

## 2026-10-12: Pen completion targets the created anchor for numeric editing

Subdivision now retains the generated point ID and selects that native point
after the path-object selection updates. Finishing a newly drawn Pen path targets
its last anchor through the same local point selection. The inspector can therefore
show precise position/handle controls immediately, without another canvas click
or a list of every point. This is editor selection only; it adds no document
mutation or Undo entry. Existing snapshot/selection validation clears a point
removed by Undo or agent edits.

Affected UI/GPU test-target compilation passed (0.69 s). Extended the existing
headless ImGui Pen input fixture, which actually sends the Pen shortcut and mouse
events: subdivision creates/selects the inserted native anchor, Undo clears that
removed target, and Redo restores exact document state. The focused run passed
(6.20 s build, 0.76 s execution). This adds widget-input fixture evidence, not
native-window pixel/layout acceptance. Whitespace review passed; existing YUV
warnings remain. Newly drawn multi-anchor target selection and normal/narrow
native inspector review remain unverified artist gates.

## 2026-10-12: guarded anchor removal and canvas Delete routing

Added native `remove_point` with owning object, inspected expected object and
native point ID. The shared tool removes exactly that anchor and its owned tracks,
then applies a guarded object replacement. Other points, contours, object paint,
transform, extensions and identity remain unchanged. Removing the final anchor
retains an empty contour/path object rather than deleting the object or inventing
replacement geometry. Undo restores the removed point and its owned data.

Canvas Delete invokes this semantic edit only for a persistent point target,
with focused non-text input, current editable image, no active gesture/edit and
no pending work. Successful removal clears that point target. Protection/stale
errors cancel the preview and remain visible. Artists and agents therefore use
one removal operation rather than independent deletion paths.

Affected UI/GPU test-target compilation passed (1.93 s). A focused editor/provider
check passed (8.18 s build), covering final-anchor removal, retained object roots
and paint, stale/protected rejection, selected-target clearing and exact one-step
Undo restoration. The existing numeric preview/cancel/Undo seam also passed after
the enum change. Whitespace review passed; existing YUV warnings remain.
This does not yet qualify native Delete key input, multi-anchor/closed-path removal
pixels or the real MCP removal route. Public skill authoring examples await that
protocol qualification; native artist acceptance and broader rewrite gates remain open.

## 2026-10-12: Delete key input removes the targeted anchor with one Undo

Extended the existing headless ImGui Pen fixture with actual Delete key press and
release events after selecting the inserted native point. The viewport routes the
key through `remove_point`, retains the same path object and both remaining endpoint
IDs, removes exactly the inserted anchor, and clears its target. One Undo restores
the exact three-point document, including the inserted point and subdivision
geometry. This tests widget input and the registered editor/provider/project route,
not merely calling the helper.

The focused run passed (10.06 s build, 0.76 s execution); whitespace review passed.
Existing YUV warnings remain. The native-window visual/focus acceptance gate is
still open, as are real MCP removal, protected/text-input keyboard cases and
closed/animated multi-anchor removal pixels. No broad build or new test framework
was added; all five replacement slices remain open.

## 2026-10-12: contextual anchor removal on open and closed paths

Anchor right-click targeting now covers open and closed contours and selects the
native point for the inspector. The existing compact menu includes Remove anchor,
using the same guarded semantic edit as Delete. Make first anchor is enabled only
for a closed contour's non-first anchor. Menu targets validate the current document,
object selection, protection and continued point membership before enabling actions;
an agent edit or selection change cannot leave an actionable removed target.
The menu adds no second deletion implementation or project data.

Final affected UI/GPU test-target compilation passed (0.68 s); whitespace review
passed. Existing YUV warnings remain. No new test framework or broad build was
added for this low-impact contextual UI change. Underlying removal and first-anchor
transactions retain their prior focused evidence. Native popup click/focus and
normal/narrow layout remain unverified under the Fold UI review gate; real MCP
removal and broader artist acceptance remain open.

## 2026-10-12: real MCP animated anchor removal, pixels and Undo

Added a Removal variant to the retained native anchor/handle MCP fixture. After
creating and sampling coordinate tracks, the actual CLI/MCP subprocess inspects
the path and proposes `remove_point` for its animated first anchor. The path and
contour remain; the other point record and paint remain exact original values.
The removed point's coordinate tracks disappear with their owning point, not
through a second animation cleanup path.

Preparation leaves committed pixels unchanged. Candidate/Apply display pixels
match exactly; the retained single anchor has no visible stroke in the qualified
shared-viewer frame. All three observed frames report zero CPU adapter nodes and
zero evaluation readback bytes before explicit observation. Save/reopen preserves
exact project state. First artist Undo restores the native anchor ID and midpoint
position/outgoing pose [120,100]/[60,10]; second Undo restores the exact original
static canvas.

One affected GPU application build passed (31.01 s), then the approved local
Unix-socket/Vulkan run passed (20.31 s). Existing YUV and unused Mixed-variant
warnings remain. Curated `resources/motion-context/mcp-removal-evidence.json`
records frame hashes, transfers and persistence/removal checks. Updated the Fold
public workflow with guarded removal and its explicit geometry/ownership scope.
Whitespace review passed. Native artist popup/focus acceptance, closed-path and
final-anchor MCP removal, exported trajectories and wider rewrite gates remain open.

## 2026-10-12: focused handle guides without hidden hit targets

Selected paths still display their outline, anchors and first-anchor marker, but
incoming/outgoing handle stems and circles now appear only for the targeted
native point. Select an anchor to expose its handles and numeric controls; moving
to another anchor reveals that point's construction. This removes the all-point
handle tangle without a new toolbar mode, project setting or second selection
model. Viewport handle picking uses the same point-target condition, so invisible
handles cannot intercept a click. Anchor picking remains available across the
selected paths.

Affected UI/GPU test-target compilation passed (0.66 s), and whitespace review
passed. Existing YUV warnings remain. No new tests or broad build were added for
this reversible guide/picking condition. Document geometry, animation evaluation
and native artwork rendering are unchanged. Normal/narrow native visual review
and the select-anchor-then-handle interaction remain unverified under the Fold UI
acceptance gate; they are not inferred from compilation.

## 2026-10-12: leave anchor focus with Escape

In the focused canvas, Escape now clears the point target when there is no
active gesture, edit or pending transaction. Object selection remains, so the
artist can leave the anchor inspector and handle guides without losing the
path. During an active edit, the existing cancellation flow retains the point
target. Text-input focus continues to suppress this canvas shortcut.

Affected UI/GPU compilation passed (0.51 s); existing YUV warnings remain.
No broad build or new test was added for this reversible selection change.
Native shortcut interaction and normal/narrow visual review remain open under
the Fold UI skill's requirement to inspect and exercise the affected UI.

## 2026-10-12: coordinate-only path guides

Canvas guide drawing now samples native anchor/handle coordinates directly.
It no longer creates a sampled contour or clones point/contour extension data
for each selected path on each UI frame. Stable IDs still identify the targeted
anchor, and the same native coordinate evaluators supply guide geometry.
This changes UI construction only; artwork remains in fframes/Skia.

Affected UI/GPU compilation passed (0.46 s), with existing YUV warnings.
Whitespace review passed. No frame-speed claim: latency and native visual
parity have not been measured for this change. Native normal/narrow review
remains open.

## 2026-10-12: discoverable native point animation rules

The Motion authoring catalog now explains native point animation ownership,
path-local position, relative handles, replacement Vec2 units/space, coupled X/Y
key times and explicit static-pose retention when clearing tracks. These rules
are returned with the existing document/edit schema; no second curve protocol
or animation document was added. The public author route already edits native
tracks through guarded replacement, as qualified by the retained MCP fixture.
The shared component-curve service remains an artist/provider seam, not a
separately qualified MCP method.

This is schema guidance only. Native curve-widget interaction and exported
trajectory parity remain open. No new tests or application build were added.

## 2026-10-12: closed-contour animated rendering evidence

Extended the retained native anchor/handle renderer fixture with `--closed`.
The animated two-anchor contour now exercises both the edited segment and its
closing cubic through upstream SVG/Skia. At exact times 0, 1/2, 1, 2 and 0,
animated Float RGBA matches independently specified static poses exactly.
Repeated warm renders, return-to-first-time output and the fresh-renderer
subdivision check also pass. This directly exercises closure after the native
coordinate-streaming change; it does not qualify native UI guides.

One focused example build/run passed (build 1m18s, dependencies rebuilt).
Command: `cargo run --manifest-path crates/motion-fframes/Cargo.toml --example
anchor_animation --offline -- /tmp/fold-closed-anchor-evidence --closed`.
Curated `resources/motion-context/closed-anchor-render-evidence.json` records
hardware UUID, exact-pose checks, diagnostics and raw-frame hashes.
Subdivision still changes 17/12/9/23 Float pixels at the four unique poses,
with maximum component differences 0.088379/0.261475/0.173096/0.176270.
No raster-invariance acceptance is claimed. Full application, export and native
artist review remain open. Existing YUV warnings remain.

## 2026-10-12: exact HoldNext animated subdivision

Native animated subdivision now supports compatible HoldNext intervals alongside
Linear and Hold. HoldNext retains the exact value at the current key and uses
the next value between keys. A source whose jump already occurred before a
union interval is constant there; it does not falsely conflict with another
moving source. Distinct moving modes within one interval still reject rather
than bake or change animation. Bézier timing remains unsupported.

Extended the existing trajectory fixture with distinct-schedule HoldNext tracks.
The focused test passed (2.26 s build, 0.07 s execution): it compares original
and subdivided cubic geometry over 161 rational times and 101 positions, as
well as existing Linear/Hold, identity, serialization and Undo cases. No broad
build was run. Whitespace review passed. Actual MCP HoldNext, native artist
interaction and exported/raster trajectory acceptance remain open.

## 2026-10-12: semantic native point moves

Added typed `move_point` authoring intent with the inspected object guard,
stable point ID, anchor/incoming/outgoing target, canvas-space position, exact
time, explicit linked-handle choice and Auto Key choice. It delegates to the
same native helper used by canvas gestures and applies the existing guarded
point replacement. No separate agent geometry or animation evaluator exists.
The existing authoring catalog exposes this command through its generated edit
schema. New key IDs are created only when the shared helper requires a new key;
existing track/key identities and extension guards remain native behavior.

Extended the retained anchor-edit fixture with JSON command roundtrip, exact
artist-helper parity at an existing key and stale-object rejection. The focused
test passed (0.56 s final build). During iteration corrected a missing test type
qualification and an invalid comparison of independently generated new key IDs;
no implementation fallback was introduced. Whitespace review passed. Actual
MCP command invocation, installed schema and native interaction remain open.

## 2026-10-12: canvas gestures use semantic point moves

Canvas anchor and handle drags now submit the same guarded `MovePoint` command
exposed by the native authoring schema. The gesture keeps its original inspected
object as the expected value, supplies canvas-space pointer position and exact
editing time, and carries the existing Alt/unlinked and Auto Key choices.
Preview/cancel/commit continues through the shared transaction service; no
independent canvas mutation path remains for these gestures.

The retained headless ImGui pointer fixture passed (46.25 s affected build,
0.80 s execution). It exercises actual canvas mouse input, unchanged committed
state during preview, Escape cancellation, final point/handle data, stable point
ID, one Undo and Redo. Existing YUV warnings remain. Whitespace review passed.
This is not native OS/window artist acceptance: normal/narrow visual review and
real MCP `move_point` invocation remain open.

## 2026-10-12: real MCP semantic anchor and handle moves

Reused the retained animated-anchor MCP fixture to submit two `move_point`
commands at the existing 1-second key. Absolute canvas targets [330,200] and
[470,280] on the sheared path produce local anchor [280,200] and outgoing
[120,80]. The second guard includes the first edit's result. Actual subprocess
proposal/inspection/Apply, exact retries, unchanged committed preparation
pixels, candidate/Apply pixel equality, save/reopen, retained point/track/key IDs
and both Undo steps pass. Curated evidence is in
`resources/motion-context/mcp-move-point-evidence.json`; the public Fold workflow
now includes the qualified semantic move example and coordinate/key rules.

Affected build passed (42.55 s), but an edit to fixture canvas coordinates during
that build was not captured in its executable; its run rejected the second
expected object. Rebuilt after final source was fixed (29.72 s), then the focused
local-socket/Vulkan run passed (5.79 s). No production guard was weakened.
Existing YUV and unused Mixed-variant warnings remain. Whitespace review passed.
Incoming, linked-handle and new-key MCP moves, installed packaging, native artist
review and exported trajectories remain open.

## 2026-10-12: compiler output outside installed sources

Application worker configuration now places its default Cargo target under
`cache/target`, alongside the existing user-owned Motion compilation cache,
instead of writing into the fframes bridge source directory. Explicit
`FOLD_MOTION_TARGET` remains an intentional configuration override. Read-only
installed source trees no longer determine the default build-output location.
Affected app/GPU library compilation passed (9.12 s); existing YUV warnings
remain. No package build or new test was added for the path configuration.

Packaging inspection also confirmed a larger open gap: the default bridge is
still derived from the build-time repository root, and `package_fold.py` does
not include the Motion compiler SDK. A complete installed solution must bundle
and resolve the pinned source/dependency inputs and compiler/runtime tools,
then qualify relocated cold and warm compilation and GPU rendering without
checkout access. This path fix alone does not establish installed operation.
Whitespace review passed.

## 2026-10-12: graph evaluator participates in worker identity

Dependency inspection found that motion-canvas depends on fold-graph for typed
track sampling, but the retained worker source identity omitted graph sources.
Added its manifest and source files to the existing explicit dependency-input
list. Changes to shared track evaluation now invalidate compiled Motion cache
identity rather than reusing a binary with stale animation semantics.

Extended the existing transport fixture: changing graph source changes identity,
and restoring bytes restores identity. The focused retained-runtime test passed
(4.15 s build, 1.21 s execution), including existing control reuse, stale frame
rejection and blocked-pipe cancellation. No new test infrastructure or broad
build was added. Whitespace review passed. This qualifies cache input identity,
not installed SDK packaging or new upstream/GPU pixel behavior.

## 2026-10-12: compiler packaging and warm-frame input audit

Inspected the retained worker request and compilation paths. Warm requests with
unchanged generated program identity reuse the live worker; full bridge-source
and cached-binary digest checks occur when compilation/preparation is requested,
not for every frame on that retained path. No source-hash removal is warranted
by this inspection. Program generation and document validation still execute
on requests; their measured cost must guide further optimization.

The production bridge's local dependency closure is motion-fframes,
motion-canvas, gpu-sharing, graph, animation, foundation and project. Their
workspace manifests require the root workspace declarations. Generated script
programs also depend directly on the exact pinned upstream fframes Git revision.
The current compiler copies the bridge lock and invokes Cargo offline, so a
relocated SDK must provide dependency resolution inputs and native Skia build
resources as well as Rust sources. Copying only the bridge directory or worker
binary cannot establish editable-script support.

Next packaging change must provide an explicit installed SDK location and a
complete locked dependency bundle, with writable output in the user cache.
Acceptance requires a relocated cold compile, control-only warm reuse and
native rendering without checkout access. This is source inspection evidence,
not runtime timing or installed acceptance; no build was run for this audit.

## 2026-10-12: explicit relocatable installed SDK location

Application assembly now supports a package-build setting,
`FOLD_MOTION_SDK_RELATIVE`, resolved relative to the running executable's
parent directory. A packaged build can select
`../share/fold/motion-sdk/crates/motion-fframes` without embedding the checkout
path. The existing explicit `FOLD_MOTION_BRIDGE` override takes precedence.
Development builds retain their source location when no package setting is
provided. Installed configuration does not probe for a checkout or silently
switch locations when its SDK is missing. Build identity reruns when the package
setting changes; writable compiler output remains in the user cache.

Affected app/GPU compilation passed. No installed SDK or vendor bundle has yet
been created, and the packaging script does not yet enable this setting.
Relocated cold compilation and rendering remain required. No package build or
new test was added for this configuration step. Whitespace review passed.

## 2026-10-12: staged production Motion SDK sources

Added `scripts/stage_motion_sdk.py` to stage the seven audited production crates
and their root workspace declarations into a new SDK directory. It excludes
editor/test-only dependencies, keeps the pinned bridge lock, resolves the
production Linux graph offline, then records SHA-256 hashes of every staged
input. This separates compiler sources from the checkout without copying
retired engines, editor packages, tests or build output. It refuses an existing
destination rather than overwriting another SDK.

Executed staging in `/tmp/fold-motion-sdk-source-resolved-20261012` without a
compile. Full offline Linux Cargo metadata resolved all seven local packages
inside the staged SDK; no local dependency points back to the checkout. The
53 recorded source hashes match the resolved files. The first staging probe
revealed that production dependency pruning updates the copied bridge lock;
staging now resolves before hashing so its manifest describes final inputs.
Whitespace review passed. Upstream dependencies were resolved from the existing
Cargo cache, not a bundled vendor directory. Toolchain/native Skia resources,
package wiring and relocated cold compilation/rendering remain open.

## 2026-10-12: vendored SDK dependencies and relocation resolution

SDK staging now supports `--vendor`: Cargo vendors the locked registry and pinned
fframes Git sources, and staging writes relative source replacement configuration
under `.cargo/config.toml` before hashing final inputs. Runtime compilation
explicitly supplies this SDK configuration when present, regardless of the
application's working directory. Its bytes participate in worker source identity.
No alternate renderer or source-language path was introduced.

Offline vendor initially found missing encode_unicode and other locked cached
packages. An approved dependency download completed the vendor input set.
The first `--vendor` probe caught quiet Cargo suppressing its configuration;
removed quiet mode and reject empty output before recording success.

Affected Motion library compilation passed (2.98 s). Moved the completed SDK to
`/tmp/fold-motion-sdk-relocated-20261012`, then resolved its graph offline and
locked with an empty Cargo home and the explicit configuration. All 276 resolved
package manifests reside inside that relocated SDK; seven are local Fold crates.
Curated evidence is `resources/motion-context/sdk-dependency-evidence.json`.
No compilation/package build was run for dependency qualification. Native Skia
build resources, bundled compiler tools, package wiring, cold compilation and
native rendering remain open. Whitespace review passed.

## 2026-10-12: staged local native compiler archives

Inspected pinned upstream build scripts: Skia and ffmpeg-sys-fframes both accept
local `file://` binary archive templates. SDK staging now accepts explicit
`--skia` and `--ffmpeg` archives together with `--vendor`, copies their keyed
filenames under `native/skia` and `native/ffmpeg`, and hashes them in its input
manifest. It rejects missing/misnamed archives and incomplete native pairs.
Worker compilation uses local SDK archive templates when its native bundle is
present. The SDK input manifest also participates in worker identity.

Staged `/tmp/fold-motion-sdk-native-inputs-20261012` from the retained release
Skia archive and pinned FFmpeg 9.0.0 cache archive. Both input digests match the
staging manifest and both gzip/tar archives are readable; curated sizes/hashes
are in `resources/motion-context/sdk-native-input-evidence.json`. Affected Motion
compilation passed (0.37 s), and whitespace review passed. No cold compiler or
package build was run. This does not prove archive feature-key compatibility,
system linker/Clang prerequisites or installed rendering. Bundled Rust tools,
package wiring and relocated cold/native execution remain open.

## 2026-10-12: relocated bundled Rust tools

SDK staging accepts an explicit Rust sysroot through `--toolchain`, copies Cargo
and rustc plus their libraries/standard library, and retains toolchain licenses
and copyright notices. Input hashing now streams files to avoid loading large
compiler libraries into memory. Packaged application configuration selects
`SDK/toolchain/bin/{cargo,rustc}` by default; explicit tool overrides still take
precedence. Development configuration retains its existing command lookup.

Staged and relocated the toolchain probe to
`/tmp/fold-motion-sdk-toolchain-relocated-20261012`. Its own Cargo reports 1.95.0;
its own rustc reports the relocated SDK sysroot. That rustc compiled and linked
a small standard-library program, which executed successfully. Affected app/GPU
compilation passed (2.21 s), with existing YUV warnings; whitespace review passed.
The link still used the system C linker. This is toolchain relocation evidence,
not full Motion cold compilation, compiler/linker independence or GPU rendering.
The separate source/vendor/native/toolchain probes have not yet been assembled
and qualified as an installed application package.

## 2026-10-12: Linux package includes and selects the Motion SDK

The Linux package builder now requires explicit Motion Skia/FFmpeg archives and
a compiler sysroot. Before building it checks that those inputs exist, then
stages production sources, locked vendored dependencies, native archives and
compiler tools at `share/fold/motion-sdk`. Desktop/CLI release builds receive the
executable-relative SDK location. The package file manifest includes SDK input
hashes, source manifest and notices. It no longer produces a desktop package
without Motion compiler inputs or leaves its default worker location pointing
at the checkout.

Python compilation, command help and whitespace review passed. No full package
build was run for wiring. The component staging/relocation probes are separate
from installed application acceptance. Complete combined SDK cold compilation,
local native archive feature compatibility, system linker/Clang prerequisites,
worker rendering and package relocation still require qualification. An existing
package directory is still rejected; no compatibility packaging path was added.

## 2026-10-12: combined SDK cold build and scripted Vulkan pixels

Assembled production sources, locked vendor inputs, local native archives and
Rust tools at `/tmp/fold-motion-sdk-combined-20261012`. Built a standalone
fframes script callback against that SDK with its bundled Cargo/rustc, an empty
Cargo home, a new target directory and offline dependency resolution. The
3m15s cold build passed. Skia's build output records its local SDK archive URL;
FFmpeg reports prebuilt binaries, with a separate empty native archive cache.
The system C linker and installed Clang remain prerequisites of this probe.

The resulting binary rendered a moving circle through the real retained
fframes/Skia Vulkan renderer at exact times 0, 1/2 and 0. All frames have visible
pixels; time changes pixels; repeated warm draws match exactly; return to zero
matches the first Float RGBA frame exactly. Approved native Vulkan execution
passed. Curated hashes/scope are in
`resources/motion-context/sdk-cold-script-evidence.json`; build output is
`/tmp/fold-sdk-cold-build.log`, and probe inputs are `/tmp/fold-sdk-cold-probe`.

This is standalone SDK compilation/render evidence. It does not qualify the
application's generated worker protocol, candidate authoring, exposed-control
updates, installed package relocation or artist UI. Complete package build and
those integration gates remain open. No workspace suite or repeated cold build
was run. Whitespace review passed.

## 2026-10-12: SDK-scoped writable native archive cache

Packaged native compilation now sets the FFmpeg archive cache under the user's
Motion cache, partitioned by bridge/SDK source identity. It no longer consults
an unrelated default FFmpeg cache when a native SDK bundle is configured.
Archive URL templates still resolve inside the selected immutable SDK; cache
writes remain outside installed source directories. SDK staging now explicitly
records system C/C++ linker/development-library and Clang/libclang prerequisites,
which were used by the cold probe and are not supplied by copying Rust tools.

Affected Motion compilation passed (0.45 s), Python compilation and whitespace
review passed. No repeat cold build was run for this path change. Runtime native
cache placement is implemented; installed package and generated-worker control
qualification remain open. The 3m15s cold dependency build is setup cost, not
script update or warm-frame latency, and does not establish acceptable first-use
artist performance.

## 2026-10-12: direct compiler experiment with prebuilt SDK libraries

Tested compiling the same script probe directly with bundled rustc and the
already-built dependency libraries, without running Cargo or rebuilding those
libraries. Upstream SVG macros require `CARGO_MANIFEST_DIR`; the first probe
failed clearly when it was absent. Supplying the existing generated manifest
directory made compilation pass in 1.144 s. Native Vulkan Float RGBA at all three
poses matches the Cargo-built probe exactly.

Copied the 286 rlib/proc-macro libraries to a new directory and compiled again
successfully (7.762 s including that observation's cold file access). A further
probe hid the original dependency directory while compiling from the relocated
libraries; it passed in 1.978 s. Its native Vulkan frames also match exactly.
The debug library set occupies 1,602,675,540 bytes; release size and general
script-update performance remain unmeasured. Curated evidence is
`resources/motion-context/sdk-direct-compiler-evidence.json`.

Production compilation is unchanged in this experiment. Next compiler phase
should build dependency libraries once in the SDK producer and replace runtime
Cargo compilation with one direct-rustc path, rather than ship both compiler
paths or defer a three-minute dependency setup to the artist. That change must
verify pinned library/compiler identity, macro/media inputs, release sizing,
generated-worker controls and package relocation, then remove superseded runtime
Cargo configuration and cache behavior in the same phase. These focused probe
times are not a general latency or installed-experience acceptance claim.
Whitespace review passed; no additional dependency rebuild or workspace suite
was run.

## 2026-10-12: precompiled SDK library producer and input manifest

Added SDK `--precompile` production staging. With complete vendor/native/compiler
inputs, it builds release dependency libraries once using bundled tools and a
private temporary target/cache, then installs the rlib/proc-macro set under
`libraries`. Its manifest records exact rustc version, profile, core source/lock/
configuration/native-archive input hashes, library hashes and the two required
public externs. Missing or ambiguous primary libraries fail staging. Temporary
build output is removed after successful library installation.

Qualified the manifest/install step against the existing combined SDK's debug
artifacts, without repeating a dependency build: 286 libraries and 56 source
inputs recorded. Rechecked all source input hashes and both primary library
hashes. Python compilation and whitespace review passed. Curated evidence is
`resources/motion-context/sdk-library-manifest-evidence.json`.

Release precompilation, final release size, runtime manifest verification and
direct-rustc replacement remain open. Package staging does not yet request
precompilation because production runtime still uses Cargo; the next phase must
replace that runtime path and enable the producer together, not retain two
runtime compiler implementations. No fresh release build or workspace suite was
run for this producer step.

## 2026-10-12: single direct compiler runtime cutover

Replaced runtime Cargo builds with direct `rustc` compilation against the SDK's
precompiled dependency libraries. Removed Cargo and target-directory fields from
runtime configuration, app assembly and qualification examples; removed the
`FOLD_MOTION_TARGET` setting. There is no Cargo fallback. Generated Cargo.toml is
retained only as metadata required by the upstream SVG procedural macro.

Initialization checks the compiler version, SDK input/library digests, required
externs and library inventory. Cache identity includes the library manifest.
Compilation still has cancellation/deadline control, atomic binary publication,
source-change guards and cached executable digest validation. Packaging now
requests precompiled SDK libraries. Example arguments are scene/evidence directory,
SDK root and Rust compiler executable.

Checks: motion-context all-target check and app check passed; retained-worker
control/stale-frame/cancellation fixture passed; packaging scripts compile as
Python. Generated-worker preparation against the combined staged SDK passed. The
resulting executable ran on native Vulkan: t=0 and t=1/2 have different pixels,
and returning to t=0 reproduces identical Float RGBA bytes. Evidence:
`resources/motion-context/sdk-runtime-direct-worker-evidence.json`. This uses
prebuilt debug SDK libraries and standalone readback; full release SDK/package
production, installed-app execution and application GPU-sharing parity remain open. Previous cold-build/direct-link evidence remains
experiment evidence, not installed application acceptance.

SDK hashing now checks cancellation and its verification deadline within each
64 KiB read, including large dependency archives. Debug-build verification was
CPU-bound for about two minutes before script compilation; this is an observed
startup cost, not accepted performance. Release startup cost still needs a
measurement. No active probe remains after the generated worker check.

## 2026-10-12: SDK-only application defaults and release producer qualification

Development assembly now selects `FOLD_MOTION_CACHE/sdk/crates/motion-fframes`
(or the normal user cache's equivalent) instead of repository source crates.
Installed assembly selects its package-relative SDK. Both select the SDK's
bundled rustc by default. `FOLD_MOTION_BRIDGE`, `FOLD_MOTION_CACHE` and `RUSTC`
remain explicit overrides. An unstaged SDK fails; no source/Cargo fallback exists.

The producer requires an actual Rust sysroot, including redistribution notices,
and validates notices before creating the destination. Passing a previously
staged bundle failed this qualification because its notice layout is different;
the producer was restarted with the real sysroot. Release precompilation
subsequently passed; its log is `/tmp/fold-motion-sdk-release-qualified-stage.log`.
No full package acceptance is claimed. Relocation/runtime evidence follows below.

The retained-worker fixture now also checks manifest rejection for compiler ABI
mismatch, changed source inputs, unlisted dependency libraries and an expired
verification deadline; the focused test passed. SDK metadata now distinguishes
precompiled runtime link prerequisites from source-build Cargo/Clang prerequisites.

For development, stage a complete SDK with `scripts/stage_motion_sdk.py` using
`--vendor --precompile --skia ARCHIVE --ffmpeg ARCHIVE --toolchain SYSROOT`.
Choose a new absolute destination and set `FOLD_MOTION_BRIDGE` to its
`crates/motion-fframes` directory, or stage at the default user-cache SDK location.
Cargo and Clang belong to production staging; authored scripts use direct rustc.

Release SDK production completed successfully: dependency compilation took 2m30s;
348 dependency libraries and a complete 19,533-file staged inventory were emitted.
The producer removed its temporary target/build directory. The SDK was moved to
`/tmp/fold-motion-sdk-release-relocated-20261012` before runtime qualification;
the original staging path no longer exists. Generated-worker preparation
against this relocated release bundle passed. This is SDK qualification,
not full application package or artist acceptance.

The relocated release-SDK worker ran on native Vulkan with exact Float RGBA
hashes matching the prior debug-SDK worker at t=0, t=1/2 and t=0. Returning to
zero is byte-identical; advancing time changes pixels. The executable has launch
permissions and requires neither the removed producer target nor its old SDK
path. Evidence: `resources/motion-context/sdk-release-relocation-evidence.json`.

Preparation with the debug driver took 95.716 seconds, including whole-SDK input
and library verification; three standalone worker frames took 511.413ms including
process/renderer startup. These are scoped measurements, not warm application FPS
or optimized runtime startup acceptance. All probe processes for this checkpoint
have finished. Full installed application/package qualification remains open.

## 2026-10-12: optimized runtime SDK startup measurement

Added separate SDK verification and program-preparation timings to the existing
preparation example. Built that example with the production optimized Runtime
implementation; the focused build passed in 55.50s. Measured against the relocated
release SDK, with one fresh program cache then one cached program invocation:

| Case | SDK verification | Program preparation | Total process |
| --- | ---: | ---: | ---: |
| Empty program cache | 6126.207ms | 6363.600ms | 12491.336ms |
| Cached program | 5988.079ms | 424.372ms | 6413.918ms |

Evidence: `resources/motion-context/sdk-optimized-startup-evidence.json`.
The filesystem cache may be warm. These are single scoped observations, not
cold-disk statistics or installed-app startup acceptance. SDK verification is
retained per Runtime in preview and conversion services; ordinary frames do not
repeat these timings. However, first renderer initialization still pays the
verification cost, and native-only documents currently compile an empty script
worker too. Startup improvement remains required. A prebuilt native-only worker
using the same worker/render implementation is a candidate for the next focused
phase; it must not introduce an alternate renderer or a Cargo fallback.

All probes for this measurement completed. Full package/installed-app checks,
artist review, vector raster quality, broader 3D tools and final removal remain open.

## 2026-10-12: prebuilt native-only worker

Native-only documents now select the SDK's prebuilt `fold-motion-native-worker`.
Script documents retain the single direct-rustc authoring path. The native entry
point implements the same Scripts trait and calls the same `worker::run` and
fframes renderer; it explicitly rejects script dispatch. There is no alternate
render implementation or Cargo fallback. Runtime selection shares validation,
worker identity/retention, cancellation, source guards and transport behavior.

SDK manifest version 2 requires and hashes the native executable. Version 1 is
not accepted. SDK production compiles this small native entry against its verified
precompiled libraries, with direct rustc, then records its digest. Native worker
selection checks the executable digest again before launching.

Affected-crate all-target and app checks passed. The retained-worker fixture now
runs the native path with a compiler that permits only its version query; it
passed control updates, stale-frame rejection, invalid pixels, cancellation,
restart and source-change guards. Actual producer library-install qualification
reused the previously checked release dependency libraries; full cold dependency
and package builds were not repeated for unchanged library code.

Actual native-scene preparation with a version-only compiler selected the SDK
worker and created no program cache. Optimized preparation: 7230.356ms SDK
verification, 471.516ms worker selection, 7703.370ms total. Native Vulkan frames
match the generated script worker exactly for three artist-transform poses;
returning to the first pose is byte-identical. Evidence:
`resources/motion-context/sdk-native-worker-evidence.json`.

SDK input/library verification remains the main startup cost. It still verifies
script dependency libraries for native-only scenes; separating verification at
the native/script resource boundary is the next focused performance step.
Script compilation against the version-2 SDK passed: 6432.849ms verification
and 6299.924ms script preparation. The output is a generated program in the
script cache, not the native executable. All probes in this checkpoint finished.
Full installed-app and native GPU-sharing acceptance remain open.

## 2026-10-12: native-first SDK integrity verification

SDK initialization now verifies source inputs and the prebuilt native worker.
It validates manifest ABI, paths, extern declarations and directory inventory,
but reads script-library contents only before the first script compilation or
cached script use. Successful script verification is retained per Runtime;
a failure does not publish a verified state. Verification stays cancellable and
uses the remaining compiler deadline. No library is loaded into a script compiler
before its digest passes; native-only artwork does not consume these libraries.

The retained-worker fixture passed with deferred-library tampering/recovery checks:
changed script library bytes do not prevent native initialization, but script
verification rejects them; restoring the declared bytes permits verification.
Affected-crate all-target and app checks passed. Rebuilt the optimized preparation
example once for the changed runtime (15.74s), without repeating SDK production.

| Optimized case | Initial SDK verification | Program preparation | Total |
| --- | ---: | ---: | ---: |
| Native-only | 849.122ms | 411.787ms | 1262.590ms |
| Cached script, first script verification | 844.866ms | 5962.869ms | 6809.309ms |

Native-only preparation continues to pass with a compiler that rejects every
compilation call and creates no program cache. The previous native-only total
was 7703.370ms; the new observation is 1262.590ms. This is a scoped comparison
with potentially warm filesystem caches, not installed-app or cold-disk latency
acceptance. Evidence: `resources/motion-context/sdk-native-first-verification-evidence.json`.
Pixel rendering code and the prebuilt worker are unchanged from the preceding
exact Vulkan pixel comparison. First script use retains its integrity-check cost;
ordinary frames reuse the initialized worker. All measurement processes finished.
Full package/artist, vector-quality and 3D acceptance remain open.

## 2026-10-12: installed-layout CLI Motion qualification in progress

Building the real optimized `fold-cli` with spatial/GPU capabilities and
`FOLD_MOTION_SDK_RELATIVE=../share/fold/motion-sdk/crates/motion-fframes`.
Build log: `/tmp/fold-motion-installed-cli-build.log`. Staged the version-2 Motion
SDK by moving it into `/tmp/fold-motion-installed-cli-20261012/share/fold/motion-sdk`;
the previous SDK path no longer exists. Copied the independently existing color
runtime/config into the package; native color dependency resolution points inside
that package. No native color/video builds were repeated for this Motion check.

Prepared current schema-4 native and script project archives with explicit ACEScg
project settings at `/tmp/fold-installed-motion-artwork-20261012`. Once the CLI
build finishes, move the package and render these archives from unrelated CWD,
without Motion SDK/compiler/color-root overrides. This is a focused Motion
installed-layout check, not full package producer, desktop UI, video-helper or
artist acceptance. No launch/render acceptance is claimed yet.

The CLI optimized spatial/GPU build passed in 4m47s, with existing unused-code
warnings for `gpu/yuv.rs` Layout.format and Layout.parameters when native-video
is not enabled. Copied the built binary and moved the complete staged directory
to `/tmp/fold-motion-installed-cli-relocated-20261012`; its old package and SDK
paths are absent. Actual native Vulkan renders passed with Motion SDK/compiler
and color-root overrides removed, CWD `/tmp`, explicit GPU selection and only a
writable Motion cache override.

The first manually prepared test archive lacked its document's project-browser
entry and was correctly rejected as InvalidOrganization. Corrected that fixture's
organization data, without changing application loading rules, and reran successfully.

| Installed CLI render | Total process time | Result |
| --- | ---: | --- |
| Native ellipse, 32x32, t=0 | 14328.874ms | Nonblank image, 216 nonblack pixels |
| Script, 640x360, t=0 | 14713.911ms | New program cache and image |
| Script, t=1/2 | 7780.289ms | Different image |
| Script, t=0 again | 10072.755ms | Exact original PPM hash |

Evidence: `resources/motion-context/installed-cli-motion-evidence.json`.
This proves actual app assembly, package-relative SDK/compiler resolution,
project loading, native/shared Motion GPU rendering and installed ACES color
routing at this seam. Individual runs start new CLI/renderer processes and include
color/GPU startup; they are not warm frame timings. End-to-end startup remains
slower than the isolated preparation tool and needs attribution/optimization.

No full package producer/native-video build or desktop launch was performed in
this check. Normal/narrow UI artist acceptance, broader vector quality, full 3D
workflows, remaining delivery/color cases and final deletion audit remain open.
All installed-layout probe processes have completed.

## 2026-10-12: resume — hierarchy copies and independent 3D resources

Added native Duplicate, Make geometry unique and Make material unique actions to
3D hierarchy context menus. Duplicate copies a complete subtree with fresh object,
parameter, track and key identities. It preserves animation values, protection and
extension data, and shares geometry/material resources. The public `insert_tree`
edit publishes the complete hierarchy atomically; it rejects existing or external
children. Final ownership validation rejects disconnected trees and duplicate owners.
Resource separation changes only the selected instance and retains source resource
extensions. It works when another instance protects the shared resource. Both
artist actions use the existing preview/commit, revision guard and Undo path.

Checks: `cargo check -p fold-three-d --features ui --offline` passed (5.44s).
The focused editing suite passed (4 tests, 0.02s after a 24.88s build). The new
provider-authoring seam covers hierarchy copies, independent animated identities,
shared resources, protected peers, resource separation, invalid tree rejection and
Undo. The source now has the expected Compositor operator count of 24; the render
preparation loop also no longer has the earlier unconditional break. Those older
checkpoint failures must not be reported as current failures without a fresh check.

The product architecture document referenced by the UI skill is absent. Applied
the skill's rules and this plan's product contract. Native visual review is still
open. Orca screen capture reports unsupported on this Linux provider. The user
also reports that left-click drag does not work in computer use; do not use that
path as interaction evidence. Existing ImGui input tests remain separate evidence.

## 2026-10-12: declared script images and retained resource reuse

Added typed Image script controls. `Context::image(name)` returns the declared
project asset's SVG href. Native images and script controls share one artist image
selector, source qualification, sorted asset declarations, immutable snapshot
resolution, relinking and resource transport. Control edits preserve IDs and do
not change compiled-program identity. Missing or forged asset declarations reject.
Image capture remains unsupported and fails explicitly; SDR/ICC limits remain.

Removed repeated byte scans of unchanged worker image/font storage. The worker
retains fonts through Arc identity; image conversion checks retained Arc handles
before scanning new pixel data. Every demand still checks its current allocation
allowance. New content retains complete validation and cache invalidation.

Checks: affected UI/context check passed (2.65s); host authoring test passed
(6.55s build, <0.01s test) for resource declarations, sampling, stale/protected
controls, unchanged compiler identity, artist publication and Undo. The worker
check passed (11.64s). The optimized existing image example built (18.60s), then
ran on native Vulkan. Native and scripted image pixels match exactly across
retention, replacement and malformed-snapshot recovery. Lowering a retained
snapshot's allowance rejects; removing resources releases references. Existing
color/alpha sample checks passed on device
`330fe04bf2d13725a29d0a23ee9e7147`. This is a 32x32 image seam, not broad HDR,
installed SDK or artist visual acceptance. The installed SDK must be rebuilt to
include the changed canvas/worker source before qualifying the complete app.

## 2026-10-12: 3D viewer selection and world translation

Added camera-space picking against the renderer's evaluated triangles, including
hierarchy transforms, perspective/orthographic projection, visibility, zero-alpha
materials and camera clipping. Added labeled X/Y/Z translation handles and a
camera-plane center handle to the shared viewer. Numeric transforms remain in
the inspector. Movement converts the world delta through the sampled parent
inverse, then uses the inspector's shared binding edit policy. Existing tracks,
auto-key, protection, revision/time/selection changes, cancellation and one-step
Undo use the normal artist transaction path. No Bevy state enters the document.

Checks: affected UI check passed. One geometric seam test passed (7.30s build,
0.01s test) for nearest/hidden picks in both projections, axis and plane motion,
nonuniform rotated parenting, auto-key and protected parameters. The existing
native ImGui workflow passed after adding viewer clicks and movement at 640px
and 260px widths (5.95s build, 0.30s test). It selects the object, previews a move
without mutating the committed project, cancels with Escape, commits on release
and restores the original document with one Undo. Hierarchy duplication and
resource separation also pass in this artist state fixture. These checks do not
replace native visual review. Rotation/scale gizmos and native artist acceptance
remain open; translation is the implemented direct-manipulation scope here.


## 2026-10-12: native image-pattern paint and resumed checkpoint

Implemented native image-pattern fill and stroke resources. Patterns use the
same upstream SVG/Skia renderer as other Motion paint. The document stores an
asset ID, tile origin, tile size and invertible affine transform. Asset discovery
includes patterns and script image controls. The inspector has one paint-type
choice, an image selector and tile size controls. Transform controls are folded
under an expander. There is no new toolbar row.

Verification:

- `cargo check -p fold-motion-context --features ui --offline` passed.
- The focused declared-image authoring test passed. It covers asset discovery,
  invalid pattern dimensions and existing control guards/Undo.
- The optimized `motion-fframes` `images` example passed on native Vulkan. It
  checked repeated tiles, shifted origin, transparent areas, unchanged retained
  pixels, replacement, failed-resource recovery and script/native image parity.
- The SDK producer rebuilt `target/motion-sdk-resume` with pinned native archives,
  vendored dependencies, the bundled compiler and precompiled release libraries.
  This is SDK staging evidence, not complete installed-package acceptance.
- `cargo check --workspace --all-targets --features fold-app/desktop --offline`
  passed in 59.18 seconds before the animation-review API addition.
- Boundary and proposal-ownership checks passed. Their 4 and 9 focused policy
  tests passed. The boundary list now permits the independently valid shared
  `fold-graph` Track/Binding dependency of `motion-canvas`.

Limits: arbitrary SVG-pattern conversion is not implemented. Script image/pattern
capture still rejects unsupported content explicitly. Native normal/narrow visual
acceptance remains open. The Linux computer provider reports no screenshot
capability; drag is also unavailable according to the user. Existing native ImGui
input checks are seam evidence only.


## 2026-10-12: public contact sheets and onion review

Implemented optional `observe_frame.review` for committed and candidate targets.
The primary exact time is first, followed by 1–15 supplied exact times. One
immutable snapshot and retained renderer serve the entire job. Contact sheets
use ordered tiles. Onion review averages opaque display pixels equally; it is
not a linear/HDR export operation. Each pose records provenance, transfer counts
and the SHA-256 of its RGBA bytes. Existing job cancellation and admission apply.
Input/output pixel limits and a working-memory reservation bound review storage.

Verification:

- Both observation wire tests passed, including oversized, empty and invalid
  contact-sheet requests. The desktop-feature test build passed in 8m31s.
- Native Vulkan MCP `mcp_anchor_animation_candidates_keep_native_identity_pixels_and_undo`
  passed in 122.53s with the rebuilt SDK. It checked every tile byte against the
  individual frame and every onion byte against the two source poses. Candidate
  review left committed pixels unchanged. Apply parity, save/reopen, stable path
  identity, parameter sampling and Undo passed in the same workflow.
- Real review pixels were decoded to PNG and visually inspected. A two-pose curve
  appears in the contact sheet and both poses appear in the onion image.
  Evidence: `resources/motion-context/animation-review-evidence.json`; images:
  `docs/Motion_Replacement_Report_assets/{sheet,onion}.png`. These are rendered
  review images, not desktop screenshots or artist acceptance.
- Workspace/all-target desktop Clippy completed in 42.06s with warnings. Strict
  `-D warnings` stopped on the pre-existing tuple return type in
  `foundation/src/affine.rs:96`. Existing warnings span shared and feature crates.
  The new review test's rounding expression and image-authoring test's cloned
  slices were simplified after this run. No broad warning cleanup was applied.
- The 3D viewer handles now clip to the viewer canvas. Workspace Clippy checked
  this final UI source. Its transaction/input behavior was checked earlier.

The Fold skill now includes the verified public review request. All five slices
remain open. This closes the missing review operation at the tested seam; it does
not close native artist acceptance, performance, full packaging or other feature
gaps in the current-status table.


## 2026-10-12: relocated SDK script and conversion check

Moved the newly staged SDK from `target/motion-sdk-resume` to
`target/motion-sdk-relocated`. The original staging path no longer exists.
With `FOLD_MOTION_BRIDGE` set to the relocated bridge and a new compilation cache,
`mcp_capture_prepares_editable_shapes_without_changing_pixels_or_committed_state`
passed on native Vulkan in 235.20s. It exercised actual script compilation,
editable capture, candidate/committed pixel parity, independent native paint
editing, save/reopen and Undo. The application used the bundled SDK compiler and
precompiled dependencies; runtime Cargo was not restored.

This qualifies the relocated SDK in the tested desktop-feature MCP host. The
color runtime was supplied by the existing qualified color package. It does not
qualify a new full package from `scripts/package_fold.py`. Current SDK path:
`target/motion-sdk-relocated`.


## 2026-10-12: 3D rotation and scale gizmos

Implemented Move, Rotate and Scale in the existing single-row 3D toolbar.
Rotation uses local-axis rings. Scale uses local-axis handles and a uniform
center handle. Numeric inspector controls remain available. All three tools
preview through the same guarded edit state, cancel with Escape and commit one
Undo entry. Protection and Auto Key use the shared parameter-binding logic.
Local ray-plane intersections preserve parent affine transforms, including
nonuniform scale and shear. A plane through the camera is rejected so an
edge-on ring cannot intercept another handle due to raster rounding.

Verification: the focused `fold-three-d --features ui --test interaction --lib`
run passed 3 unit tests and 2 interaction tests (23.58s build; 0.42s test time).
Native ImGui pointer input checked selection and move/rotate/scale preview,
Cancel, commit and Undo at 640px and 260px window widths. The geometry seam
checked parented local rotation/scale, exact-time key insertion, unchanged
starting pose, stable parameter IDs, protection and invalid-scale rejection.
This is input/geometry evidence. Native visual and artist acceptance remain open.
Animated world-preserving parenting remains open.


Final source check for this checkpoint:
`cargo check --workspace --all-targets --features fold-app/desktop --offline`
passed in 7.22s after the gizmo changes. `git diff --check` passed. The HTML
report's relative asset/evidence links resolve. The full test suite has not been
repeated at this intermediate checkpoint; earlier focused and native results
above are retained without expanding their acceptance scope.


Feature-matrix correction: focused `fold-three-d --features ui` Clippy exposed
`clippy::never_loop` in `render/src/gpu/preparation.rs:99` when native-video is
disabled. The desktop checkpoint enabled native-video and did not expose it.
Moved the unsupported-native-backend check before the decode loop, and compiled
the native loop only with its owning feature. No fallback decoder was added.
Focused GPU-only Clippy passed in 10.47s with existing warnings. The native-video
and spatial render check passed in 11.25s. This supersedes the handoff's unresolved
`never_loop` note for both checked feature combinations.

The workspace/all-target desktop check was repeated after the decode feature-gate
fix and passed in 19.85s. No full test suite was repeated. Existing work remains
uncommitted and preserved. The replacement goal remains active.


### 2026-10-12 — declared image conversion through public MCP

Conversion now resolves project image assets through the same resource loader,
color checks and retained worker snapshots as rendering. It preserves the asset
ID for centered Contain/Cover/Stretch images and full-tile image-pattern paint.
Capture rejects undeclared resources, missing bindings and unsupported pattern
effects. This supersedes earlier image/pattern capture rejection entries.
Authored controls remain unchanged; Undo restores the script.

Evidence:
- Optimized `motion-fframes` image example: exact native/captured pixels for all
  three fits, transformed images, shifted pattern origin and worker conversion.
  Missing and undeclared capture resources reject. Results:
  `resources/motion-context/image-capture-evidence.json`.
- Native public MCP capture workflow passed in 234.66s. Original, candidate and
  committed pixels match; captured image/pattern asset IDs survive. Gradient
  editing, candidate isolation, Apply, save/reopen and Undo passed in that test.
  Results: `resources/motion-context/mcp-image-capture-evidence.json`.
  Real renderer output: `docs/Motion_Replacement_Report_assets/image-capture.png`.
- Fresh SDK staged at `target/motion-sdk-image-capture` with bundled compiler,
  vendor inputs and precompiled libraries. An earlier stage to
  `target/motion-sdk-images` timed out in vendor while another build ran; that
  partial directory is not qualified. The successful SDK is not a full package.
- Two retained runtime tests and one resource-binding test passed. Affected-crate
  all-target UI check, default-feature check (3.16s), isolated bridge examples
  check and workspace/all-target desktop check (5.13s) passed.

Limits: arbitrary SVG patterns, noncentered image alignment and unsupported image
interpolation/effects still reject. Broader color/HDR and native artist review
remain open. All five slices remain open.


### 2026-10-12 — animated world-preserving 3D parenting

`move_world` now preserves the complete animated world transform. The document
stores a bounded, flat list of frozen parent-transform factors before the existing
local channels. At exact time t, new compensation is:

`inverse(new_parent_at_edit(t)) * old_parent_at_edit(t) * old_compensation(t)`.

Each frozen factor retains native track interpolation. Affine multiplication
preserves shear. No frame sampling/baking, live old-parent reference or alternate
renderer is introduced. The original position/rotation/scale parameters, track
IDs, key IDs, extensions and content remain unchanged. Later new-parent edits
contribute relative to the motion captured when parenting. Reparenting again
prepends compensation; at most 512 factors are allowed per object. Identity
literal factors are omitted. Frozen factors cannot be rewritten or discarded by
whole-object edits. Copies receive new frozen track/key IDs as well as new
editable channel IDs. The shared world-movement tool includes compensation when
converting pointer movement to authored local coordinates.

The existing parent menu uses this operation. The inspector shows “Parent motion
preserved” when compensation exists, with an explanation of the local channels.
No extra toolbar controls were added. Native visual review of this status remains
open; automated interaction is not artist acceptance.

Validation:
- `cargo check -p fold-three-d --all-targets --features ui --offline` passed.
- `cargo test -p fold-three-d --features ui --offline --lib --test editing
  --test interaction`: 9 passed; build 34.39s, tests 1.34s. This includes normal/
  narrow viewer input and guarded authoring. Existing render YUV warnings remain.
- Parenting compares 137 fractional-time samples with animated source/destination
  parents, Bezier position tracks, quaternion rotation and nonuniform scale.
  Rendering and viewer matrices agree. It also checks repeated reparenting,
  compensated ancestors, independent duplicate identities, later parent edits,
  world movement, protected/cyclic rejection, document serialization and Undo.
- The first combined test run caught an unnecessary identity compensation in an
  existing UI equality check. Literal identity factors are now omitted; rerun
  passed. No broader workspace rebuild was repeated for this isolated change.
- `git diff --check` passed. Reviewed new factor ownership, bounded evaluation,
  extension guards and reuse of the shared transform evaluator.

This supersedes the earlier static-only parenting restriction. Broader replacement
and native acceptance remain open; all five slices remain open.


### 2026-10-12 — complete producer, relocated package and live desktop

Ran the real `scripts/package_fold.py` producer with the existing pinned native
build cache at `/home/rgame/Documents/fold-phase-16/native`. It built OCIO/config,
the SDK and native worker, LGPL video runtime/helper, desktop and CLI. SDK release
dependency build: 2m28s; helper: 4.56s; application release build: 1m33s. This was
one complete producer run, not a manually assembled package. Log:
`/tmp/fold-motion-package-producer.log`.

Moved the package from `target/motion-installed-20261012` to
`target/motion-installed-relocated-20261012`; the original path is absent.
Verified all 19,582 declared file hashes and the producer's source manifest.
`ldd` resolves all three executables; the helper's FFmpeg libraries resolve inside
the relocated package. CLI tests ran from `/tmp` with no Motion SDK/compiler,
color-root or video-helper overrides. Explicit GPU selection, native-video backend
selection and a writable cache were the only runtime settings used by the checks.

| Installed CLI case, 640x360 | Total process time | Result |
| --- | ---: | --- |
| Native ellipse | 13.798s | Nonblank, 5,824 nonblack pixels |
| Fresh script compilation | 15.456s | Exact native-ellipse pixels |
| Script at 1/2 second | 7.632s | Motion changes pixels |
| Script at zero again | 8.223s | Exact repeat |
| Independent 3D box | 1.198s | Nonblank, 9,216 nonblack pixels |
| Timeline native-video source | 0.970s | Strict cuda-native decode succeeds |

These include process, color, GPU, compiler/worker and output startup. They are
not warm frame measurements. Motion startup is still materially slower than the
3D/video cases and remains a performance investigation target.
Evidence: `resources/motion-context/full-package-evidence.json`; fixtures and
outputs: `target/motion-package-fixtures`. Old demo artwork contains retired
document types and was preserved, not used as a current-format fixture.

Launched the relocated desktop from `/tmp` with the current script project. It
reports GTX 1070/Vulkan and shared GPU rendering. The installed CLI MCP adapter
connected to the actual desktop socket, discovered Motion schema 4, and obtained
a rendered frame. Desktop/CLI pixels match exactly when both use the explicit
sRGB display transform. Initial default comparison differed as expected: viewer
uses sRGB, delivery defaults to Rec.1886 Rec.709. No color behavior was changed to
force the comparison. Live observation took 93.54ms, including protocol/hex image
transfer; this is not renderer-only frame speed. It reported zero CPU adapter
nodes and zero evaluation readback bytes. Evidence:
`resources/motion-context/installed-desktop-evidence.json`; actual frame:
`docs/Motion_Replacement_Report_assets/installed-desktop-frame.png`.
This frame is renderer output, not a desktop screenshot. The desktop remains open
for native visual review; user feedback was requested without stopping code work.

Current package-boundary and proposal-ownership checks passed, as did 13 policy
tests. Source search found retired Motion IDs only in deliberate non-execution
and opaque-storage tests. This is a targeted removal check; the full checkpoint
suite/audit and remaining feature/color/performance/artist acceptance still apply.

### 2026-10-12 — Native worker startup and user visual review

The user answered “Looks correct” to the installed Motion workspace review at
normal and narrow panel widths. This closes that visual-layout gate for the
installed build. It does not qualify 3D interaction, all artist workflows or
performance. The desktop remains open; no screenshot was available.

Fixed GPU runtime startup to select the bundled native worker when the document
has no scripts. It previously entered script compilation for native documents.
The GPU path now uses the same executable selection as the CPU worker path.
Scripts still use the compiled-program path. No fallback was added.

Evidence: `resources/motion-context/native-gpu-no-compiler-evidence.json`.
With compilation disabled, 38 GPU frames passed with one native worker, no
program cache and one image snapshot. Memory release and stale-frame rejection
passed. First render was 584.804ms after runtime setup.

Application evidence: `resources/motion-context/application-frame-profile.json`.
The release profiler measured 40 native frames at 640×360. First evaluation
fell from 7042.498ms to 1542.990ms; warm median was 15.962ms. All measured
frames had zero CPU pixel upload/readback. The unchanged 10,000-dot script
baseline was 42.482ms median and 45.123ms p95. These include graph evaluation
and display output, but are not full UI FPS. The affected GPU example and
application profiler built successfully; touched-crate formatting passed.

The installed package predates this startup fix and needs a final refresh.
An additional analytic color check remains unqualified: encoded sRGB
[0.2,0.6,1] produced linear center [0.0313721,0.317627,1], with maximum
difference 0.001733 from the analytic conversion. This finding does not change
the successful worker-selection result. Investigate it before color acceptance.

### 2026-10-12 — Precise GPU color and signed HDR qualification

The native worker's analytic color regression failed before the fix and passed
after it. A 96×32 upstream SVG fixture isolated the same error in rectangles,
ellipses and paths. CPU rendering and direct GPU clears were accurate; direct
GPU paints lost precision. Disabling internal MSAA did not change the result.
Restored the existing 8-sample policy; no diagnostic renderer remains in the app.

The pinned Skia m153-0.101.2 source chooses byte vertex colors for any RGB in
0..1, even after conversion to linear space. Added the exact-value byte-storage
check in `resources/motion-context/skia-color-precision.patch`, used only for
Ganesh vertex-color decisions. The existing range predicate remains unchanged.
This retains Skia's own float vertex path rather than adding another rasterizer.
The fframes pin and document model remain unchanged.

`scripts/build_motion_skia.py` verifies the source archive, applies the patch,
checks reusable source, builds the native library and emits the standard archive
with patch/source/library hashes. SDK staging verifies those hashes and rejects
unpatched input. The builder passed using the existing native build output.
Patched input passed validation; the original archive was rejected.

Evidence: `resources/motion-context/color-precision-evidence.json`. All three
GPU shape samples now exactly match the CPU RGBA16F reference:
[0.0330810546875, 0.318359375, 1, 1]. Maximum analytic error fell from
0.001733 to 0.000188, within the test's unchanged 0.00025 bound. The original
640×360 native-worker regression also passed. A real fframes shader preserved
[-0.25, 2, 0.125, 0.5] exactly through the linear premultiplied float target.
This qualifies signed/HDR transport at the direct renderer seam, not installed
application/export parity or HDR display output. Native SVG paint remains encoded
sRGB in 0..1; this check does not add HDR native paint or high-bit-depth images.

One native dependency build took 8m10s. The final focused example/native-worker
build took 6.61s; both GPU checks completed in 0.43s. Removed the temporary
direct-paint diagnostic code. Formatting, Python syntax and whitespace checks
passed. The new archive is in `target/motion-skia-precision`; see
`resources/motion-context/Skia_Precision_Build.md` for repeatable commands.
The installed package still predates this fix and the native startup fix.

### 2026-10-12 — Mixed application signed/HDR export

The focused native test `mixed_motion_and_3d_float_exr_matches_linear_preview`
passed in 35.63s using the newly packaged precision SDK. It sends Motion and 3D
through Compositor Over and Timeline, then compares Float RGBA EXR output with
linear preview at exact times 15/30 and 16/30 seconds. Both frames retain 2,280
negative-red pixels and 2,256 green pixels above 1.9. All 230,400 pixels per frame
match exactly in premultiplied output. Straight output has exact unpremultiplied
RGB, unchanged alpha and zero RGB where alpha is zero. Animated frames differ.
Preview uses no CPU adapter nodes or readback before the explicit test review.

Evidence: `resources/motion-context/mixed-hdr-export-evidence.json`. The test
uses a real fframes shader with premultiplied [-0.25, 2, 0.125, 0.5], plus animated
SVG content. This closes signed/HDR parity for this mixed Float EXR workflow.
Native HDR paints, high-bit-depth image input, HDR display and other delivery
formats are not qualified by this test. The focused test build took 5.51s.

The complete package producer also passed with the startup fix and patched Skia
archive (SDK build 2m24s; app binaries 25.18s). It was moved to
`target/motion-installed-precision-relocated-20261012`. Installed hash/render
verification is in progress; creation alone is not installed acceptance.

### 2026-10-12 — Relocated precision package and 64-copy 3D scene

The new package passed all 19,582 declared file hashes after relocation. All
three executable dependency checks passed; the video helper uses packaged native
video libraries. Installed CLI native, script, half-second script, repeated
script, 3D and native-video renders passed. Native and script pixels match
exactly; repeated script pixels match; changing script time changes pixels.
Evidence: `resources/motion-context/installed-precision-evidence.json`. Build
activity overlapped these process checks, so their timings are not a clean
frame-speed benchmark. The only source change after packaging is the 3D
qualification example's project-save line. Production source hashes match.

Extended `crates/three-d/examples/render_scene.rs` with a bounded copy count.
It uses the actual duplicate/edit path, retaining independent object/animation
IDs and one shared geometry/material. A 64-copy scene rendered at 0, 1/2 and
1 second on the NVIDIA GeForce GTX 1070. All frames are finite and nonempty;
13,329 pixels change between the first and last frame. There are no CPU raster
adapters or readbacks before the explicit qualification readback. Build: 4m54s
with the GPU/UI dependency set. Evaluation plus qualification readback/file work
took 525.01, 21.45 and 18.45ms at 320×180. Three frames do not qualify sustained
performance. Evidence: `resources/motion-context/three-d-clones-evidence.json`.

The example saves `target/motion-three-d-clones/scene.fold` for native review.
The report includes the actual half-second render with an sRGB display transform
and checkerboard behind transparency. This is renderer output, not a screenshot
or native artist acceptance.

The rebuilt desktop launched with the 64-copy project on the shared Vulkan
backend. Protocol socket: `/tmp/fold-precision-review/fold-session-urlH9s/protocol.sock`.
The original review desktop was left intact. Requested native 3D normal/narrow
layout, selection, numeric position editing and Undo review without requiring
drag input. Feedback is pending. Targeted Rust formatting and diff whitespace
checks passed after these qualification changes.

### 2026-10-12 — Retained GPU cache allowance

Added a generic external-resource allowance to the shared GPU host. Motion
reserves one eighth of the host budget, capped at 64 MiB, before starting a GPU
worker. The allowance appears in host scratch accounting and remains owned by
the worker until process retirement and I/O-thread join. Each private frame
request carries a separate cache allowance; target-image accounting is unchanged.
Skia receives that limit and the renderer checks its budgeted cache usage after
drawing. An over-limit frame cannot be published. The worker releases its
renderer on failure and releases an old context before creating its replacement.

This covers Skia's **budgeted cache**, as confirmed in the pinned source's
`GrDirectContext::getResourceCacheUsage`. It is not total VRAM telemetry or a
hard bound on temporary, unbudgeted or driver/device allocations. Skia can
exceed its soft cache limit while resources are locked; Fold rejects that frame
and releases the renderer. Broader resource accounting remains open.

The affected Motion host all-target check passed. The native worker build
passed. A real worker rendered under 32 MiB, rejected a one-byte cache allowance,
and recovered identical pixels after restoring 32 MiB. Reported successful
cache usage was 2,132,020 bytes. Evidence:
`resources/motion-context/gpu-cache-pressure-evidence.json`. The existing
native/script image fixture also passed with pressure/recovery cases, retaining
image replacement, capture and pixel-parity checks. The blocked/stale worker
retirement test passed in 0.89s and released all host allocations.

The updated SDK producer passed (dependency build 2m52s). The retained runtime
then completed 38 GPU frames with one bundled native worker and compilation
disabled. A 32 MiB cache allowance stayed reserved between frames, independently
of released shared images. After runtime destruction, all host allocations were
zero. Stale intent and oversized-demand rejection still pass. Evidence:
`resources/motion-context/gpu-cache-ownership-evidence.json`. Warm median was
10.878ms for this native fixture; this is not full UI FPS or performance acceptance.
The complete installed precision package predates this cache change.

### 2026-10-12 — Exact aligned Bézier subdivision

Native segment subdivision now supports Bézier coordinate intervals whose
source key times align and whose time-handle fractions match per component.
Spatial De Casteljau is linear in the moving control vectors. Applying that same
operation to temporal handle offsets preserves the whole trajectory. The helper
uses shared curve evaluation to resolve Auto handles, then stores explicit new
handles. Existing source position tracks remain unchanged; retained source
spatial-handle tracks keep their IDs and scale their existing offsets.

Equal-valued endpoints are not treated as constant Bézier curves: their handles
can produce overshoot. Mismatched source spans, incompatible timing fractions
and conflicting moving modes reject before mutation. This does not bake curves
or add a new animation evaluator. Existing Linear/Hold/HoldNext behavior retains
its original empty-handle representation.

The focused trajectory test passed (final build 0.81s, test 0.16s). Manual and
Auto Bézier cases each compare original and subdivided geometry at 161 rational
times and 101 positions with an absolute 1e-8 bound. Source position tracks and
JSON roundtrip match. Incompatible timing rejects without mutation. Earlier
Linear/Hold/HoldNext cases remain in the same test. Evidence:
`resources/motion-context/bezier-subdivision-evidence.json`. Updated public
workflow guidance distinguishes source qualification from the existing Linear
MCP test. Native/MCP/export Bézier qualification and the known raster edge
difference remain open. Installed packages and the cache SDK predate this change.
Formatting and diff whitespace checks passed.

### 2026-10-12 — Current cutover checkpoint and retained-service audit

Started the required serial default-workspace checkpoint after the cache and
Bézier changes: `cargo test --workspace --offline --no-fail-fast -- --test-threads=1`.
Log: `/tmp/fold-motion-current-checkpoint-tests.log`. Completion is pending;
this entry is not a full-suite pass. Current package/headless boundaries and
proposal ownership checks passed, as did all 20 Python policy tests (4.883s).

The current application registers only the new Motion and independent 3D
providers. Neither lockfile contains the retired `fold-motion`, Rhai, Parley,
Skrifa or i_overlay packages. Retired Motion provider IDs remain in deliberate
opaque-storage/non-execution tests. The current contract tests exercise rejection
and byte-preserving save/reopen; their current full-run result is pending.

Retained-service decision: preserve shared vector coverage for the independent
Compositor mask and Roto consumers (`compositor/src/shapes.rs` and
`compositor/src/roto/render.rs`). These are active non-Motion functions. The
replacement policy permits independently valid shared services. They do not
provide a Motion renderer, legacy provider or fallback path. This resolves the
earlier Roto backend decision by retaining its independent service; it does not
claim that every custom vector rasterizer has been removed from the application.

The image-input audit confirms a concrete integration constraint. At the pinned
upstream revision, usvgr `PreloadedImageData` carries premultiplied RGBA8 bytes,
and the Skia renderer's image path consumes that representation. Fold currently
accepts 8-bit sRGB stills without embedded ICC profiles; explicit color/alpha
reassignment and high-bit-depth/HDR image input remain unsupported. Wider support
requires an explicit upstream resource transport change. Silent conversion to
8-bit would not satisfy the precision requirement. Existing signed/HDR shader
and mixed Float EXR evidence does not qualify HDR image input.

### 2026-10-12 — Mid-gray image check during checkpoint

Source inspection suggested that an untagged upstream Skia image might bypass
sRGB conversion. A real native-worker check disproved that hypothesis for an
opaque mid-gray image. Premultiplied RGBA8 [128,128,128,255] rendered to linear
[0.2158203125,0.2158203125,0.2158203125,1], versus analytic sRGB 0.2158605001.
Maximum error is 0.00004019, inside the 0.0005 bound. No renderer change was made.
Evidence: `resources/motion-context/image-midgray-evidence.json`. This narrow
check qualifies that opaque SDR sample, not ICC, high-bit-depth or HDR input.

The full workspace checkpoint is still compiling. A read-only process inspection
first hit an automatic approval-review timeout; the permitted retry succeeded
and confirmed new active compiler processes. The existing Cargo process was not
restarted. No current full-suite result is claimed.

### 2026-10-12 — Checkpoint build concurrency

The default-parallel workspace build was making progress but was strongly
I/O-bound: `/proc/pressure/io` reported full stall time near 73%, with low CPU
contention. Process inspection confirmed active compiler/linker children of
this task's Cargo process. Sent SIGINT only to that verified build process group;
session 44803 exited 130. No source or completed build artifact was removed.
Resumed the same checkpoint with `-j 2` and serial tests. New log:
`/tmp/fold-motion-current-checkpoint-tests-j2.log`. This was a measured resource
adjustment, not a restart after an observation timeout. Results remain pending.

### 2026-10-12 — Current workspace result and stale assertions

The two-job serial workspace run completed with exit 101: 575 passed, two failed,
70 ignored across 132 result groups. All five current-contract tests passed.
Log: `/tmp/fold-motion-current-checkpoint-tests-j2.log`.

The nested composition test still required an opaque Timeline background. That
contradicts the intentional transparent-background change recorded above and
`timeline/src/sequence_video.rs`. Corrected four expected pixel arrays: source
alpha 0.5 at clip level 0.5 gives 0.25; empty gaps give zero alpha. Kept nested
composition, immutable snapshot, save/reopen and Undo assertions intact.
The operation inventory test counted 24 entries while the current registry has
25, including Roto. Corrected the count; its per-operation configuration and
serialization checks remain intact. No production behavior changed here.

Only these two test targets are rerunning, with two build jobs and serial tests.
Log: `/tmp/fold-motion-checkpoint-fixes.log`. The full run is not a clean pass;
the targeted corrections remain pending until that process completes.

### 2026-10-12 — Checkpoint assertion corrections verified

The first focused rerun reached an additional stale assertion comparing opaque
presentation pixels with transparent scene pixels. `output_scene::evaluate`
explicitly composites over black; corrected that expected display result while
retaining transparent scene checks. Both complete affected test groups now pass:
5 composition workflow tests and 11 native authoring tests. Log:
`/tmp/fold-motion-checkpoint-fixes-2.log`. This resolves the two full-run failures;
it does not qualify the 70 ignored tests. No full-suite rerun is needed for these
assertion-only changes. Started the current workspace/all-target desktop check
with two build jobs; log `/tmp/fold-motion-current-checkpoint-check.log`.

### 2026-10-12 — Current desktop build checkpoint

`cargo check --workspace --all-targets --features fold-app/desktop --offline -j 2`
passed in 59.79s. This includes the latest cache allowance and Bézier changes.
Log: `/tmp/fold-motion-current-checkpoint-check.log`. Started the matching
workspace/all-target desktop Clippy command with two jobs. It is not a strict
warnings-as-errors run; completion remains pending.

The current Clippy run completed with exit 0 and warnings. Its log records
style/complexity warnings across the workspace; no warning-free claim is made.
Both desktop checkpoint processes are terminal. The targeted test corrections,
all-target desktop check and normal-policy Clippy are now verified.

### Focused Bézier review — interaction rejected; replacement in progress

The user rejected the release Bézier demo as laggy and glitchy. Earlier layout,
geometry and renderer checks do not constitute acceptance of this interaction.
Goal mode is paused at the user's request; work continues one demo at a time.

Replaced the viewport's embedded point-drag branch with `ui/path_input.rs`, a
separate pointer interaction state. The implementation follows established
vector-editor interaction patterns studied in Graphite's path tool and Paper.js
hit testing; no third-party implementation was copied. Press selects without a
mutation. A three-pixel threshold starts a drag. The initial grab offset is kept,
Shift constrains movement, and Alt frees a smooth handle. Smooth handles preserve
the opposite length; corner handles remain independent. Control interaction does
not wait for rendered-image readiness. Stationary holds do not submit previews.
The previous point-drag branch was removed, not retained as a fallback.

Ten focused Motion UI checks pass (5.55s), including off-center selection, drag
cancellation/Undo, stationary request counts and handle manipulation with pending
artwork. This does not prove artist acceptance or full desktop frame latency.
Release build and opt-in passive native timing capture are the next review step.
Reference sources: https://github.com/GraphiteEditor/Graphite/blob/master/editor/src/messages/tool/tool_messages/path_tool.rs
and https://paperjs.org/reference/path/ .

A further source trace found renderer churn: retiring every superseded preview
cancels `GpuRuntime::render`, whose error path stops the retained native worker.
The preview mailbox now gives same-view, same-pose content edits up to 100ms to
finish the current frame while replacing the pending slot with the newest demand.
Retired consumers are removed, so old pixels cannot publish to the current view.
Seeks, viewport/output changes, removal and expired grace still cancel. Ten
preview-mailbox tests pass, including 18 coalesced edits, one pending demand,
no stale presentation consumers and cancellation after the grace expires.
This proves scheduling behavior, not measured artist latency. The release build
is refreshed once to include this final scheduler change.

## 2026-10-12: path guide snap-back remains open

The user rejected the new release: the guide snaps back both during a held drag
and on release. This is not accepted behavior. The existing path input test was
expanded to draw the shared inspector before the viewer on every ImGui frame;
that focused test passes (0.84s), so it does not reproduce the live failure.
The passive native trace contains 3,388 frames with revision 3 throughout;
controls took 13.58ms median / 16.38ms p95. No successful edit was observed.
A release build with temporary `[DEBUG-snapback]` cancellation diagnostics passed
(25.46s). The current project was saved through the public protocol to
`target/bezier-before-snapback-trace.fold` before restarting the single review
instance. The next native drag must identify the reset path before a fix is claimed.
The broad replacement goal remains paused.

## 2026-10-12: replace canvas point input with ImPlot

The user explicitly requested ImPlot rather than further custom drag work.
The cancellation trace from the rejected release was empty after the reported
reproduction; it did not establish a reset cause. No snap-back fix is claimed
on that evidence. Temporary cancellation diagnostics are removed.

The Motion canvas now uses `dear-implot` 0.18, compatible with the existing
Dear ImGui 0.18. ImPlot owns anchor/handle hit testing, pointer capture and
coordinate conversion. ImPlot line plots draw the selected path and tangent
guides in the same canvas coordinates. The custom path press/drag controller
is removed. Fold retains document evaluation, smooth-handle coupling,
transactional preview, Escape cancellation and one Undo entry per drag.
The fframes artwork renderer and project schema are unchanged.

Verification: all 10 focused Motion UI tests pass (3.85s), including ImPlot
handle input with a stale rendered image, shared inspector/viewer state,
preview coalescing for a stationary pointer, Escape, release, Undo/Redo,
Pen insertion and object movement at two viewer sizes. Test input frames
deliver their events without ImGui queue trickling; production input keeps
its existing queue policy. The optimized desktop build passed (3m 50s, including the first native ImPlot compilation). The final focused UI run also passed all 10 tests (7.87s).
Native alignment, latency and artist acceptance remain unverified.
This entry covers canvas path editing, not the separate keyframe graph.
The broad replacement goal remains paused.

## 2026-10-12: real-session path release fix

The user screenshot shows reset orange guides and a displaced cyan render.
The real Session reproduced the cause: a worker edit completion was rejected
because the gesture's own preview was active. The UI then returned to committed
geometry while rendering retained the preview. The previous synchronous UI
fixture did not exercise this lifecycle.

Release now commits the validated preview when command ID, base revision and
arguments match exactly. Project revision guards and Undo remain authoritative.
`motion_gesture::released_motion_preview_commits_the_displayed_curve_once`
failed before the fix with “Finish the current edit, then retry import or relink”
and passed after it, including preview content identity and Undo/Redo.
The existing compositor preview/cancel/one-Undo integration test also passes.
The optimized desktop build passed in 16.25 seconds.

A release CPU profile of 120 preview submissions in the saved review scene
measured 0.120 ms median, 0.153 ms p95 and 0.245 ms maximum. This excludes
rendering, UI drawing and display latency; it is not native speed acceptance.
Current viewer messages do not resize the canvas. No routine editing banner
was found on the Motion path; no new notification was added.
Native drag/release alignment and responsiveness remain pending user review.
The broad replacement goal remains paused.

## 2026-10-12: native canvas Bézier interaction accepted

After the real-session release fix and optimized desktop relaunch, the user
reported: “okay. it feels good.” This accepts the demonstrated canvas Bézier
interaction. It does not qualify the separate keyframe graph, other native
workflows or larger-scene performance. The broad goal remains paused.
