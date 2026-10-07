# Phase 57 — Reusable GPU effects, transitions and instance timing

Authorized 7 October 2026. **Complete pending review and optimizations — accepted by the user on 7 October 2026.** The user reviewed the updated UI and requested committing the phase. This document retains the full agreed scope and evidence. Unchecked items are post-phase review, implementation and optimization follow-ups; this status does not assert that unverified workflows or unfinished GPU lowering are complete.

## Product decisions

1. Effects are reusable, artist-editable graph definitions, not necessarily single shaders. The same definition can be instantiated as a compositor node, a timeline clip effect, or a Composition layer effect, including rendered Motion layers. Definitions expose stable typed input/output identities, promoted controls, defaults, ranges, animation metadata, explicit dependencies and versions. Instances own values, animation and bindings; editing one instance must not rewrite its definition or another instance. Stack order is processing order; opening an effect navigates to its actual implementation, without a duplicate hidden authoring graph.
2. Both Compositing graphs and common Compositions can implement effects/transitions. A Composition may combine Motion and Compositing graphs with explicit published inputs. Preserve independent feature engines: hosts resolve capabilities, never interpret another engine's graph schema. Project keeps opaque documents; Render receives execution operations, not clip/layer/effect models.
3. Contracts: an image effect receives Source and returns Image; a transition receives Outgoing, Incoming and instance timing and returns Image; adjustment effects receive the composite accumulated below their position. Additional typed inputs support masks, textures and other controls. Host inputs are invocation-local, never destructive source rebinding. Definitions and instances retain unknown extension data and stable references through save/reopen.
4. Transitions are timeline-owned intervals with explicit alignment around the cut and source-time mapping. Default centered placement requires source handles; insufficient handles reject with an actionable error rather than silently freezing or retiming. A later explicit hold policy may extend this. Editing a transition duration changes choreography, not underlying footage speed. Audio remains separately controlled; adding a video transition does not silently add an audio fade. Use exact rational interval arithmetic and half-open activity. Progress is elapsed/duration, so the last sampled frame need not equal one; after the interval the incoming clip is used directly.
5. Dissolve uses a true Mix: `(1-p)*Outgoing + p*Incoming`, including premultiplied alpha. It is not foreground-over-background opacity. Existing compositor Merge A remains foreground and B background. Test transparent pixels, finite HDR/negative RGB, endpoint continuity and color-space consistency.
6. Composition layer processing order is source → effect stack → layer transform/opacity → containing stack. Adjustment layers process lower siblings accumulated within their containing group, over their own active range, in containing-composition coordinates. Timeline adjustments process lower video tracks within their explicit active range. Neither may read an output that already includes itself. Nested group scope and cycle checks must be explicit.
7. Timing is a reusable primitive, not a clip-specific oscillator. Default range comes explicitly from the owning invocation; editor focus and parent-search heuristics are forbidden inputs. It exposes raw Progress, curve-shaped Value, Elapsed seconds and Duration. A standalone graph uses its explicit document range; nested instances receive their own range. Explicit range override is supported. Preserve rational time until scalar evaluation.
8. Timing defaults to linear, clamped progress; Repeat and Ping-pong are explicit alternatives. Oscillator retains periodic defaults. Share normalized curve evaluation rather than copying oscillator semantics: its existing modulo-at-one behavior must not leak into clamped transitions. Preserve raw progress independently to avoid double easing. Curve edits use shared UI controls and transactional undo/cancel. Two instances can have different durations and controls.
9. Templates may retain fixed intro/outro durations with a flexible hold. If the assigned duration is shorter than intro+outro, compress both proportionally. Build this as a reusable construction on the timing primitive; per-copy/glyph staggering similarly composes with timing. Do not turn Timing into a monolithic animation node. Duration fitting does not implicitly retime media or Solver state.
10. Live Shader is initially a WGSL image-processing node with named images, typed parameters and one image output. An explicit source-associated interface schema supplies stable identities, defaults, ranges and labels; generate GPU bindings and validate source against it. Compile off the UI thread; preview successful candidates, retain a clearly identified last-good result on error, and reject stale publication. Interface changes are atomic and preserve compatible values, animation and connections. Renames/removals/type changes require explicit migration; conflicting artist edits reject. Pin successful source/definition versions for export.
11. Shader authoring and Motion Rust assets remain distinct: runtime pixel execution versus author-time native graph construction. Reuse their proven identity/migration/job patterns where appropriate. Shader sampling footprints are explicit; conservative full-frame evaluation is the initial fallback for unknown footprints. Arbitrary WGSL declares GPU requirements; CPU-only execution must report an unsupported requirement, never bypass an effect.
12. Reference is a common artist concept over existing stable output references, not an untyped universal graph. Image references accept image-capable footage, Composition, Compositing, rendered Motion and timeline outputs. Geometry/field/audio retain typed capabilities. Reference identifies a particular output; Input receives invocation-supplied content. Reusable effects and transitions use Input for their primary media. Expose time mapping, output selection, missing providers and dependency cycles consistently.
13. Houdini-like authoring means small composable primitives, simple useful defaults, promoted controls and inspectable editable definitions. Convenience workflows must manipulate that same construction. Reuse Dear ImGui shared SDK controls; quiet chrome, contextual advanced controls and normal/narrow verification apply. Public MCP/CLI discovery, schemas and transactional operations must support the complete workflow without reading Rust sources.

