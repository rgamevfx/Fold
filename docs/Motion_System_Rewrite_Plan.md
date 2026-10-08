# Motion System Rewrite Plan

**Date:** 8 October 2026  
**Status:** Active implementation plan; implementation has not started under this plan.  
**Direction:** [Motion architecture and node-set proposal](Fold_Motion_Architecture_and_Node_Set_Proposal.md).  
**Replaces:** The motion and agent roadmap in [retired dev-plan.md](../dev-plan.md), earlier motion proposals, and their implementation/qualification instructions. [Retirement register](Retired_Documentation.md) defines the scope.

## 1. Mandate and precedence

Rewrite Fold's motion system and all MCP/agent contracts around it. The result has one native motion document, one typed language, one operation registry, one evaluator, and one public authoring protocol. Bevy supplies spatial rendering behind a Fold adapter; Parley supplies text layout behind a Fold service. Qualify a vector backend and a bounded USD adapter as described in the proposal.

**Delete the superseded implementation. No legacy motion engine, old-protocol aliases, compatibility shims, alternate legacy evaluation path, or fallback to Rust assets/Wrangles/Solver may remain in the shipped application.** Preserve old source through Git history, not compiled features or dormant replacement directories.

This instruction overrides proposal §14's recommendation to retain the existing engine until replacement acceptance, §5.1's option to retain Rust builders, and any earlier compatibility or phase-completion requirement. A future Rust extension API must use the new registry and semantics; retaining the old builder SDK is outside scope.

The proposal owns target semantics and operation vocabulary. This plan owns sequencing, removal requirements, acceptance and status. Shared product rules still apply: independent Timeline/Compositor/Motion packages, transactional project edits, immutable evaluation snapshots, rational time, explicit color/alpha/resource ownership, feature-neutral infrastructure, and Dear ImGui through the UI SDK. Bevy's ECS is reconstructible rendering state, never project authority or the document clock.

The old roadmap is frozen. Unfinished unrelated work is deferred, not accepted or silently added to this rewrite. Fix a shared issue here only when it blocks a rewrite gate, and record that dependency.

## 2. Rewrite boundary

Source entry points below were inspected for planning; Phase M0 must enumerate their callers and dependencies before removal. A path is a starting point, not a complete deletion manifest.

| Area | Starting points | Required disposition |
| --- | --- | --- |
| Motion document, operations and execution | `crates/motion/src/{document,graph,authoring,evaluation,fields,geometry,nodes,scene,canvas,animation,prepared}` | Replace old models, registries, evaluation and preparation; recreate needed behavior from new contracts |
| Programmable motion and simulation | `crates/motion/src/{program,wrangle,simulation}`, associated examples/tests | Remove old Rust asset compiler/SDK/bundles, WASM execution, Wrangles and Solver; initial motion is stateless |
| Motion UI and registration | `crates/motion/src/{ui,package}`, `crates/app/src/packages.rs` | Remove old node/inspector/creation registrations; expose only qualified new operations |
| Application coupling | `crates/app/src/{rust_asset,image_bake,composition_layers,document_placement}.rs`, output/session integration | Remove old motion commands and concrete model dependencies; reconnect new provider outputs through explicit services |
| Agent contract and adapters | `crates/app/src/automation/`, `session_automation.rs`, `session_candidate.rs`, `crates/motion/src/automation.rs` | Replace motion schemas, wire contract, authoring SDKs, candidate/observation contracts and MCP/CLI exposure; audit every transport method |
| Shared authoring infrastructure | `crates/platform/src/authoring*`, project transactions, app session/jobs | Retain only audited feature-neutral services that meet the new contract; remove motion-specific assumptions and obsolete protocol glue |
| Rendering | Motion consumers in `crates/render`, GPU coordinator, viewer and delivery integration | Remove old motion lowering/preparation; preserve independently used compositor, media and color behavior |
| Shared animation/Composition | `crates/animation`, `crates/composition`, Timeline/provider animation capabilities | Keep general curves and layer/clip services where useful; new Motion tracks edit its native signals; remove duplicate motion ownership |
| Public material and dependencies | `skills/fold`, installed skill entry, demos, scripts, generated schemas, manifests and lockfile | Replace obsolete authoring examples and clients, remove dead dependencies and old fixtures; regenerate from the new registry |

