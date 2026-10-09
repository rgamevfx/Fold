# Current architecture and dependency policy

> **Pre-rewrite baseline — 8 October 2026.** This document describes the pre-rewrite source, not the target motion architecture. Use the [Motion System Rewrite Plan](Motion_System_Rewrite_Plan.md) for motion, MCP/agent ownership, priorities and acceptance, and its linked proposal for target semantics. Shared non-motion ownership remains useful; legacy motion and protocol descriptions below are removal context. The old dev-plan is retired.

Source-inspection revision: 7 October 2026.

This document maps the [product specification](Fold_Application_Product_Architecture.md) to current implementation. [The rewrite plan](Motion_System_Rewrite_Plan.md) owns current priorities and acceptance; [dev-plan.md](../dev-plan.md) preserves the retired ledger. Historical phase/consolidation documents describe their own checkpoints; they are not current implementation inventories. This revision is a code audit, not a fresh runtime, performance or release certification.

## M5a implementation update — 9 October 2026

This section supersedes the baseline's Motion/compositor/protocol inventory.
`fold-graph` owns typed node/parameter/edge records, bindings/tracks, expressions,
source support and conservative reconciliation. Compositor schema 8 persists
those native records; `OperationConfig` is a transient specialized kernel/UI
projection. Schemas 1–7 remain opaque, with no reader or execution fallback.
Motion owns procedural object graphs and authoritative scene hierarchy. Common
Outliner/Composition rows reference those objects; the existing Network hosts
object and compositor Graph/Source views. Composition owns Track/Curve editing.
Inspector nested occurrence paths supply exact local clocks, including reverse
and paused reuse. Instance override channels retain native key identities.

`fold-project::proposals` owns generic immutable candidate lifecycle. App's v2
native dispatcher composes public creation, hierarchy, source and structured
provider edits, explicit image bindings, inspection/rebase/Apply and save-copy.
Shared project/render infrastructure contains no feature schema. See the
[28-operation removal audit](rewrite/m5a-compositor-inventory.md) and
[qualification evidence](rewrite/evidence/m5a-compositor-cutover/result.md).
Earlier physical/performance and M0–M5 obligations remain separate.

## Ownership

| Crate | Current responsibility | Source entry point |
| --- | --- | --- |
| `fold-foundation` | Stable IDs and checked rational time | [time](../crates/foundation/src/time.rs) |
| `fold-animation` | Shared curves, key identity, interpolation and evaluation | [animation](../crates/animation/src/lib.rs) |
| `fold-color` | Pinned OCIO runtime/configuration, authored-color interpretation and CPU/GPU processor resources | [color](../crates/color/src/lib.rs) |
| `fold-native-video` | Native decoder helper protocol, device/plane ownership and readiness | [native video](../crates/native-video/src/lib.rs) |
| `fold-project` | Opaque envelopes, assets, bins/items, revisions, transactions, history and persistence | [model](../crates/project/src/model.rs) |
| `fold-media` | Source inspection/pinning, bounded decode/EXR/audio preparation and encode adapters | [media](../crates/media/src/lib.rs) |
| `fold-render` | Image IR, CPU/GPU execution, vector coverage, channels, output processing and admission/resource services | [render](../crates/render/src/lib.rs) |
| `fold-platform` | Static registration, output/authoring/animation capabilities and desktop/workspace contracts | [platform](../crates/platform/src/lib.rs) |
| `fold-ui` | Dear ImGui backend, shared editors/widgets, viewer presentation, workspaces/preferences and native drops | [UI manifest](../crates/ui/Cargo.toml) |
| `fold-timeline` | Sequence/track/clip models, editing and video/audio compilation, optional UI | [timeline](../crates/timeline/src/lib.rs) |
| `fold-compositor` | Image graph validation, channels, compilation, authoring and optional UI | [compositor](../crates/compositor/src/lib.rs) |
| `fold-composition` | Common layer state, hierarchy, timing, animation, graph-output assembly and optional UI | [Composition model](../crates/composition/src/model.rs) |
| `fold-motion` | Procedural graph, geometry, fields, Solver, Wrangles, native Rust assets and optional UI | [Motion](../crates/motion/src/lib.rs) |
| `fold-app` | Concrete registration, cross-feature commands, session/workers, playback, delivery, automation and binaries | [assembly](../crates/app/src/packages.rs) |

