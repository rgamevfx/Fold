# Phase 23 — Production compositor nodes and multilayer images

Status: in progress. This user-directed phase follows 22; it does not close the
separate delivery and hardening obligations in phases 20–21.

## Goal

Replace the initial compositor stand-ins with a small, flexible, predictable
compositing vocabulary. Use Nuke's image, mask and channel workflows as a
reference while preserving Fold's package boundaries, shared UI SDK, immutable
evaluation, exact time, transactional undo and explicit resource ownership.

## Decisions from the design discussion

- Image connections preserve arbitrary named layers/channels, including beauty,
  depth, CG lighting AOVs, mattes and vectors. Untouched channels pass through.
  Layer names are not a fixed enumeration; color/data interpretation is explicit.
  Input color processing applies to color channels, never indiscriminately to
  depth, vectors or other data. Depth retains floating-point precision.
- Applicable processing nodes have an optional mask input. Artists connect images
  directly and select a channel (alpha by default); an Extract Mask node is not
  required. Disconnected masks mean full coverage. Provide invert and mix.
  Effect masking interpolates between original and processed values; it does not
  replace or multiply output alpha. For Merge the original is B. Matte/alpha
  editing remains an explicit operation. Generators and Read need no effect mask.
- Shared controls are consistent: operation controls first, channel selection,
  optional mask/channel/invert/disable and mix; occasional sampling/bounds/alpha controls
  are advanced. Numeric editing and animation remain available. Drags preview,
  commit one undo entry, and cancel back to their starting state.
- Read's primary controls are a file picker (including image-sequence discovery),
  source frame range, optional offset and input color transform. Preserve exact
  source-frame mapping; offset zero preserves source frame numbers. Advanced
  controls cover missing/out-of-range frames, rate, alpha interpretation and
  color-layer assignment. Nested Fold document references remain editable.
- Shuffle has A and optional B inputs. Source layer → destination layer is the
  common path. Per-channel mappings can read either input or constant zero/one;
  destination names may be new. Unmapped A channels pass through. Shuffle copies
  values without color conversion, normalization or implicit premultiplication.
  Remove/Keep Channels handles pruning independently.
- Image format and signed data bounds are distinct. Transforms/blur retain useful
  overscan until explicitly cropped. Each operation defines filtering, edges,
  channel dependencies and spatial support. Preview/export/headless share math.
- Viewer playback caches only displayed pixels at the requested physical preview
  resolution, using the existing RGBA8/BC7 policy and global budget. Twenty source
  layers do not imply twenty cached playback images. Alpha/depth views cache their
  visualization, not the underlying multilayer frame. Working/intermediate caches
  remain separate, precision-preserving and budgeted.
- Viewer selection drives required-channel evaluation upstream. Include auxiliary
  dependencies (Merge alpha, effect masks, nonlinear-grade alpha). Decode may have
  unavoidable format granularity; disclose it rather than claiming selective I/O.
- Presentation identity includes content/output/time, selected channels,
  visualization (including depth range), display transform, resolution and quality.
  Switching views cannot publish or replay stale images; indicators describe the
  selected view. Reuse/evict prior views under the same shared budget.
- Depth/data visualization is explicit and bypasses beauty color interpretation.
  Export never reads the quantized viewer cache or inherits viewer channel choice.

## Existing nodes and replacements

| Existing | Required replacement |
| --- | --- |
| Read | Simple source controls above; named-channel media ingest |
| Solid | Constant color/alpha/format; preserve old serialized identity |
| Transform | Translate, rotate, scale, pivot; skew/filter/invert advanced; separate opacity; preserve legacy results |
| Crop | Explicit data crop versus format change, signed bounds |
| Blur | Gaussian, continuous linked XY size, selected channels, mask/mix, edges |
| Grade | Black/white points, lift/gain/multiply/offset/gamma; selected channels, alpha-aware math, mask/mix |
| Mask | Rectangle with antialiased coverage; Ellipse and editable Roto |
| ApplyMask | Explicit multiply-alpha semantics, distinct from effect masking |
| Merge | A/B, Over default, compositing operations, mask/mix, channel/bounds policy |
| Output | Published document result; intermediate viewing independent of delivery |

Additional first-toolset work: Reformat; Premult/Unpremult; Extract Mask;
Copy/Set Alpha; Shuffle; Remove/Keep Channels; Erode/Dilate; Exposure; Invert;
Clamp; Dot/Backdrop/Group and published controls; FrameHold/TimeOffset.
Keying, tracking, paint, denoising, optical flow and 3D remain separate designs.

## Implementation boundaries and sequence

1. [x] Introduce validated named-channel and interpretation contracts, explicit
   channel routing/dependency planning, and tests for AOV preservation. Keep
   authored graph types in compositor and neutral execution types in render/media.
2. [ ] Integrate source metadata and multilayer EXR/image-sequence ingest with
   bounded, cancellable worker decoding, source verification and exact frame maps.
3. [x] Implement Shuffle and channel management end to end, with GPU execution and
   CPU reference equivalence. Preserve data values and only evaluate demanded work.