The protocol audit includes generic methods previously used for motion, not just methods whose names contain “motion.” Each existing MCP/CLI method gets a disposition: implement under the new contract, retain as an independently justified non-motion service, or delete. Reusing transport plumbing does not justify preserving old request semantics. Shared non-motion authoring must use an explicitly supported contract after the cutover; no hidden compatibility adapter.

### Project files and historical artwork

Use a distinct new motion document/schema identity so old payloads cannot be misinterpreted as new constructions. The project layer continues preserving unsupported opaque documents and unknown extension data. Opening an old motion document reports an unsupported provider/schema and cannot execute it. This is generic project preservation, not a legacy motion reader.

Automatic old-motion migration is excluded. Keep original user projects untouched. Rebuild selected repository demos with the new public surface; remove obsolete shipped fixtures and instructions when replacing them. If conversion is later requested, scope it separately as an offline tool with explicit loss reports, without adding old-engine code to the application. New-schema version evolution and unknown-operation preservation remain required.

## 3. Delivery sequence and tracking

Phases M0–M9 are ordered. M4 and M5 may prepare independent parts after M3, but M6 requires both. Finish each gate before claiming its dependent phase complete. This document is the status ledger; add evidence links here rather than resuming old numbered phases.

| Phase | Deliverable | Proposal slice | Status |
| --- | --- | --- | --- |
| M0 | Complete removal inventory and frozen contracts | Preparation | Pending |
| M1 | Old engine/protocol removed; application builds without motion | Rewrite cutover | Pending |
| M2 | New document, registry, language and reference evaluator | A | Pending |
| M3 | Native editing and new MCP/CLI round trip | A | Pending |
| M4 | Qualified Parley/vector/Bevy rendering boundary | B | Pending |
| M5 | Integrated motion-design construction | B | Pending |
| M6 | Incremental execution, GPU fields and full observation loop | C | Pending |
| M7 | Production operation modes, assets and images | D | Pending |
| M8 | USD and production delivery | D | Pending |
| M9 | Final acceptance, documentation and deletion audit | Release | Pending |

Each phase record must list the implementing commits, qualified operation modes, removed surfaces, commands/results, visual/runtime evidence and unresolved failures. “Implemented,” “automated checks passed,” and “artist/agent acceptance passed” are separate claims. Old acceptance results cannot qualify new code.

### M0 — Inventory and freeze the first contracts

**Work**

- Trace the table in §2 through workspace imports, registrations, generated code, tool clients, skill distribution, fixtures and packaging. Record every old subsystem and its deletion owner/phase, including dynamic registrations and embedded guide text.
- Write the target ownership map and dependency diagram. Keep conceptual services inside focused modules unless a real dependency boundary requires a crate. Inventory shared services by independent consumers before retaining them.
- Specify document/schema identities, operation versions, stable IDs, typed values/signals/fields/attributes, geometry domains, clocks, spaces and provenance. Define how Motion outputs enter Composition/Timeline and compositor image inputs without importing Motion models into infrastructure.
- Freeze the minimal M2/M3 operation subset and the exact M5 visual scene. Create a catalog coverage table from proposal §12: each of the 28 artist operations and 22 foundation operations has its key, modes, domains, dependencies, target phase and acceptance fixtures. Keep the eight later candidates separate.
- Define a new protocol version and capability discovery, scoped proposals, concurrency guards, candidate lifetime, independent observations, atomic apply, cancellation, errors and limits. Specify client/version rejection, session permission boundaries, retry/idempotency and job retention; do not inherit these accidentally from the old protocol.
- Record dependency experiments for Bevy, Parley, vector rendering and USD: exact candidate versions, features, licenses, native libraries, build/package support and GPU compatibility. The current render/UI manifests use wgpu 27; treat that as an integration constraint to investigate, not a required new version. Use official documentation and executable evidence when selecting versions.
- Freeze benchmark scenes, hardware, quality/color configuration, timing boundaries and acceptance tolerances before optimization. Record inherited shared failures separately.

