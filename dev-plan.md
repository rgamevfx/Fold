# Development plan

- This is the active refinement and integration plan. It supersedes `docs/archive/Dev-Plan-initial.md`, archived with its existing status and unfinished hardening checkpoint intact. Retiring that plan does not declare its unfinished work complete.
- Before each phase, write a brief plan in `docs/phase-N.md`: goal, implementation boundaries, tasks, acceptance criteria, and verification. Continue numbering at 11 so existing phase documents are not overwritten; the old unfinished phase 10 is carried into the hardening work below.
- Follow `docs/Fold_Application_Product_Architecture.docx`, `AGENTS.md`, and the Fold UI skill. The explicit ACES/ACEScg and per-viewer-transport decisions below refine the product proposal and take precedence over the earlier fixed-SDR/shared-transport scaffolding.
- During iteration, format touched files, check affected crates, and run focused tests for changed behavior. Run workspace checks, full suites, Clippy, and package-boundary checks at slice checkpoints. Record pre-existing failures separately; do not claim unperformed native checks or performance measurements.
- Mark a phase `[x]` only when its acceptance criteria pass; record evidence, remaining limits, and deliberately deferred work in its phase document. Include the tracker update in the phase commit. A technical spike is not implementation acceptance.
- Preserve Rust application logic, Dear ImGui through the shared UI SDK, peer creative packages, stable references, transactional undo, immutable render snapshots, exact rational time, and unknown extension data. Do not introduce feature models into project/render infrastructure or move render authority into panels.
- Keep changes incremental and end-to-end. Retain working reference paths and existing fixtures until replacements pass equivalent tests; remove superseded scaffolding rather than maintaining two competing workflows. Add abstractions only where the new requirements demonstrate a need.

## Agreed product and execution decisions

### Reference platform and scope

- Target this Linux machine with an NVIDIA GTX 1070 and 8 GB VRAM. Record the actual OS, CPU, RAM, driver, graphics backend, codec/runtime versions, and build for measurements. This is a modest reference configuration, not a claim of broad hardware support.
- Keep one native window with internal docking, static packages, and a narrow verified media profile initially. Multiple editor/viewer panels are in scope; multiple native windows, runtime plugins, general 3D, simulations, and broad codec/platform coverage remain deferred.
- GPU evaluation is a first-class delivery requirement, not satisfied by uploading CPU-rendered frames to a GPU viewer cache. Hardware decoding, hardware encoding, and codec/GPU interoperability are separate capabilities, each requiring measured correctness on the reference machine.
- Do not assume portable zero-copy interoperability. Keep a measured CPU decode/upload and readback/encode fallback, without allowing unnecessary round trips between supported GPU operations.

### ACES and OpenColorIO

- Ship a pinned, versioned ACES OCIO configuration and the runtime/resources required to use it. New projects default to ACEScg working space. ACES/OCIO is required in this plan, not a later expansion.
- Provide config-driven input color-space assignment, display/view and applicable look selection, and explicit export/output transforms. Support validated external OCIO configurations. Choose exact ACES/config/OCIO versions and default SDR display/view during phase 11, then record and test them rather than depending on whatever is installed globally.
- Preserve source primaries, transfer, matrix, range, chroma location, bit depth, and alpha interpretation. Correct YUV reconstruction precedes the appropriate OCIO input processing; OCIO is not a replacement for correct decoding. Input interpretation must be visible and overridable when metadata is ambiguous.
- Use scene-linear premultiplied ACEScg for normal compositing, normally RGBA16F on the GPU, with higher precision where an operation requires it. Preserve valid negative and above-one RGB values; alpha and nonfinite-value validation remain explicit. Nonlinear transforms require correct unpremultiply/repremultiply handling.
- Separate working images, display images, and encoder input. Apply each transform once. Export must not inherit a viewer's exposure, look, or display setting accidentally. Matching full-quality preview and export must agree when their explicit output settings match.
- Preserve existing projects deliberately: do not reinterpret legacy linear-sRGB authored values as ACEScg or silently change their appearance. Define compatibility/migration behavior with fixtures before changing defaults or schemas.

### Viewer cache and resource ownership