Timeline, Compositor and Motion do not import each other's engines. Composition resolves graph outputs through platform capabilities and imports neither Motion nor Compositor. Motion deliberately reuses Composition's common layer-state contract. App stages operations that span these models. Project and Render remain free of creative document schemas.

Creative UI dependencies are optional. App has no default features; `gpu` enables headless GPU rendering, `native-video` adds native media integration, and `desktop` enables UI, native video and audio-device dependencies. See the [app manifest](../crates/app/Cargo.toml). Static Rust registrations are in-process APIs, not a stable plugin ABI.

**Known policy discrepancy:** [check_boundaries.py](../scripts/check_boundaries.py) permits `fold-ui` to depend on Foundation, Animation and Platform, while the current UI manifest also directly depends on Media and Project. Phase 51 records this pre-existing check failure. Reconcile ownership or explicitly revise the policy with evidence; this documentation update neither changes the policy nor certifies a clean boundary check. The script checks declared/transitive dependencies and headless isolation, not runtime semantics.

## Project state and editing

[ProjectState](../crates/project/src/model.rs) contains `Arc<Document>` roots, assets, organization and opaque settings/environment metadata. A document carries provider/type/schema identity, revision, declared references/assets, opaque payload bytes and extension fields. Providers own typed validation and serialization.

[Project](../crates/project/src/coordinator.rs) is the single writer. `EditBatch` carries an expected revision. Generic validation and atomic publication protect envelope/reference integrity; package/application command handlers validate creative meaning. Undo/redo publishes new revisions. History is bounded by entry count, not retained bytes. Do not infer universal typed publication validation solely from generic coordinator validation.

`CommittedSnapshot` is coordinator-issued and required by project save and application delivery. It exposes an immutable evaluation `Snapshot` with no reverse conversion. `EditSession` retains a transient proposal/generation; successive updates replace staged mutations, while previously issued evaluation snapshots remain immutable. Gesture completion commits once; cancellation drops the overlay.

Maintenance C5 keeps [Session](../crates/app/src/session.rs) as the application publication coordinator. Focused command, worker and transport modules return explicit outcomes. File/export worker completions carry a project generation as well as their existing revision guards; replacement cancels work and retains admission until acknowledgement. Saved-state refresh is separate from cached output metadata preparation. See [C5 evidence and acceptance limits](maintenance/evidence/c5/result.md).

Private agent candidates use isolated project state with their own validated snapshots and generation/base checks. Their observation does not publish into the active project. Committing a candidate still requires conflict checks against the live session. See [automation](../crates/app/src/session_automation.rs) and [candidate display](../crates/app/src/session_candidate.rs).

## Composition and creative models

[Composition](../crates/composition/src/model.rs) owns a front-to-back layer list. `LayerState` contains common identity/hierarchy, lock/visibility, timing, transforms and animation. A layer may reference an asset or provider output; an empty source represents a group. Source origin and source start explicitly map local time independently of a trim.

The common stack can contain MoGraph, Compositing, footage and nested Composition outputs. MoGraph object/group references retain actual graph identity. Geometry groups consume children in the same graph; common layer groups can mix graph types. Composition validates hierarchy, unique ownership and declared dependencies.

[Layer graph contracts](../crates/platform/src/layer_graph.rs) expose named image inputs and an explicit provider evaluation context. [Application layer operations](../crates/app/src/composition_layers.rs) synchronize common layers and graph metadata atomically. Selecting a layer opens its real network; no wrapper image-source graph represents the common stack.

[Composition UI](../crates/composition/src/ui/mod.rs) owns Track view and Curve Editor. Provider animation channels expose graph parameters in the owning layer/source clock. Network is graph authoring; standalone Animation/MoGraph panels and Network Animation modes were retired in Phase 51a. See [animation contracts](../crates/platform/src/animation.rs) and [Phase 51](phase-51.md).

Timeline independently owns sequences and linked A/V clip edits. Compositor owns image-node semantics and named-channel operations. Motion owns geometry/field semantics and source graphs. Common Composition does not replace these models with a universal graph.

## Evaluation and execution