**Gate:** Every old surface has a disposition, every initial catalog operation has a planned home, and the first vertical slice has executable acceptance specifications. Dependency uncertainty has a bounded experiment and a decision owner. No claim of backend compatibility is based only on a dependency's API description.

### M1 — Remove the old motion system and protocol

**Depends on:** M0.

**Work**

- Delete the old motion implementation and its registrations, UI, builders, Wrangles, Solver, compiler/toolchain integration, specialized runtime paths and obsolete tests. Remove motion-only dependencies from manifests/lockfile.
- Delete old motion authoring/protocol schemas, SDK generation, command dispatch, candidate glue, guides and scripts. Rework shared automation only where the M0 inventory establishes an independent consumer. Remove protocol aliases and backward-version dispatch.
- Make the application build and run with motion unavailable. Hide unavailable creation/actions; retain explicit diagnostics when an existing document refers to an unavailable provider. Avoid placeholder success responses, blank renders presented as success, and accidental reinterpretation of old documents.
- Keep needed non-motion project, Composition, Timeline, compositor, media, viewer and delivery services operational. Replace any direct motion dependency in those services with a genuine feature-neutral boundary or remove the obsolete coupling.
- Retire obsolete shipped demo fixtures without overwriting user artwork. The prior implementation remains available only through version control.

**Gate:** A buildable cutover has no registered or executable old motion engine or old motion protocol, and non-motion smoke workflows still function. An old motion project is preserved opaquely with a clear unsupported diagnostic. Record removed symbols/files/packages and search/build evidence. Temporary loss of motion functionality is intentional; it does not justify retaining a fallback.

### M2 — Native document, typed language and reference evaluation

**Depends on:** M1.

**Work**

- Implement the versioned motion document and single operation registry. Generate language declarations, UI metadata, public schemas and reference entries from registry contracts. Validate defaults, units, spaces, domains, versions, cost classes and execution capabilities.
- Implement native identity, immutable revisions, serialization, presentation metadata, reusable asset definitions/instances, named inputs/outputs, protected controls and source mappings. Integrate edits with project transactions and undo.
- Implement parser, type inference, bounded expressions, diagnostics, canonical printing and scoped reconciliation. Preserve unknown operation records as unresolved. Names/layout changes retain identity; ID-free fragments create new identities unless mapped explicitly. Invalid source retains its buffer and leaves the last valid revision intact.
- Implement a CPU reference evaluator with explicit rational time, a common signal/field IR, cycle rejection, deterministic ID-based random policy, resource budgets and cancellation. Define out-of-range time, rotations, local asset clocks and channel combination semantics.
- Prove the smallest animated shape construction with typed inputs, an expression, Tween/Keyframes, placement and named output. This uses new data and execution throughout. Rendering qualification follows in M4.

**Gate:** Parse → document → canonical source → parse preserves supported meaning and identities. Save/reopen, undo/redo, asset overrides, invalid edits, cycle errors and unknown records behave as specified. Evaluated samples agree regardless of seek order. No legacy SDK or full Rust build participates in source edits.

### M3 — Shared native editing and replacement agent protocol

**Depends on:** M2.

**Work**