- Retain the decision in `docs/adr/0001-viewer-cache.md`: cache display-transformed 8-bit GPU images at the requested preview resolution, not linear ACES image files. For the initial SDR path, use RGBA8 with the selected OCIO display/view transform already applied; draw overlays afterward.
- Keep viewer presentation caching separate from decoded-source, geometry, and intermediate-result caches. Native decode planes and ACEScg working textures may be retained in their own budgets where reuse justifies them. Export and scene-linear analysis never read back the 8-bit viewer cache as their source.
- Include config/processor identity, display/view/look, baked viewer adjustments, exact time, output identity, resolution, and quality in presentation keys. Color interpretation and working-space changes invalidate affected upstream results; a display-only change need not invalidate scene evaluation.
- Share reusable content across viewers. Budget globally rather than cloning a full cache/decoder budget for each panel. Protect all displayed, referenced, and in-flight GPU resources; apply backpressure before allocation and never block the UI waiting for space.
- Start from the existing 256 MiB presentation-cache budget, now shared, and tune from evidence. Do not allocate all 8 GB VRAM to Fold caches. A 1080p RGBA8 frame is about 7.9 MiB; even 8-bit caches need bounded retention.
- Persistent disk render caches, BC7/other compressed presentation tiers, and proxy generation need measured proposals before implementation. They are not substitutes for a correct bounded hot path. HDR monitoring is separate from the initial 8-bit SDR presentation path.

### Project browser and ingest

- Name the new panel **Project**. It replaces the media-import scaffolding, not the useful selected-object property inspectors.
- Display linked media assets and creative documents together: sequences, compositions, and motion documents organized into bins. Documents retain provider-owned payloads; showing them beside media does not turn them into file assets or move feature schemas into project core.
- Import links to files in place. It registers assets without automatically creating clips, changing an open composition, or selecting a new delivery output. Probing, fingerprinting, thumbnailing, and waveform work stay off the UI thread.
- Persist names, bin hierarchy/membership, and stable item references in project content. Keep expanded rows, filters, panel targets, transports, and layout in workspace/UI state outside authoritative project content.
- Drag media/document items into compatible timelines or compositions using typed stable references. Placement is a separate undoable operation, with explicit destination, time, stream/output, and cycle validation.

### Panel targeting and independent viewers

- Each editor instance has an explicit document target selected through a compact shared breadcrumb/dropdown. List compatible project documents, not only currently open ones. Nested breadcrumbs additionally identify source context and exact mapped local time.
- Each viewer supports **Pinned output** and **Follow editor**. Pinning resolves a stable document/output reference; following resolves a particular editor instance's preview target. Open outputs are convenient menu shortcuts, not the complete set of viewable project outputs.
- Each viewer owns its transport at the bottom of its panel: play/pause, playhead, review range, and loop state. Two viewers can seek and play independently, including when they show the same document. Following an editor chooses a source; it does not imply a shared playback clock.
- Separate viewer binding, transport, editor selection, and persisted delivery output. Focusing a panel or choosing a viewer source must not silently change export output or another viewer's time.
- Route keyframing, timeline seeks, Open Source, shortcuts, and direct overlays through an explicit editor/viewer context. Do not resolve time from whichever viewer updated last.
- Audio-monitoring policy and the fallback when a followed editor closes require an explicit phase-11 decision. Recommended defaults are one explicitly monitored viewer at a time and retaining the last valid output when following ends. Independent transports are agreed; these fallback details are recommendations, not pre-existing user acceptance.

## Implementation baseline

The following is a code/document inspection baseline, not a fresh test or performance certification.