Provider compilation operates on an immutable snapshot, output reference, exact time, dimensions and cancellation. Nested outputs contribute to shared image IR. Compilation/evaluation rejects unsupported references and invalid graphs; cancellation propagates through provider and execution work.

[SceneRequest and scene evaluation](../crates/app/src/output_scene.rs) separate scene demand from viewer presentation. [OutputRenderer](../crates/app/src/output.rs) owns retained delivery resources and uses a committed snapshot plus explicit output color settings. Preview and delivery share compilation/evaluation; full-quality equivalence requires matching requests and declared tolerances.

The CPU evaluator is a correctness/reference path. [GPU execution](../crates/render/src/gpu/mod.rs) implements image operations, native-plane reconstruction, OCIO, vector coverage/compositing, textured work and display/output processing. Core working images allocate RGBA32F; selected textured resources use an adapter-supported RGBA32F or RGBA16F format. GPU operation support does not imply every source/geometry preparation stage runs on GPU.

Shared vector geometry with compact instance transforms/paint reduces repeated upload and CPU transformation. GPU coverage and ordered compositing preserve working float color. CPU geometry construction and explicit bounded fallbacks remain part of execution. Selective opacity/over fusion exists; a general tiled optimizer and arbitrary Rust-to-GPU translation do not.

## Reusable effects, instance timing and format (Phase 57 working tree)

[Platform effects](../crates/platform/src/effects.rs) owns the interface and invocation contract: stable named images, scalar/vector controls, instance-owned animation, exact timing range and logical destination format. Providers append to a shared image-graph prefix, so invocation inputs are not copied or rebound in saved definitions. Registry resolution validates declared dependencies, output capabilities, interface versions and cycles. Compositor and Composition implement published definitions in their own schemas; Timeline remains a host. Project retains opaque documents and Render receives generic operations.

Clip and Composition layer effects run before host opacity/placement. Composition adjustments process lower siblings within their containing group; timeline adjustments process lower video tracks within their active range. Adjustment opacity mixes processed and original content. Composition adjustment transforms are rejected because their input already uses containing coordinates. Timeline owns adjacent-cut transitions and source-handle validation. Video transitions do not create audio fades.

[Animation timing](../crates/animation/src/timing.rs) supplies exact range mapping, normalized curves and intro/hold/outro segments. Motion Timing/Curve/Timing Segments nodes consume these semantics; oscillator keeps periodic behavior. Uniform values are sampled once. Domain-varying Motion field evaluation still runs on CPU; GPU image effects on rendered Motion do not imply GPU geometry construction.

[Live Shader](../crates/render/src/shader.rs) is a generic GPU-required image operation with generated bindings, bounded loop-free WGSL, explicit premultiplied alpha and full-frame sampling fallback. Compositor owns source/interface drafts and atomic port migration. App executes revision-guarded, cancellable source validation on its command worker; rendering workers create backend pipelines. A renderer caches up to 64 source-keyed pipelines. Uniform values and logical format do not change generated source. Export consumes its immutable snapshot, never an editor draft or preview texture. Explicit last-good backend behavior and integrated delivery acceptance remain open in [Phase 57](phase-57.md).

Motion persists an optional logical canvas separately from raster dimensions. Fit/Fill preserve geometry proportions; Responsive preserves logical height and adapts width. Format/Anchor expose responsive layout primitives; Format's pixel size converts full-output pixels into scene units. Existing explicit image transforms remain; Relative Transform adds normalized anchors, percent offsets and placement/width/height/shorter-side size references. Old Motion documents retain their coordinates until a canvas is declared or captured by a resolution edit. Referenced image geometry retains logical size when source raster metadata changes.