- Build node/control/source and timeline editing over the same transaction service. Apply the Fold UI skill before implementation. Keep graph positions, groups, annotations, protected controls and track identities stable through agent edits.
- Implement Discover, Inspect, Propose, Observe, Apply and Cancel capabilities from proposal §10 over new versioned MCP and CLI adapters. These are capability names until the concrete schema is frozen; document actual endpoint names only after implementation.
- Provide scoped source and structured patches through the same validation path. Support base revision and expected values, disjoint rebasing, explicit conflicts, immutable candidate generations, semantic diffs, idempotent retry, bounded job lifetime and cancellation.
- Implement initial observation metadata and CPU measurements. Frames become available with M4; discovery must report unavailable capabilities honestly. Candidate apply checks the exact inspected generation and commits one meaningful undo group.
- Keep agent observation time and selection independent of the artist. Invalid or stale proposals cannot overwrite the current document or last valid presentation. Locks survive source regeneration and parent replacement.
- Replace active skill instructions, generated guides and minimal examples with verified new-protocol behavior. Public workflows must not require reading Rust crates or using private APIs. Remove old protocol recipes from shipped guides.

**Gate:** Source edit → native graph edit → timeline key edit → disjoint agent patch → undo/redo → save/reopen preserves intent, identity, layout and overrides. Same-property edits conflict; locked controls resist indirect replacement. MCP and CLI exercise the same behavior, reject old contracts and expose matching schemas. Rendering completes this handoff gate in M5.

### M4 — Text and rendering integration

**Depends on:** M3; use the M0 experiments to select and pin dependencies.

**Work**

- Wrap Parley in a Fold-owned text service. Retain one normalized layout with logical/glyph mappings, actual font resolution and content identity. Feed vector drawing, spatial preparation, outlines and inspection from that same layout.
- Qualify and select the vector backend behind an adapter using holes/fill rules, joins/caps, clipping, thin strokes, overlaps and zoom. Preserve source paths and native text; tessellation and glyph buffers remain caches.
- Integrate selected Bevy plugins for offscreen spatial rendering with Fold-supplied time. Retain geometry/material/instance resources across requests; use packed instance data where appropriate. Rebuilding an ECS world is recovery behavior, not the frame path.
- Resolve the GPU ownership route with explicit device/queue compatibility, submission/completion, texture access, depth/picking, readback, budgets and device recovery. If shared resources cannot be qualified, revise the boundary with measured transfer/lifetime costs before proceeding; do not route through the deleted renderer.
- Qualify Screen, Camera Plane and World placement, depth-compatible spatial text/vector preparation, ordered screen artwork, lighting, linear working color, premultiplied alpha, transparency and exactly one display conversion. Reconcile spatial lighting with Fold's existing color services.
- Deliver revision/time/camera-bound frames from the same rendering path to viewer, agent and headless output. Specify reset/warm-up for history-dependent effects and shutter sampling.

**Gate:** One retained text layout produces matching vector/spatial/outline results. Mixed screen/world scenes have checked depth, color and alpha at declared tolerances. Rapid edits cannot present stale frames or release in-flight GPU resources. Headless and desktop paths agree under the same profile. Record selected dependency versions and evidence, including unsupported cases.

### M5 — Complete visual and artist–agent proof

**Depends on:** M3 and M4.

**Work**

- Implement only the operation modes needed for proposal §13.1–13.2: Text, Text Animator, Fill/Stroke, Outline Text, Extrude, Material, distribution/Cloner, Animator, Field/Ramp/Random, time signals, Place, Scene, Camera, Light and publication/reuse support.
- Prove planar contours with holes and explicit tessellation/attribute transfer, stable instance identity and random values, geometry sharing, text selection/group pivots, staggering and explicit deformation sampling basis.
- Assemble one scene containing kinetic type and its extruded counterpart, a field-driven cloner, camera/light and ordered screen graphics. Expose wording, palette, timing, spacing/count, field and camera controls.
- Perform the proposal §13.4 handoff through public tools: artist edits ramp, key, lock and layout; agent changes field direction/timing; disjoint edits survive and true conflicts are surfaced. Save/reopen and continue from the saved construction.
- Rebuild a small reference demo through the new protocol and document verified controls and examples in the Fold skill. Inspect multiple times and control variations, not just a hero frame.