- `crates/project/src/model.rs`, `coordinator.rs`, and `persistence.rs` already provide generic document envelopes, asset IDs/locations/fingerprints, immutable snapshots, atomic edit batches, undo, unknown-data preservation, and atomic Linux save replacement. Persistence is a version-1 JSON archive. There is no first-class bin/item organization model or typed color configuration contract. Preserve these foundations and extend validation/migrations instead of replacing the project store.
- `crates/app/src/media_workflow.rs` retains one/two-layer import scaffolding. `crates/timeline/src/authoring.rs` probes media and creates assets plus clips in the same operation. `crates/timeline/src/ui/inspector.rs` combines MEDIA import controls with SELECTION properties. Split ingest from placement and retain relevant property editing.
- `crates/platform/src/packages.rs` and `lib.rs` provide static provider/command registration and cross-document compilation. Extend their metadata/capability surface for browser items and output discovery rather than teaching the shell concrete feature payloads. Several helpers choose the first matching document; they cannot define explicit multi-document navigation.
- `crates/platform/src/desktop.rs`, `crates/app/src/session.rs`, `session_view.rs`, and `session_transport.rs` expose one desktop viewer state/navigation path and one preview request stream. `Session` has one latest-request mailbox and one desktop playback controller; review ranges are keyed by document. These must become instance-aware, not merely duplicated widgets.
- `crates/ui/src/shell.rs` owns one viewer window, transport, resolution setting, and overlay rectangle. `sdk.rs` registers one panel per contribution ID, not a general instance lifecycle. `desktop.rs` creates the wgpu device/queue and owns one `PreviewHost`. Introduce the necessary host instance/resource seams while retaining feature independence and headless builds.
- `crates/ui/src/preview.rs` already provides a byte-budgeted 256 MiB GPU RGBA8 LRU with displayed/in-flight protection. `PreviewKey` uses content identity but a fixed `view: 1`; output selection assumes the video port. Preserve the good cache behavior, replace fixed identities, and support all viewer consumers.
- `crates/render/src/lib.rs` and `graph.rs` execute CPU full-frame graphs in fixed premultiplied linear-sRGB RGBA32F. Output uses a hard-coded sRGB transfer. `vector.rs` rasterizes through 8-bit linear color/coverage before expansion. This is not an ACES or GPU-evaluation implementation; vector/authored-color precision must be included in the migration.
- `crates/media/src/video.rs` accepts a narrow MP4/H.264, limited BT.709, zero-origin CFR profile. Decode launches FFmpeg for small batches into temporary RGB files, with a 32 MiB read-ahead cache and two worker-owned pinned source copies. The adapter converts to RGB8 sRGB before CPU linearization. Replace this bottleneck without discarding source-change safety, cancellation, or exact timing.
- `crates/app/src/media_workflow.rs` exports through `PreviewKey` and `Frame::to_display()` before `Encoder` converts RGB8 sRGB to BT.709 H.264. Refactor shared scene evaluation away from viewer-specific requests and give export an explicit color/output contract.
- `docs/consolidation-performance.md` records CPU render/decode plus display conversion far above the 33.3 ms frame budget; it explicitly does not establish native UI latency, sustained playback, or aggregate memory safety. Reuse `crates/app/examples/execution_probe.rs` and `scripts/measure_process.py` rather than claiming those measurements cover the new pipeline.
- The archived tracker checks off motion, while `docs/phase-9.md` and `docs/consolidation.md` still list semantic, UI, export, and native-acceptance gaps. Reconcile these records against current behavior/evidence in phase 11; unresolved work remains a named obligation of this plan.

## Baseline and contract slice

- [x] 11. Establish reference evidence and resolve cross-cutting contracts before schema or renderer changes.
  - Inventory supported media/effects, frame/color assumptions, source ownership, per-module budgets, panel targeting, output selection, and deferred acceptance. Map each required change to existing owners; preserve current tests as regression fixtures.
  - Capture a reproducible release-build baseline on the GTX 1070 machine: cold/warm decode, compile/evaluation, display conversion, upload/presentation, encode, process-tree memory, scratch usage, and cancellation latency. Separate headless from native evidence.
  - Select the bundled ACES/config/OCIO versions, default display/view, authored-color and legacy-project policy, and runtime packaging strategy. Prove OCIO CPU processing and GPU shader/LUT integration with the wgpu backend; do not assume generated shaders are directly compatible.
  - Define the minimum contracts for media metadata, project item/bin organization, output enumeration, frame/color/storage readiness, per-panel IDs, viewer requests, independent transports, and workspace persistence. Keep service APIs feature-neutral and limited to demonstrated needs.
  - Decide audio monitoring/clock ownership, editor-to-viewer association, source-switch time policy, and follow-editor closure behavior. Decide initial duplicate-import, bin-delete, referenced-item-delete, and relink policies before UI implementation.
  - Reconcile phase-9 acceptance discrepancies and list each unresolved item with an owning phase or an explicitly approved deferral. Archive status must not erase obligations.
  - Acceptance: an actionable phase document records selected dependencies/policies, runnable baseline fixtures/results, migration risks, and remaining decisions with owners. No later phase starts by silently inventing conflicting semantics.
  - Evidence: [phase-11 contracts and acceptance](docs/phase-11.md) and [native release baseline](docs/phase-11-native.md) record headless/native measurements, pinned OCIO 2.4.2 / ACES 1.3 Studio 2.2.0 CPU–Vulkan shader/LUT comparisons on the GTX 1070, approved policies and inherited phase-9 owners. Checkpoint: 127 tests passed serially, 1 native-audio test ignored; inherited parallel ImGui test conflict and 10 motion Clippy warnings are recorded, not hidden.