## Format and scene-unit decisions — added 7 October 2026

The user expanded this phase after reviewing resolution/aspect behavior. These requirements are part of Phase 57 completion, not an optional follow-up.

14. Every reusable invocation carries its logical destination format separately from the requested raster dimensions and render region. Format includes full width/height, pixel aspect and bounds. Half-resolution preview, tiled/cropped requests and viewport zoom cannot change layout. Format and framing enter evaluation/cache identity. Nested content explicitly inherits the destination or retains a fixed canvas with a placement transform; no editor-derived format.
15. Keep existing explicit Transform nodes. Add a separate Relative Transform for compositing/image placement, sharing transform execution. Controls use nine-point anchor, position offsets in percentages, proportional uniform size referenced to width/height/shorter side, and Fit/Fill/Stretch/Native placement. Default centered placement preserves proportions. Scaling an image is distinct from responsive procedural generation; an input's authored frame and destination frame must both be known.
16. MoGraph uses a resolution-independent logical canvas measured in scene units. Retain familiar values (for example 1920×1080 units); output pixels are a rasterization choice. HD→4K changes sampling density only, without rewriting geometry, layout, keyframes or procedural assets. Canvas/geometry extent and output resolution have distinct persisted meaning and editing controls. Audit existing pixel assumptions in generation, fields, media cards, overlays, picking and Rust authoring helpers rather than patching the renderer alone.
17. Aspect-ratio changes have explicit Fit, Fill or Responsive framing. Fit preserves the full canvas with unused space; Fill preserves proportions and crops; Responsive adapts logical bounds while leaving arbitrary authored geometry unchanged. Anchors/relative measurements opt individual elements into responsive layout; optional aspect-based author constructions may rearrange content. No guessed automatic artistic reflow. Stretch is an explicit image-placement option, not the default Motion framing.
18. Motion object/group layout exposes simple anchor, relative position and proportional size controls backed by the same editable primitives. Stroke widths, text sizes, spatial effect distances and offsets have declared scene-unit versus output-pixel meaning; scene-unit values scale with the design and explicit pixel values stay fixed at full output resolution. Do not multiply geometry coordinates on a resolution edit.
19. Format exposes dimensions/aspect/center and useful bounds to procedural networks and shaders. A circular wipe uses normalized center, aspect-correct distances, and radius to the farthest destination corner; it must remain circular and cover portrait, landscape and off-center cases. Resolution changes update constants/resources without normally recompiling shader pipelines. Test the same definition simultaneously in different formats.

### 57G — Format-aware invocation, Relative Transform and logical Motion canvas