4. [ ] Replace core operators with the semantics above; shared optional masking,
   mix/channel selection, alpha operations, real filtering and signed bounds.
5. [x] Integrate per-viewer layer/channel visualization, settings persistence,
   demand identities, stale-result rejection and displayed-image-only caching.
6. [ ] Build Read and node inspectors, graph entries, direct controls and remaining
   first-toolset nodes through the shared SDK. Inspect normal/narrow layouts.
7. [ ] Migrate old documents/animation paths without silently changing appearance;
   retain unknown extension data and fail unsupported capabilities explicitly.
8. [ ] Qualify native desktop, preview/export/headless, GPU/CPU, resource pressure,
   cache invalidation, undo/cancel and performance. Remove obsolete scaffolding.

## Acceptance and verification

- Synthetic multilayer fixtures contain at least twenty layers including signed/HDR
  color, float depth, vectors and mattes. Shuffle swaps/copies A/B channels and
  constants, preserves other channels, and reports missing channels usefully.
- Masked Grade/Blur/Merge distinguish effect masks from alpha multiplication;
  mask inversion, disabled/disconnected masks, mix endpoints and fractional values
  have numerical reference tests. Identity settings preserve the input.
- Fractional transform/filter, overscan round trips, crop/reformat differences,
  Gaussian edges and nonlinear grading of transparent/HDR pixels have CPU/GPU
  comparisons. No silently degraded preview or CPU upload disguised as GPU work.
- Read sequence range/offset, missing frames, color-vs-data interpretation and
  source replacement exercise transactional undo, cancellation and reopening.
- Beauty/alpha/depth switching and depth-range changes have distinct presentation
  identities. A twenty-layer source consumes the same playback storage as a
  one-layer source at equal output resolution/format/count. Required working
  allocations are measured separately. Cached replay does not decode or grade.
- Independent viewers and background range preparation preserve budgets, fairness,
  in-flight ownership and stale-result rejection. Export ignores viewer settings.
- Run touched-crate checks and focused behavioral tests during iteration. At slice
  checkpoints run workspace checks/tests, Clippy and package boundaries; record
  inherited failures separately. Native UI review covers normal/narrow sizes,
  animation, keyboard access, one-entry gestures and cancel/undo.
- Requalify accepted phase-19 throughput on the reference GPU; measure channel
  demand, decoded data, transfers, passes and working/presentation memory separately.

## Evidence and outstanding work

Baseline inspection: ten compositor operators; fixed RGBA render graph; integer
box blur; RGB-only gain; nearest-neighbor affine sampling; hard rectangular mask;
Over-only Merge. No named-channel EXR decoding or channel-selective viewer exists.
The plan records target behavior, not implementation certification. Checklist items
remain open until their integrated acceptance evidence is recorded here.


### Implemented foundation (2026-10-03)

- Validated named-channel references and color/alpha/data interpretation live in
  render; authored routing stays in compositor. Shuffle aliases channels without
  allocating pixel planes, then packs only requested tuples into execution work.
  Untouched AOVs survive processing and nested composition reads. Neutral named
  image fragments cross the capability boundary without intermediate pixels;
  RGBA-only feature providers retain their existing evaluation adapters. Nested
  channel catalogs are memoized per query and validate dependencies/recursion.
  A/B per-channel mappings and constants are
  simultaneous; the inspector also offers a whole-layer mapping shortcut.
- Flat single-part EXR metadata and selected-channel decoding use verified immutable
  source bytes, cancellation and decoded/working memory reservations. Read source
  replacement runs in an application worker and commits assets plus the node edit
  as one undoable transaction. Only the default rgba RGB layer receives input
  color processing automatically; artists explicitly assign additional color AOVs.
  RGB naming alone does not classify normals/positions as color. Data bypasses
  the input transform. The current adapter rejects deep, multipart and subsampled
  sources explicitly. Compressed blocks can still include unrequested channels.
- New Grade, Merge operations, affine filtering and separable Gaussian Blur have
  shared CPU/GPU semantics. Old serialized variants retain their legacy math.
  Exposure, Invert, Clamp, Premult/Unpremult and antialiased Rectangle/Ellipse are
  available. Common channel selection, image mask/channel, invert and animated mix
  preserve channels outside the selection. Multiply Alpha remains an explicit
  alpha operation. Disabled masks retain their connections and skip evaluation
  of otherwise unused mask branches. A zero mix bypasses the operation during lowering.
- Per-viewer RGB/layer and scalar-channel/range settings persist in the workspace.
  Preview compilation requests the selected channels. Scalar visualization bypasses
  the beauty display transform. Channel selection and range participate in playback
  cache identities and range preparation; export compilation remains independent.
  Playback still stores one RGBA8/BC7 display image under the existing global budget.
- Read exposes a worker file picker, first/last frame, offset, input transform and
  advanced color-layer assignment. New processing properties use the existing
  animation and transactional edit lifecycle. Shuffle/Remove/Keep and common effect
  controls use the shared ImGui SDK.

### Remaining implementation — phase is not complete

- Sequence discovery/decoding, missing/out-of-range frame policies and the complete
  Read advanced controls. Current EXR import is a still-image source.