- [x] Checkpoint: demonstrate an OCIO-transformed reference image through CPU and GPU paths on the reference machine, record packaging/interop feasibility, and approve the project/viewer contracts. This is integration evidence, not a claim that the production renderer is complete.

## Project organization and ingest slice

- [x] 12. Add persistent project organization and asset-only ingest.
  - Extend the generic project model with stable bin/item identities, display names, and references to assets/documents. Start with a simple tree and one organizational location per item; aliases and smart bins are not required.
  - Validate parentage, cycles, dangling memberships, and deletion policies transactionally. Moving or renaming an item must not change its asset/document identity or render meaning. Do not infer render dependencies from bin membership.
  - Persist reusable source/stream metadata and input-color interpretation through media-owned contracts without leaking timeline/compositor models into project core. Distinguish an imported asset from each use of that asset.
  - Import on workers into a selected bin, reporting unsupported/missing media and cancellation clearly. Separate project linking from the renderer's temporary source-pinning strategy; linked import does not authorize copying originals into managed project storage.
  - Implement the chosen duplicate/relink policies. Relinking preserves references, updates verified content identity and metadata atomically, validates affected uses, and invalidates dependent results without silently changing an export's pinned sources.
  - Add explicit format/schema compatibility and migrations for old projects: existing assets/documents remain discoverable even without bin records. Preserve unknown payload bytes, metadata, dependency records, and conservatively retained resources.
  - Acceptance: asset-only import, bin create/rename/move/delete, relink, undo/redo, cancellation/stale proposals, legacy load, and save/reopen pass focused tests. Import leaves timelines, compositions, viewer bindings, and delivery selection unchanged.
  - Evidence: [phase-12 implementation and acceptance](docs/phase-12.md): archive-v2 organization with deterministic v1 migration, bounded cancellable asset-only import/relink services, media-owned metadata and provider validation; 46 focused/regression tests passed, plus headless/desktop checks. Conservative legacy-metadata/provider safety rejections are documented. Project UI and placement replacement remain phase 13.

- [ ] 13. Replace import scaffolding with the Project panel and reference-based placement.
  - Build a shared-SDK Project browser with bins, media/document items, readable names/type indicators, search, import, and contextual organization actions. Keep selection properties in appropriate inspectors; remove redundant path-entry/import scaffolding once replacement workflows pass.
  - Expose sequence/composition/motion creation and opening through registered capabilities/commands. Do not enumerate feature implementations in the generic shell. Show unavailable-provider and missing-media items honestly.
  - Use asynchronous, bounded thumbnail/waveform work and virtualized lists as needed. Color image thumbnails through an explicit presentation policy; do not let browsing trigger unbounded full-resolution decode or UI-thread I/O.
  - Add typed drag payloads and keyboard/menu placement alternatives. Media-to-timeline creates correctly mapped clips and linked A/V; media-to-compositor creates a source node. Compatible document outputs create references rather than flattened intermediates.
  - Adapt existing timeline import/placement and cross-document commands to reuse already imported AssetIds/DocumentRefs. Handle explicit target track, insertion time, duration, streams, output selection, and dependency cycles; one placement gesture is one undo entry.
  - Keep authored project changes separate from browser selection/expansion state. A bin move or name edit must not flush reusable render content.
  - Acceptance: import → organize → drag into two different documents → rename/move → undo/redo → save/reopen preserves sources and references. Test invalid drops, locked tracks, referenced deletion, missing providers, and narrow-panel/keyboard behavior.
  - Implementation/evidence: [phase-13 implementation and remaining acceptance](docs/phase-13.md). Project grid/tree browser, bounded source previews, asset-only desktop ingest, typed bin/editor drops, provider-owned creation/placement, and reference/history/persistence tests are implemented. Checkpoint: 141 tests passed, 1 native-audio test ignored. **Not complete:** native import/drop acceptance is unverified after desktop-input tool failures; Motion media-input and still-image placement limitations remain explicit, not approved deferrals.
- [ ] Checkpoint: build a short sequence and composite using only the Project workflow, reuse a motion document without flattening it, and verify cross-document undo plus persistence. Run slice checks and native workflow acceptance; no dependency on the retired import UI remains.