- [ ] Shared full-output format/framing math, explicit logical-versus-raster dimensions and invocation propagation.
- [ ] Separate Relative Transform node with shared transform execution, anchors/proportional size and fit modes; native/public controls and animation.
- [ ] Persisted logical Motion canvas independent from output resolution; audit generators, field/image consumers, media placement, Rust SDK helpers, overlays and picking.
- [ ] Fit/Fill/Responsive framing and composable object/group anchors/relative layout; declared coordinate and distance units.
- [ ] Format-driven circular-wipe construction and mixed Motion transition; portrait HD, landscape 4K, square, off-center, reduced-preview and region tests.
- [ ] Undo/save/reopen, independent shared-definition instances, matched preview/export and actual GPU/resource performance evidence.

## Performance contract

- Image effects execute on the shared GPU Host with resident intermediates; no per-effect image readback, private devices or unaccounted resource ownership.
- A uniform timing value is computed once per invocation/sample and supplied as a GPU parameter. Domain-varying timing/curve math needs explicit supported GPU lowering; the existing CPU oscillator field evaluator is not GPU acceleration.
- Rendered Motion effects use the same image path. This does not certify GPU geometry construction or arbitrary Motion graph execution; identify remaining CPU stages and measure their contribution.
- Cache compiled shaders and reusable execution structure; changing animated values must not compile shader pipelines. Node count is not dispatch count. Reuse matching input work and fuse compatible simple operations only with equivalence evidence.
- Glow uses appropriate multipass/downsample algorithms with explicit quality and consistent preview/export semantics. Film emulation/grain retain explicit working-space/alpha treatment and deterministic seed/time dependencies.
- Cache identity includes definition/source content, instance inputs, values, exact timing, dimensions/region, quality and color context. Changing inputs invalidates output and any relevant state.
- Cancellation, stale generations, allocation pressure and GPU-completion lifetimes use existing shared contracts. No UI-thread compilation/media processing.
- Qualify GTX 1070/8 GB using uncached full-resolution 1080p workloads, stack depth, parameter changes and large Motion instance counts. Record preparation, compilation, GPU and frame-readiness times, P95, dispatch/transfer counts and memory peaks separately from warm cached replay. Use the existing 30 fps reference target where applicable; record misses without silently lowering quality.

## Ownership and sequence

Shared timing/math belongs in Animation. Platform owns typed invocation/output contracts. Compositor owns image graph definitions and Shader authoring. Composition owns layer stacks/adjustment scope; Timeline owns clips, transitions/handles and timeline adjustments. Motion owns domain-aware timing consumers. Render owns generic GPU operations and pipelines. App assembles cross-feature transactions, jobs and public tools. Factor a reusable-definition module only where actual consumers demonstrate the need.

### 57A — Shared timing and curves

- [x] Extract shared normalized curve sampling; preserve oscillator compatibility and add clamp/repeat/ping-pong endpoint tests.
- [x] Implement exact range mapping, explicit context propagation, raw/shaped outputs and explicit overrides.
- [ ] Register usable Timing nodes/controls and public schemas; verify independent instances, nesting, random seeks, invalid durations and undo/save/reopen.
- [x] Provide fixed-intro/hold/outro and existing stagger constructions; qualify short-duration behavior.

### 57B — Reusable definitions and GPU image effects

- [ ] Typed definition interfaces, invocation-local images/values/timing, instance identity and migration/persistence.
- [ ] Compositor Input and reusable-instance nodes; graph and Composition providers with explicit published interfaces.
- [ ] One editable Glow definition used as a node and independently on multiple timeline/Composition/Motion layers.
- [ ] Ordered stack add/remove/reorder/bypass/parameter animation, transactions, inspector/navigation and public tools.
- [ ] Film emulation and deterministic grain examples using the same contract, with declared color and quality semantics.

### 57C — Transitions and duration-fitting templates