- Distinct signed data bounds and format throughout CPU/GPU execution; overscan
  preservation; production Crop/Reformat semantics. Current execution uses a fixed
  document raster and clips outside it.
- Roto authoring; Erode/Dilate; dedicated Extract Mask and Copy/Set Alpha workflows;
  Dot/Backdrop/Group and published controls; FrameHold/TimeOffset. Per-channel
  Shuffle currently provides the underlying copy/set routing, not these workflows.
- Linked Blur XY controls, intermediate-node viewing and
  direct viewer manipulation of new Transform/shape variants, plus remaining
  inspector refinements and native interaction review.
- Full migration/edge-case/resource-pressure qualification, sequence acceptance, native throughput requalification and packaging checkpoints.

### Verification recorded so far

- Native GPU channel/mask/Grade/Merge/data-display and fractional-sampling/Gaussian
  comparisons passed: 11 focused render tests, run serially, including explicit
  alpha operations and the unary GPU operators. A prior concurrent native-GPU run
  crashed; serial execution passed.
- Compositor migration, animation, graph authoring and initial channel routing:
  18 tests passed, including transactional undo, cycle rollback, shape-to-alpha
  connections, data RGB without alpha, and production inspectors at 420/280 px.
- Real 24-channel EXR fixture and twenty-RGB-layer memory-pressure fixture:
  three media tests passed. Three application tests passed, including source replacement undo/redo, nested channel catalogs/values
  and inactive frames, and native GPU beauty/alpha/depth output.
  Each 3×2 displayed image occupied 24 bytes regardless of the 20 additional AOVs;
  each selected view decoded one requested tuple. This measures display allocation,
  not sustained playback performance or total decompression scratch.
- 54 active UI unit tests passed (two native GPU tests remain opt-in), including
  selected-channel and range invalidation of retained images, existing cache and
  independent viewer behavior, and normal/narrow UI tests. Pending-channel views
  no longer retain the previous channel image under the new selection.
- Desktop application all-target check and package-boundary/headless UI isolation
  check passed after integration. Full-feature Clippy completed with inherited
  media, motion, native-probe and workspace-test warnings. Strict linting stops on
  inherited media `type_complexity` / `items_after_test_module`.
- Workspace checkpoint: `cargo test -j 1 --workspace --all-features -- --test-threads=1`
  with the pinned `FOLD_VIDEO_ROOT` and without `LD_LIBRARY_PATH`: **277 passed,
  0 failed, 39 opt-in tests ignored**. Final Read, mask-disable and EXR-resource
  refinements were subsequently checked with focused regressions, including
  native GPU multilayer output. The desktop all-target check covers the Read
  refinements; final media/compositor all-target Clippy also passed with inherited
  media warnings. The full workspace result predates those
  final refinements; it is not a native interaction/performance certification.
- Initial all-feature compilation required the pinned video runtime; its root was
  located and supplied. Two earlier workspace builds were interrupted, first for
  nested-image implementation and then under build pressure. The completed run
  used one build job. Approximately 20 GiB of incremental compiler caches from
  before this phase were removed when disk space became tight; current build
  outputs and source files were preserved.
- Native UI review is unverified: Orca reported `runtime_unavailable`; attempting
  to open it timed out. Automated ImGui drawing is separate evidence and does not
  certify native keyboard, picker or pointer workflows.


### Numerical and ownership details

- Grade defaults to identity with unpremultiply/process/premultiply enabled.
  Gamma preserves the sign of negative values. Zero alpha is handled explicitly;
  alpha itself passes through Grade and the RGB unary operations. Independent
  scalar data channels use the red component of Grade controls.
- Gaussian size is sigma in source pixels with normalized three-sigma support,
  independent X/Y values and transparent or extended-edge sampling. The CPU path
  reuses its reserved output and reserves one scratch plane; GPU uses two passes.
- Viewer RGB uses the application's existing opaque-black presentation convention.
  Scalar views store opaque linear grayscale for the selected black/white range.
  These presentation choices never alter source or export values.
- The EXR adapter currently limits a flat part to 256 channels, 4,194,304
  pixels per window and 128 MiB compressed input, with explicit errors when budgets
  or supported format capabilities are exceeded. Requested nonfinite samples are
  rejected. This is a documented limit, not complete OpenEXR format coverage.

- Final Read behavior: the desktop picker lists only supported compositor sources
  (MP4 and flat EXR). Inactive nested reads query structural channel metadata and
  return zero channels without compiling/decoding their child; an incomplete child
  remains an explicit error when active. Source lowering enforces the render-node
  budget while constructing the graph, before it can grow unbounded.

- EXR resource refinement (2026-10-04): sequential ZIP/RLE decoding reserves
  selected tuples plus bounded block scratch rather than full source planes.
  Leases follow the pinned decoder's retained thread-local interleave buffers.
  Other codecs still use conservative whole-part scratch accounting pending
  codec-specific qualification. A 1024×512, twenty-RGB-layer ZIP fixture decoded
  the selected RGB tuple with 448 MiB of the 512 MiB working budget occupied;
  retaining all source planes would require 120 MiB. Three media regressions
  passed. This change postdates the workspace checkpoint above.