## Panel navigation and viewer-instance slice

- [ ] 14. Introduce explicit panel instances and document/output targeting.
  - Separate contribution IDs from runtime panel-instance IDs in the shared SDK/shell. Support multiple viewer instances and the editor-instance context needed to follow a specific editor; do not duplicate feature logic to create instances.
  - Replace first-document/global-selection fallbacks for normal editor targeting with explicit per-instance document bindings. Retain intentional legacy-load behavior only at clearly defined boundaries.
  - Implement one shared compact breadcrumb/dropdown component. Editor menus list compatible project documents; viewer menus offer pinned outputs, follow-editor choices, open-output shortcuts, and all compatible project outputs.
  - Preserve nested navigation and rational local-time mappings. Keep inspectors and overlays associated with the correct editor/document/viewer context; switching focus alone must not retarget unrelated panels.
  - Handle duplicate names, rename, deletion, missing providers, closed editors, and multiple outputs predictably using stable IDs. Keep persisted delivery output independent of all navigation.
  - Store versioned layout/binding state outside project content and tolerate unavailable panels/documents on restore. Browsing or changing panel targets does not create project undo entries.
  - Acceptance: two editor contexts and two viewers can show distinct targets, including two views of the same document. Pin/follow, nested Open Source/back, rename/delete, focus changes, and workspace restore do not cross-wire targets or delivery output.

- [ ] 15. Give each viewer an independent transport and request lifecycle.
  - Replace the global viewer playhead/navigation/range state with viewer-instance state. Review ranges are per viewer, not merely per DocumentId. Retain transport controls at the bottom of each viewer, including loop behavior.
  - Address transport commands and preview requests to an instance. Scope request generations, cancellation, pending results, and stale-presentation rejection per consumer; seeking one viewer must not cancel another viewer or export.
  - Share scheduling/decoding/cache services instead of multiplying worker-owned resources without an aggregate bound. Equivalent requests may share content work while their cancellation/presentation lifetimes remain independent.
  - Implement the approved audio-monitoring policy using one coordinated device service, explicit clock mapping, latency compensation, and safe handover. Unmonitored viewers retain independent playback; no audio I/O, allocation, or blocking work enters the callback.
  - Make editor/viewer association explicit for keyframe insertion, timeline seeks, overlays, and transport shortcuts. Validate behavior when two viewers show the same document at different times and when follow-editor changes source.
  - Acceptance: independently play, pause, seek, loop, retarget, and close two viewers. Verify request isolation, stale-frame rejection, audio monitoring handover, exact edit-time targeting, and bounded teardown. Export remains pinned and unaffected.
- [ ] Checkpoint: simultaneously inspect a motion document and a composite in two viewers with separate bottom transports. Exercise independent seeks/playback, follow/pin changes, direct edits, and closing/reopening panels. Run native and automated instance-isolation tests; do not claim real-time performance until the render checkpoints pass.

## ACES color and GPU-rendering slice

- [ ] 16. Replace fixed color assumptions with production ACES/OCIO semantics.
  - Integrate the pinned bundled configuration and runtime into desktop and CLI packaging. Resolve config resources independently of the current working directory; validate external configs and report missing/incompatible resources without silent substitution.
  - Persist project working-space/config identity, media input assignments, and reproducibility information. Keep viewer display choices in workspace state and delivery transforms in explicit export settings. Include config/LUT content identity rather than only a path or human-readable name.
  - Introduce typed frame descriptors for color/alpha, planes/strides, precision, exact timing, CPU/GPU/external storage, ownership, and readiness. Separate logical evaluation requests from viewer-specific keys so export no longer calls a display-defined evaluation path.
  - Replace hard-coded sRGB conversion in media/render/export with explicit input, working, display, and output processing. Update solids, color controls, compositor parameters, motion fills/strokes, and vector rasterization consistently; do not conceal an 8-bit working-color bottleneck behind float textures.
  - Implement the approved legacy compatibility/migration path. Label old interpretation explicitly, test appearance, and never migrate unknown provider payloads without their provider.
  - Add config-driven input/output/display/view/look controls through the shared UI SDK, keeping secondary color settings in menus/settings rather than filling the viewer toolbar. Show relevant missing-config or interpretation errors visibly.
  - Acceptance: CPU/headless reference fixtures cover input transforms, ACEScg defaults, above-one/negative RGB, alpha edges, authored colors, display/output separation, legacy files, external configs, and missing resources. Matching preview/export settings agree before encoding within declared tolerances.