- [ ] GPU true Mix and CPU reference parity, including premultiplied alpha and HDR.
- [ ] Timeline transition model, exact alignment/handles/source sampling, edit invalidation and transactional creation/trim/removal.
- [ ] Timing-curve dissolve; mixed Motion/Compositing Composition transition with independent instance duration.
- [ ] Native transition controls, discovery and public authoring; intro/hold/outro template demonstration.

### 57D — Adjustments and typed references

- [x] Composition and timeline adjustment hosts, active ranges, nested scope and registry cycle rejection.
- [ ] Common Reference experience over typed output capabilities; distinguish graph Input from Reference and preserve editable source navigation.
- [ ] Verify effects before transforms/opacity, adjustment coordinates and hierarchy behavior.

### 57E — Live WGSL

- [x] Explicit interface/source schema, generated bindings, validation and cached GPU execution.
- [ ] Background compile/candidate preview/last-good diagnostics/cancellation, atomic compatible updates and explicit incompatible migration.
- [ ] Native source editor/port controls and MCP/CLI operations, persistence/bundles/dependency tracking and headless export parity.
- [ ] Sampling footprint/resource limits, GPU-required preflight and failure tests.

### 57F — GPU Motion lowering and integrated qualification

- [ ] Lower supported domain-varying timing/curve operations to GPU consumers; keep unsupported work explicit.
- [ ] Profile/reuse/fuse based on measurements, qualify multipass Glow and uncached mixed stacks.
- [ ] Matched preview/headless/export output; independent instance/source-edit migration, undo/cancel, save/reopen, missing requirements and resource-pressure coverage.
- [ ] Native normal/narrow UI review; full public-tool artwork workflow and verified Fold skill examples.
- [ ] Product/architecture ownership updates, focused checks per slice, workspace/Clippy/boundary checkpoint, final diff review and measured evidence.

## Evidence and current limitations

Initial source audit: named graph inputs currently bind persisted source references; they are not yet invocation-local graph parameters. Oscillator's custom curve sampling wraps at one and runs through CPU fields. Shader pipelines exist internally in Render, but no general live Shader authoring contract exists. No Phase 57 completion or performance claim yet. Existing unrelated `demos/timeline-playground.fold` changes are outside this implementation.

### Implementation checkpoint — 7 October 2026 (phase remains open)

Implemented in the working tree:

- Shared exact timing ranges and normalized curves; Motion Timing/Curve nodes and oscillator compatibility. Uniform samples are reused; staggered samples still use the CPU field evaluator.
- Platform effect interfaces, instance-owned controls/animation, shared image-prefix invocations and nested timing/format context. Compositor Input/Instance, published property bindings, and Composition input-layer/control mappings consume that contract. Full-length reusable Composition layers follow the invocation duration; explicitly shorter layer ranges retain seconds. Media clocks are not stretched.
- Timeline clip stacks and exact transition intervals/handle validation; Composition layer stacks and lower-sibling adjustments. Shared native stack controls are being integrated with the existing preview/undo lifecycle.
- GPU Live Shader operation, source/interface authoring, explicit migration, cached pipelines and GPU-required failure on the CPU renderer. Native interface drafts and public tools share the same migration. Current pixel programs are **loop-free WGSL**, with up to 8 images and 32 scalar/vector controls, nearest image sampling and conservative full-frame footprints. Native graph operators provide blur/multipass kernels. This is a deliberate initial restriction, not unrestricted WGSL support.
- Editable graph presets for Glow, Dissolve, Circular Wipe, Film Look and Grain. Film Look is a simple working-space grade, not a calibrated physical film model. Glow currently uses the native full-resolution Gaussian operator; downsample/quality qualification remains open.
- Relative image Transform and logical Motion canvas with Fit/Fill/Responsive framing, Format/Anchor primitives and canvas-aware authoring/viewer mapping. Responsive framing preserves logical height and adapts width. Existing documents retain legacy coordinates until a canvas is declared or captured by an output-format edit; no saved geometry is rewritten. Canvas edits use the same revision-bound transaction path as other Motion edits.