**Gate:** The integrated scene is visibly correct, procedurally editable and reproducible through native UI, MCP, CLI and headless rendering. Source, graph and timeline continue to represent the same construction after human edits. No opaque replacement image satisfies procedural acceptance.

### M6 — Incremental execution, performance and observation

**Depends on:** M5.

**Work**

- Implement dependency-aware compilation and caches for source/layout, topology, static attributes, time samples, camera/output preparation and presentation metadata. A graph layout edit must not trigger geometry work; color edits must not reshape text.
- Compile/fuse supported field IR to CPU/GPU execution with explicit domain/space conversion and fusion boundaries. Keep integer IDs exact and compare numeric results against the new CPU reference evaluator with declared tolerances.
- Retain GPU buffers, account for memory and transfer costs, expose fallbacks, prioritize interactive work, cancel obsolete CPU work and bound already-submitted GPU work. Keep observation/export progress fair under sustained editing.
- Complete frames, crops, contact sheets, clips, text/font inspection, attribute sampling, counts/bounds/visibility, supported object-ID/depth passes, motion trails and per-operation cost/cache diagnostics. Include revision, candidate generation, time, camera, quality, seed and asset/font/configuration provenance. Mark sampled and approximate results.
- Measure cold and warm performance separately for 1k/10k/100k instance scenes, changing text, overlap, deformation and mixed scenes. Record P50/P95/P99 edit-to-present and observation latency, missed presentations, memory and stalls. Qualify the proposal's roughly 100 ms small-source-edit, few-hundred-ms frame observation and 1080p60 targets on named scenes/hardware; counts alone imply no throughput.

**Gate:** Documented targets pass on the frozen benchmark or have an explicit revised target supported by evidence; an unexplained miss keeps acceptance open. Reverse/random seeks, repeated shutter samples, rapid edits, cancellation, low-memory behavior, device recovery and concurrent observation/export pass. No silent clone reduction, missing geometry or font substitution is used to achieve performance.

### M7 — Production authoring, assets and operation coverage

**Depends on:** M6.

**Work**

- Complete the M0 catalog matrix across all 28 artist and 22 foundation operations. Qualify modes individually, including geometry refinements, mixed collections, selections, conversions, attribute/domain transfer and identity policies. An advertised operation cannot consist of a placeholder or silently narrower contract.
- Complete image resources/source-time sampling, distribution and image-driven relief, Fill/Stroke/Clip/Trim policies, layouts, geometry preparation, materials and nested reusable assets. Implement explicit upstream Time Remap separately from per-channel stagger, with cost diagnostics.
- Qualify ligatures, combining marks, bidirectional text, fallback, variable fonts, line wrapping, logical selectors and reshaping correspondence. Surface ambiguous identity changes and unsupported color-glyph extrusion.
- Finish asset packaging, content/font references, dependency resolution, per-instance overrides, protected inputs, new-schema migrations and missing/unknown-version diagnostics. Native projects must evaluate without arbitrary authoring scripts.
- Rebuild required motion demos and public workflows; execute their examples against the new catalog. Keep native editing and inspection available for everything the agent creates.

**Gate:** Every initial operation and required mode is qualified or explicitly deferred in an amended scope table with rationale; a deferment is not completion of the original 50-operation scope. Held-out briefs cover kinetic type, field cloning and image relief, with visual correction and artist continuation. Packaged projects reopen reproducibly, including explicit missing-dependency outcomes.

### M8 — USD, export and production integration

**Depends on:** M7.

**Work**