- [ ] 17. Establish shared GPU resource ownership and implement the core GPU render path.
  - Evolve the device/queue ownership currently in `crates/ui/src/desktop.rs` into a host render-resource service usable by desktop and headless operation without Dear ImGui dependencies. Share the device where practical; panels receive registered presentation handles, not unrestricted device access.
  - Keep authoring models and logical graph semantics independent of physical CPU/GPU execution. Add explicit backend selection, resource lifetimes, synchronization, cancellation boundaries, and transfer accounting without inventing a universal authoring graph.
  - Implement and verify source upload/native-plane conversion, OCIO processing, transforms, opacity, merge, crop, mask, grade, and blur for the existing supported graph. Cover motion/vector output with a measured adapter; any remaining CPU operation must be explicit, bounded, and included in transfer measurements.
  - Keep supported image operations GPU-resident through working evaluation and display conversion. Use pooled working textures with completion-aware reuse; publish display-ready RGBA8 handles without CPU readback in the normal GPU viewer path.
  - Retain a correctness reference/fallback with matching semantics. Declare quality/filter choices and approximation policies; changing backend must not silently bypass an effect or clip working colors. Defer aggressive fusion until equivalence is proven.
  - Handle allocation failure and device loss with invalidation/recovery or a clear stopped-render state. Do not reuse resources based only on CPU reference counts or assume cancellation preempts submitted GPU work.
  - Acceptance: differential CPU/GPU fixtures pass for existing supported operators, OCIO, nested outputs, and alpha edges. Desktop and headless GPU evaluation work on the GTX 1070, with documented precision tolerances, GPU timing, transfer counts, memory use, and failure behavior.
- [ ] Checkpoint: render two composited video sources and a procedural title through the shared ACEScg GPU path; display using OCIO and export using an explicit output transform. Verify no normal-path working-frame readback for viewer presentation, no double transform, and no silent CPU/GPU semantic differences.

## Decode, scheduling, and cache slice

- [ ] 18. Replace batch-process decode bottlenecks and verify codec acceleration.
  - Introduce a retained/range-oriented decoder adapter that avoids launching FFmpeg and writing temporary RGB files for small playback batches. Choose the integration based on phase-11 evidence; preserve cancellation, bounded queues, source verification, and exact seek behavior.
  - Retain decoder-native planes/precision until conversion is required. Keep packet/decode order separate from presentation timestamps; do not disguise VFR as CFR. The existing verified CFR profile remains the initial acceptance baseline; expand formats only with explicit fixtures.
  - Revisit full-source temporary pinning and full-file probe costs with long-source measurements. Preserve immutable-source/export safety while avoiding duplicated whole-file copies per viewer. Source handles, decoded caches, and scratch storage need aggregate accounting.
  - Prove hardware decode support for the selected profile on this Linux/GTX 1070 stack. Validate device identity, plane formats, synchronization, and lifetime for any interop path; record CPU uploads/copies where zero-copy is unavailable.
  - Add thumbnail/waveform/decode work to bounded scheduling rather than allowing each browser job to own an unbounded decoder. Validate cancellation and process/device failures independently of codec success paths.
  - Acceptance: sequential playback, repeated seeks, reverse/random requests, fractional rates, source changes, and missing files remain correct. Report stage timings and copies for software and supported hardware paths; no unsupported acceleration is advertised.

- [ ] 19. Unify bounded scheduling and shared cache policy for multiple consumers.
  - Replace singleton latest-request assumptions with bounded per-consumer demand and shared execution. Reserve interactive capacity for monitored audio and current viewer demands while giving export fair throughput and keeping thumbnails/waveforms background work bounded.
  - Separate compressed/source, decoded, geometry, intermediate, presentation, CPU, GPU, and disk accounting. Enforce aggregate reservations/backpressure across viewers/jobs; keep history/snapshot retention visible in memory measurements too.
  - Extend content keys with stable output ports, exact temporal dependencies, operation versions/parameters, resolution/quality, source/proxy identity, and color configuration. Preserve useful caches across bin/name/layout changes and unrelated edits.
  - Generalize the existing RGBA8 cache's LRU and in-flight protections for multiple displayed consumers. Cache hits must not decode, reapply OCIO, or upload an already resident display image. Separate per-viewer state from shared cache storage.
  - On seek/edit, cancel obsolete queued work, cooperatively stop CPU tasks, and reject stale presentation. Already submitted GPU work may finish; retain reusable content only under valid keys and resource ownership.
  - Define disclosed overload behavior: drop video/reduce explicitly chosen preview quality, never silently omit effects or starve export. Do not fill the entire timeline cache before showing the current frame.
  - Acceptance: two-viewer seek/play/edit loops plus background ingest and export stay within declared aggregate budgets. Verify deduplication, independent cancellation, color-view invalidation, unrelated-edit reuse, eviction safety, fairness, and recovery from allocation pressure.