Verified so far: focused exact-timing, oscillator, canvas, independent compositor instances, premultiplied dissolve, interface mismatch/migration and preset source-validation tests. Application tests verify Composition input/control routing, nested transition duration, lower-track timeline adjustment ranges, and nested-group Composition adjustment scope. Native shader correctness, uniform-cache reuse, invalid source rejection and recovery pass on the **GTX 1070**, Vulkan driver 580.173.02. Worker-publication tests verify cancellation and invalid source do not publish saved changes.

Native authoring now includes editable preset creation, property promotion, clip/layer stacks, timeline transition duration/alignment and controls, timeline/Composition adjustments, shader interface migration, and logical canvas controls. Shader source publication uses a cancellable, revision-guarded validation worker; backend pipeline creation remains part of worker rendering. Direct graph-authoring operations can retain draft source; `fold.compositor.publish-shader` validates source before publication. This is not a guarantee that every backend will accept every source validated by Naga.

The first uncached **1920×1080, 60-frame** hardware probe is retained at [gtx1070-effects.jsonl](evidence/phase-57/gtx1070-effects.jsonl). Same renderer/pipeline cache, no frame-output cache; animated Glow strength and grain clock. Warm figures exclude frame zero; P95 uses the sorted sample at floor(0.95*(n-1)).

| Workload | Prepare P95 | GPU P95 | Ready P50 / P95 | Cold readiness | Passes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Glow, full-resolution radius 16 | 0.52 ms | 5.96 ms | 10.90 / 13.02 ms | 52.56 ms | 5 |
| Eight alternating Film Look / Grain | 2.19 ms | 3.21 ms | 10.20 / 12.05 ms | 30.07 ms | 9 |
| 1,024 Motion ellipses + Glow | 1.38 ms | 16.69 ms | 22.79 / 27.02 ms | 26.03 ms | 7 |

All three warm P95 measurements meet 33.3 ms on this run. Image upload/readback bytes are zero. Pipeline compilation is zero after initial use. Peak tracked Host allocation across the run is 137,406,772 bytes; this is tracked resources, not total driver/process VRAM. Motion geometry is static and can reuse tessellation/instance uploads; effect controls change every frame. This does **not** qualify animated domain-varying Motion fields, sustained playback, export, 4K, or allocation pressure. Initial Glow compilation misses a frame budget. Film Look is measured in the legacy color configuration; this is not calibrated film/color acceptance.

Still required before phase acceptance: integrated template demonstration, complete native interaction/normal-narrow review, public-tool artwork workflow, varying-field GPU lowering, mixed Motion/Compositing and cross-format output qualification, successful-source/backend last-good behavior, sustained and pressure measurements, workspace checkpoint checks and final architecture documentation. Aggregate checkboxes remain open where only part of their contract has landed.

### Integration review additions

- GTX 1070 circular-wipe pixel checks pass for portrait HD, landscape 4K and square **logical formats**, off-center and centered wipes, two preview densities and four times. Shader compilation remains one across format/center/time changes. These are reduced-raster correctness checks; full-raster 4K throughput remains unqualified.
- `skills/fold/examples/reusable-dissolve.json` is exercised through public batch staging, undo/redo, project save/reopen and premultiplied pixel comparison. [Agent guide](../skills/fold/references/effects.md) records exact scope; live desktop/MCP save/reopen remains separate acceptance.
- Scoped PanelContext now forwards effect-interface discovery and worker-job status. Inspector cancellation is scoped to the exact shader request. Effect keys move with timeline clips/cuts, and split clips receive independent effect instance IDs.
- Source validation rejects extra GPU bindings, workgroup storage, renderer output/status access, loops and value types larger than 64 KiB. Image inputs remain read-only; large data belongs in explicitly declared images. This prevents shader source from changing the renderer's binding/resource ownership contract.
- Workspace all-target check passes. Desktop all-target Clippy completed with warnings; new simple range-pattern warnings were corrected. Existing UI → Media/Project boundary-policy failures remain, as documented in architecture before this phase.