- Implement the bounded USD adapter selected by the M0/M4 dependency work. Qualify Rust/native access, deployment, threading and errors; define the supported schema/material subset and versions.
- Import supported composed assets/references/variants through ordinary typed Asset inputs. Preserve external identity correspondence separately from native document and element identities. Qualify units/axes and resource resolution.
- Export supported evaluated geometry, instances, cameras, materials and sampled animation. Require explicit bake policies and loss reports for unsupported procedural semantics. Preserve the native Fold document as the editable source.
- Complete Motion output integration through Composition, compositor and Timeline with correct local/source times, format, alpha and color. Qualify preview/export/headless agreement, shutter samples, interruption, reopening, missing assets and independent viewer/agent clocks.
- Package the chosen rendering/text/USD dependencies and fonts with documented platform/feature support. Keep OpenExec outside the baseline evaluator; it needs separate evidence and scope before adoption.

**Gate:** Supported USD round trips meet declared preservation rules and unsupported data produces actionable reports. Native procedural round trips remain independent of USD. End-to-end projects render and export through the supported application modes with documented recovery and resource behavior.

### M9 — Release qualification and final removal audit

**Depends on:** M8.

**Work**

- Close the proposal §15 evidence matrix: document/language, concurrent editing, text, vectors, spatial rendering, animation, scale, GPU integration, observations, USD and agent outcomes.
- Run held-out public-tool tasks and reference reconstructions; measure visual quality, editable structure, useful controls, semantic graph size, completion and correction efficiency. Include native artist continuation and conflicts.
- Audit the complete M0 removal manifest against source, Cargo dependencies/features, registrations, serialized schemas, tools/SDKs, generated docs, embedded strings, scripts, examples, test fixtures and packaging. Remove transitional stubs and dead paths. Check old versions reject rather than route to adapters.
- Rewrite current architecture/product motion sections to match the accepted implementation and replace skill references with executable, verified examples. Keep historical records clearly retired; active entry points must lead to current contracts.
- Run the workspace checkpoint and supported platform/package checks. Separate new regressions from inherited failures; a relevant unresolved failure blocks release acceptance.

**Gate:** One implementation and protocol serve all supported motion workflows; zero executable legacy motion paths or shims remain. Every claimed capability links to fresh acceptance evidence, and every limitation is explicit. The app is artist-editable and agent-authorable after save/reopen, with consistent preview/export behavior.

## 4. Validation discipline

During each phase, format touched Rust files and check affected crates. Add focused behavioral tests for changed semantics; test the document/IR independently of UI and use image/runtime fixtures for rendering contracts. Compare CPU/GPU at declared tolerances rather than silently relaxing them. Exercise examples through real public tools.

At M1, M3, M5 and M9 checkpoints, run applicable workspace boundary, format, check, test and Clippy checks documented in the repository, plus targeted native/headless workflows. A non-rendering phase cannot claim visual acceptance. Physical presentation, native interaction, headless output and automated unit results are distinct evidence. Record actual commands, dependency environment, failures and unverified cases.

For each acceptance run, retain source/project revision, protocol/operation versions, seed/font/assets, exact times, output settings, hardware and dependency versions beside the results. Performance evidence must separate parse/validate/compile/evaluate/prepare/submit/present/readback/encode costs.

## 5. Scope after the rewrite

The eight later candidates—Scatter, Sample Geometry, Offset Path, Sweep, Bevel, Subdivide, Morph and Text on Points—are a follow-on expansion (proposal Slice E), each requiring its own topology, attribute-transfer, identity and performance evidence. Physics, fluids, character rigging, arbitrary solvers, boolean modeling and general camera-silhouette outlining remain outside the initial catalog. Do not resurrect legacy systems to fill these gaps.

Unrelated historical obligations (including EXR throughput, compositor discrepancies, delivery hardening and Phase 57 work) remain in the retired ledger as deferred history. Reprioritize them explicitly after the rewrite or when a concrete dependency blocks a phase; old phase numbering supplies neither authorization to expand this plan nor proof of acceptance.