Hardware measurements and remaining requirements are tracked in [the phase evidence](phase-57.md#implementation-checkpoint--7-october-2026-phase-remains-open). This section documents current ownership, not phase completion.

## Color, media and delivery

[Color runtime](../crates/color/src/lib.rs) pins OCIO **2.4.2**, **ACES 1.3 Studio config 2.2.0**, bundled content identity and **ACEScg** working space. New-project settings use this pipeline; missing legacy color settings retain legacy interpretation. External configs use bounded, copied, hashed resource closures rather than implicit global `$OCIO` discovery.

Input interpretation, viewer display/view/look/adjustments and delivery transforms have separate scope. Current viewer display is fixed sRGB with ACES SDR Video under the input/output-only color UI decision; per-viewer OCIO display/view/look selectors are not implemented. Default delivery uses Rec.1886 Rec.709 with ACES SDR Video. Nonlinear CPU processing unpremultiplies RGB, preserves alpha and repremultiplies. Finite negative/above-one RGB is valid; alpha/nonfinite values have explicit validation. GPU processors use the corresponding OCIO shader/LUT resources.

[Ingest](../crates/media/src/ingest.rs) recognizes MP4/MOV/MXF video, WAV/MP3/M4A/AAC/FLAC audio, P6 PPM and OpenEXR. Codec/profile validation remains in adapters; Phase 55 adds HEVC Main/Main10, ProRes and DNx families alongside H.264. This is not unrestricted file/codec support: PNG/JPEG ingest and arbitrary VFR are not implied. EXR inspection includes channels and sequence discovery.

Media retains source fingerprints, verified source ownership and bounded caches/ranges. Native video transports preserve planes/precision through supported reconstruction paths. The qualified Linux/NVIDIA helper path uses explicit device identity and GPU-local copy/readiness contracts. Software decode/upload remains available; hardware support/performance must be qualified per profile. [Native video](../crates/native-video/src/lib.rs), [media budgets](../crates/media/src/budget.rs) and [source pinning](../crates/media/src/source_pin.rs) own these boundaries.

Audio workers prepare stereo 48 kHz float PCM from supported inputs. Playback consumes a bounded device ring. Exact source offsets and output-range clipping remain part of the compiled plan; callbacks do not decode or acquire project authority.

Delivery resolves an explicitly persisted `fold.output` reference separately from viewer state. Missing explicit output errors instead of falling back; older projects without that selection retain fallback root discovery. Output rendering supports explicit CPU/GPU selection and scene-based alpha delivery; ProRes 4444 uses the higher-precision RGBA output path, not an opaque SDR viewer texture. Full Phase 20 reproducibility/encoding acceptance remains open despite these implemented paths.

## Scheduling, caches and resource ownership

[Scheduler](../crates/render/src/scheduling.rs) provides bounded process-wide admission. Audio/background preparation have separate lanes; graphics consumers use weighted FIFO service at frame boundaries, with export and review preparation receiving service alongside viewers. Admission is worker-only. It is not a universal fine-grained task-graph scheduler.

[PreviewWorker](../crates/app/src/preview_worker.rs) keeps bounded per-consumer pending demand, deduplicates matching work and tracks each consumer's generation. Source preparation runs through a separate bounded worker. Seek/edit/retarget retires obsolete consumers; work shared by other consumers can remain useful. Results still require current identity/generation checks before presentation.

[GPU Host](../crates/render/src/gpu/host.rs) shares device/queue, image pooling and memory accounting. Default engine allocation budget is **1 GiB**. Resource reuse depends on GPU completion and retained ownership, not CPU reference count alone. Export can share the desktop Host; a headless process owns its own Host. Decoder/driver memory is not fully represented by Host accounting.

[Presentation cache](../crates/ui/src/preview.rs) retains viewport/region-specific, display-transformed SDR textures under a **256 MiB** default limit. [BC7 compression](../crates/render/src/gpu/cache.rs) complements immediate RGBA8 presentation. Phase 58 removes the previous quality-comparison pass and its status readback: BC7 is explicitly lossy playback storage, with completion callbacks guarding readiness. Displayed/in-flight resources are protected; compression is not a foreground presentation prerequisite. Working/source/geometry caches are distinct from presentation retention.

[Playback retention](../crates/ui/src/preview_retention.rs) owns one bounded worker for GPU compression, RAM retention and disk I/O. [Renderer transfers](../crates/render/src/gpu/presentation.rs) preserve BC7 blocks, reuse a staging buffer bounded to 64 MiB, account for GPU allocations and restore display leases without evaluation or another color transform. Direct presentation never waits for this work; retention submits only while paused and without foreground demand. Transfers already submitted finish safely. The worker releases graphics admission before waiting for completion. [Platform storage](../crates/platform/src/playback_cache.rs) owns the separate 1 GiB default viewer RAM allowance and independent, opt-in timeline/composition disk stores. Each disk store has its own folder, byte budget, index, eviction and clear operation. CPU image processing remains a separate 768 MiB default (one-third decoded, two-thirds working). Metadata is bounded to 131,072 entries per lower-tier store; staging and one temporary CPU payload are additional bounded overhead. Disk files are versioned disposable display pixels, never delivery sources.

GPU lookups use a hash index and byte limits, replacing the old linear lookup and 512-entry ceiling. Explicit range preparation can retain beyond GPU capacity; current-frame presentation and eviction protections remain GPU-owned. A valid lower-tier hit suppresses graph demands even while storage is busy. Cached-only replay stops on an actual miss; normal playback can render after an entry is found missing/corrupt. Ordinary cache bars currently describe GPU readiness; prepared-range status includes all tiers. Native tests and remaining sustained-performance acceptance are recorded in [Phase 58](phase-58.md).

Cache identity includes output/content, exact time, requested dimensions/region, channel view and relevant color/display processing. Cache bars describe actual matching resident coverage. Export never obtains scene pixels from the presentation cache. The separate project Cache-folder metadata remains reserved for future Cache-node output. Application playback disk caches retain exact viewport/output/channel variants; a different ROI or resolution may need a new render. Switching channels never caches every source plane, and old view variants survive until eviction. The existing [Image Bake service](../crates/platform/src/image_bake.rs) separately owns prepared-texture references and source invalidation; the [application job](../crates/app/src/image_bake.rs) builds local `.foldtex` resources for image-driven Motion. It supports mipmapped compact BC7 or Float16 data with explicit color/alpha and bounded background work. BC7 has lossy SDR limits; this source-texture workflow is not the display cache or a general Cache node. See [Phase 50](phase-50-image-bake.md).

Logical budgets, decoder allocations, process RSS, driver VRAM and disk scratch need separate measurements. History/snapshot retention and long-running failure/pressure coverage remain Phase 21 concerns. Phase 19A/19B qualification applies to its recorded workload, not all newer EXR or Motion work.

## UI, workspace and playback

The desktop uses Dear ImGui, winit and wgpu; GPU resources now serve engine evaluation as well as presentation. `fold-ui` owns shared components and private backend integration. Creative packages contribute panels through optional UI features. Native file drops route Wayland/X11 input into existing background ingest and transactional placement.

[Workspace](../crates/platform/src/workspace.rs) separates panel-instance IDs from contribution IDs. Editors carry document/navigation/selection context. Viewers retain independent binding, source editor type, exact time, quality, channels, range, loop and playback-mode override. Link groups A–D associate editor types; Unlinked viewers retain explicit output references. Inspectors follow explicit group selection or a locked target independently of viewer source.

[PanelContext](../crates/platform/src/panel_context.rs) routes edits, keyframes, seeks and overlays through that explicit context. Focus does not silently retarget a viewer or delivery. Shell navigation routes cross-type document opens to the matching editor in the same group, retaining the source panel and mapping exact source/parent time. Explicit document opens and Timeline/Composition tab clicks retarget linked viewers; pinned outputs and other groups remain independent. Layer graph reveals retain the parent Composition context and reuse the group's graph editor.

Real-time playback may skip video to follow its own clock, using monitored audio where active. Every-frame advances on consecutive presentation, is capped at document fps and is muted. One viewer is monitored; focus/play on another does not steal it. Closing the monitor clears monitoring. Review preparation and cached replay do not silently switch mode.

The viewer holds its last valid image during ordinary preparation. Requested time and displayed-frame identity stay distinct, and stale generations cannot replace the current request. Actual failures and missing outputs remain visible without routine loading flashes.

[WorkspaceStore](../crates/ui/src/workspace_store.rs) persists versioned project-associated sidecars on a bounded I/O worker; layout is no longer session-only. Preferences have separate storage. Project/item settings use explicit targets and draft Apply/Cancel semantics. [Settings](../crates/app/src/settings.rs) handles project defaults and color validation with provider-owned format edits.

`UiId`, `EditResponse` and shared numeric/animation controls give stable identity and gesture semantics. They are concrete SDK contracts, not a complete universal public parameter/plugin schema. Use the [Fold UI skill](../.agents/skills/fold-ui/SKILL.md) for interaction rules. No UI behavior was visually re-certified by this documentation audit.

## Motion programs and public agent access

[Solver](../crates/motion/src/nodes/solver.rs) represents fixed-step state using editable update groups, explicit initial state and bounded evaluation. Stateless graph evaluation remains arbitrary-time. Solver support does not imply general physics or unbounded particle workflows.

[Wrangles](../crates/motion/src/wrangle/mod.rs) provide bounded programmable runtime evaluation. [Rust assets](../crates/motion/src/program/mod.rs) instead construct reusable native groups at author time. The [compiler](../crates/motion/src/program/compiler.rs) validates bounded Rust source, compiles through the supervised toolchain and executes WASM with memory/fuel limits and no host imports to return a validated construction blueprint.

Source records retain stable node keys, source and the baseline native definition. Updates preserve compatible instance values/animation/connections; explicit migrations handle interface changes, and conflicting internal edits reject rather than overwrite. [Bundles](../crates/motion/src/program/bundle.rs) carry native definitions, source and embedded dependencies; external media/documents stay explicit. Multi-definition coordinated source migration is not general automatic reconciliation.

[Automation](../crates/app/src/automation/mod.rs) supplies live MCP/CLI access, discovery/schema services, bounded jobs, private candidates and observations. Application assembly creates Rust assets and their owning Composition layer in one batch. Public-tool completeness and reliable authoring remain separately tracked; Phase 56's accepted trial does not close Phase 47's benchmark.

## Persistence and compatibility

[Persistence](../crates/project/src/persistence.rs) writes a version-3 `fold-project` JSON archive and accepts only version 3. Unsupported containers fail without being rewritten. Executable provider schemas are Sequence 5, Composition 3 and Compositor 8; Compositor Grade/Merge/Transform/Blur use version 2. The two-layer Timeline provider and workspace upgrades are retired (C1). Unknown payload bytes, metadata, declared dependencies, archive extensions and assets survive load/save; core does not migrate unknown feature data.

Save writes/syncs a temporary sibling, replaces the destination and syncs the directory on Unix. `SaveError::DirectorySync` reports visible replacement with unconfirmed durability. Save/export pin committed state, not an active gesture overlay. Recovery retention/autosave, migrations, complete environment restoration and broad-platform packaging remain open integration work.

Do not introduce automatic common-Composition adoption on load: Phase 51 explicitly rejected that compatibility path. This is separate from preserving unknown envelopes or legacy color interpretation.

## Known gaps and validation

The [current roadmap](../dev-plan.md#current-roadmap) carries remaining obligations:

- Reconcile older Project/public-authoring gaps against later placement and Rust-asset work without retroactively claiming missing acceptance.
- Resolve the UI dependency-policy mismatch and inherited hardening obligations.
- Resolve the recorded combined StockX CPU/GPU nearest-transform discrepancy; qualify full-resolution uncached EXR throughput and remaining production compositor semantics.
- Diagnose Phase 56 cold desktop playback/cache-fill pacing separately from renderer readiness and warm replay.
- Finish delivery/recovery/packaging and long-running mixed-viewer/audio/resource acceptance.
- Complete the separate Phase 47 public-authoring reliability gate; declarative construction/reconciliation and further field/GPU work remain proposals.

During iteration, format touched files and check/test affected crates. At a vertical-slice checkpoint:

```sh
python3 scripts/check_boundaries.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked
```

Record actual failures and ignored/deferred checks. Native media/color builds require their packaged dependencies; desktop builds also require platform development libraries. Keep machine-specific paths outside repository configuration. Headless tests, native widget captures, virtual-display playback and physical-monitor/audio acceptance are distinct evidence. Do not convert historical successful runs into a current full-workspace pass.

### Phase 57 inspector and network selection

The workspace stores the displayed Network editor independently from the Inspector's selection owner. Opening a Composition layer replaces remembered graph-node selection with the requested layer-object scope, reveals its contained graph, and retains the Composition container selection for the Inspector. Explicit node selection transfers Inspector ownership; reselecting the layer restores the container. Layer and timeline clip inspectors share the effect-stack UI. Timeline spatial transforms are preserved in clip extension metadata and lowered after effects, before opacity/compositing. Transitions select their own stable IDs and preview interval edits without changing clip source clocks; the application command `fold.app.add-transition` creates the default graph and transition in one transaction across the independent feature packages.