Remaining concrete workflow gaps include inspecting a published Input-based definition in its host invocation while editing, native publication of mixed Composition interfaces, complete migration interactions and the broader normal/narrow host review. Opening an Input-based definition alone currently requires an invocation to render. GPU domain-varying Motion lowering remains unimplemented. Keep these requirements open rather than substituting static-node or software-rendering evidence.

Native review artifacts: [Relative Transform, narrow](evidence/phase-57/relative-narrow.png), [Shader interface draft, narrow](evidence/phase-57/shader-interface-narrow.png). These are actual Dear ImGui controls rendered by the native backend on llvmpipe, not hardware throughput evidence. Earlier filtered Timing and Canvas captures passed. The broader Motion capture fixture previously failed an oscillator-selection assertion; its baseline cause and full rerun remain open. Do not interpret the filtered captures as full native workflow acceptance.

Final checkpoint checks for this working-tree update: six `fold-app --features desktop --test reusable_effects` tests pass (native wipe is separately qualified on GTX 1070), nine compositor-session tests pass including scoped inspector discovery/cancellation, seven timeline sequence tests pass, three Motion canvas and two Timing tests pass, nine Animation tests pass, five compositor reusable tests pass, and two headless shader validation/region tests pass. The final native shader parameter/error/recovery test passes on GTX 1070. Desktop all-target compilation and `git diff --check` pass. Workspace check and desktop Clippy completed at the earlier checkpoint, with the recorded warnings and boundary-policy discrepancy.

The mixed-template regression exercises one shared Composition containing a timed Motion overlay and a mapped Compositing dissolve, instantiated at 8 and 16 seconds and evaluated at 4 seconds beyond its original one-second range. Full-length procedural objects/layers follow the explicit invocation range; shorter authored layer ranges retain seconds. Source assets keep their own bounds. Public `catalog guide="effects"` returns both the authored guide and the regression-tested batch example.

## Inspector and transition interaction follow-up — 7 October 2026

The authorized UI contract is now implemented: clip inspectors contain the source, optional Open Network, position/scale/rotation, opacity and effects. Precise Edit, Apply controls and redundant source/link diagnostics are removed. Composition inspectors expose the layer container's transform, opacity and effect stack. Selecting a layer reveals its graph without handing Inspector ownership to remembered nodes; explicit node selection transfers ownership, and reselecting the layer restores the container Inspector.

Timeline transition strips select transition IDs. Their Inspector exposes duration, alignment, promoted controls, Edit Graph, replacement and removal. Edge drags resize and body drags move the transition interval within available source handles without retiming the clips. Boundary selection/context-menu creation uses an application-owned transaction to create a default Dissolve definition when needed. Gesture previews cancel or commit one undo step. The Welcome demo's third text layer now owns Glow directly; the separate halo layer and unused graph have been removed and the revision saved after a GPU render.

Focused verification: 12 timeline UI/model tests, seven multitrack tests, ten Composition tests, the shell container/node-selection regression and atomic default-transition undo test pass. Dear ImGui tests require `--test-threads=1`; parallel execution produced ContextThreadConflict, resolved by serial execution. Native Composition and timeline clip/transition captures passed at normal and narrow sizes and were visually reviewed ([timeline inspectors](evidence/phase-57/inspectors-normal-narrow.png), [Composition](evidence/phase-57/composition-inspector-narrow.png)). Full manual pointer review and sustained playback remain unqualified; these results do not close the broader Phase 57 acceptance list.

## Completion acceptance — 7 October 2026

User acceptance: “Looks good. Commit and mark this phase done but pending review and optimizations.” Phase 57 is closed for this delivery with review and optimization follow-ups retained. Historical checkpoints below the design remain evidence of what was verified at each stage; their open-status wording is superseded by this acceptance. The final desktop build passes, the saved demo reopens on the shared GPU backend, 31 focused interaction tests pass, and normal/narrow inspector captures were reviewed. The unchecked scope/qualification items remain actionable follow-up work.