- [ ] Checkpoint: measure the declared two-layer/title/stereo-audio workload at 1080p30 on the reference machine. Target sustained playback, UI frame-time P95 below 16.7 ms, and warm presentation-cache seeks within 100 ms. Record cold/warm conditions, quality, drops, underruns, queue depth, hit rates, transfer bytes, CPU/GPU/scratch peaks, and export throughput. Separately report two-viewer behavior; do not substitute half-resolution, cached-only playback, or short headless averages for full-quality sustained evidence. Missed targets require a named corrective phase or explicit approved target revision.

## Delivery and integration-hardening slice

- [ ] 20. Finish color-correct reproducible delivery and encoding.
  - Export from one committed snapshot and explicit document/output/range/config/profile, independently of viewers and transient gestures. Preflight the dependency closure, sources, fonts, providers, color resources, and supported encoder settings before creating a final destination.
  - Apply the selected output transform to scene-linear results, then encode with correct transfer/primaries/matrix/range metadata. Remove the current export dependence on `PreviewKey` and `to_display()`; never source export from the 8-bit viewer cache.
  - Preserve ordered video/audio outputs, rational half-open ranges, latency/time mapping, cancellation, temporary destinations, and atomic/no-clobber completion behavior. Validate encoded timing/audio separately from pre-encode pixel equivalence.
  - Evaluate and expose supported hardware encoding only after quality, metadata, synchronization, copy cost, and throughput tests on the reference machine. A hardware encoder is not automatically the highest-quality default; retain a verified software profile.
  - Record rendering/config/environment/source identities needed to explain output. Missing or changed dependencies must fail clearly unless the user explicitly chooses a substitute.
  - Acceptance: desktop and CLI produce equivalent pre-encode images under pinned settings; encoded round trips pass color/timing/audio checks. Two active viewers cannot alter an export's source, range, transform, or revision. Cancellation/failure leaves no misleading completed output.

- [ ] 21. Close inherited correctness, recovery, and creative-workflow gaps.
  - Resolve the outstanding phase-9 semantic/direct-authoring/export/acceptance items identified in phase 11, or obtain and record explicit scope decisions. Preserve out-of-order motion evaluation, editable nested references, cross-document undo, and one-entry gesture commits.
  - Test save interruption, recovery snapshots/autosave policy, migrations, unknown providers/payloads, relink, missing fonts/media/configs, stale worker proposals, decode/encode failure, device loss, OOM/backpressure, and shutdown with active work.
  - Exercise long media, repeated import/delete/undo, multiple viewers, edit/seek/export loops, and workspace restore. Verify that closing panels releases consumer state without evicting in-use shared resources or destroying project content.
  - Run ten-minute mixed-rate playback with latency-compensated A/V offset initially bounded to one output frame and no accumulating drift. Include monitor handover and independent unmonitored viewer playback.
  - Review UI at normal/narrow sizes and 100/150/200 percent DPI where supported: compact chrome, readable names, keyboard/focus routing, drag alternatives, explicit source/time context, quality/pending/missing states, and correct inspector/overlay targeting. Report unsupported IME/accessibility behavior rather than implying it is solved.
  - Acceptance: tracked failures introduced by this plan are fixed; inherited gaps have evidence or explicitly approved deferrals. Full workspace tests/checks, formatting, Clippy, boundary checks, desktop/headless packaging checks, and native workflow acceptance are recorded.
- [ ] Checkpoint: import linked media → organize bins → create/edit a sequence → composite → animate → inspect two outputs with independent viewers/transports → save/reopen → export from desktop and CLI. Use the bundled ACES configuration and ACEScg default, preserve editability and undo, verify missing-dependency/recovery behavior, and publish reference-machine performance/resource evidence. The new plan is complete only when this workflow and its acceptance suite pass; retiring the old plan is not a substitute.
