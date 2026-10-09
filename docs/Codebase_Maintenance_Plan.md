# Fold Codebase Maintenance Plan

**Date:** 9 October 2026
**Status:** C0 baseline complete, with native measurement failures recorded. C1–C4 complete; desktop/native acceptance limitations remain recorded. C5–C9 not started.
**Audit baseline:** `6280fb6` — Implement M5c parameter references and scoped inspection.

## Purpose and authority

Establish a clean foundation for continued development by deleting superseded behavior, restoring provider ownership, simplifying application state, and measuring performance before optimization. This plan converts the repository audit into independently reviewable phases.

This is a separate, user-requested maintenance workstream. The [Motion System Rewrite Plan](Motion_System_Rewrite_Plan.md) continues to own Motion semantics, phase order, capability qualification, and artist/agent acceptance. Maintenance does not close or waive its gates. Changes shared with that workstream must update both plans' evidence without duplicating implementation tasks.

M3–M5 and M5a–M5c acceptance remain open. The desktop freeze remains unresolved. Full incremental Motion execution belongs to M6 and cannot begin until its prerequisites pass. The [product specification](Fold_Application_Product_Architecture.md) supplies shared non-motion requirements, subject to the current user instruction to remove obsolete implementations and compatibility shims.

## Rules for every phase

- **No additional visual feedback or UI noise.** Do not add labels, indicators, badges, status text, popups, notifications, diagnostic panels, or other visible feedback as part of this maintenance work. Typed failures are internal Rust error variants; explicit capabilities are internal registration/discovery contracts. Neither authorizes new artist-facing UI. Diagnostics and baseline instrumentation belong in opt-in logs, tests, and evidence files. Preserve existing error handling without adding presentation surfaces. This explicit user constraint takes precedence over more permissive UI guidance.
- Delete a replaced implementation, its registrations, dependencies, obsolete tests, and public guidance in the same completed phase. Git preserves historical source. Do not ship old/new switches, aliases, migration readers, or compatibility wrappers.
- Preserve user artwork and unsupported project/provider payloads through generic opaque handling. Never reinterpret old payloads as current schemas or overwrite original artwork to make a test pass.
- Preserve rational time, explicit color/alpha/resource ownership, immutable snapshots, stable IDs, transaction/undo authority, and independent creative packages.
- Keep the current reference evaluator and independently useful rendering/media algorithms unless the phase proves they are superseded. A name containing “legacy” or a large line count is not sufficient evidence for deletion.
- Keep one implementation of each behavior. Introduce modules around state ownership and useful interfaces; moving a large `impl` across files alone does not satisfy a phase.
- Read the Fold UI skill before UI implementation. When changing public authoring or demos, update the Fold agent skill and verify examples through public tools.
- Keep changes scoped to the phase. Do not introduce a dynamic plugin framework, a general-purpose job framework, or new crates without demonstrated need.
- Record implementation, automated verification, and native artist/agent acceptance separately. Unknown performance and unresolved failures remain explicit.

## Phase sequence

| Phase | Outcome | Depends on | Status |
| --- | --- | --- | --- |
| C0 | Baseline, deletion inventory, and acceptance fixtures | None | Complete; native failures remain open |
| C1 | One supported generation of documents and operations | C0 | Complete; see evidence and limitations |
| C2 | Typed failures and explicit provider capabilities | C1 | Complete; see evidence and limitations |
| C3 | Prepared Compositor representation | C2 | Complete; see evidence and limitations |
| C4 | Provider-owned agent editing | C3 | Complete; see evidence and limitations |
| C5 | Focused application coordinator | C4 | Planned |
| C6 | Explicit desktop workspace and viewer state | C5 | Planned |
| C7 | Simplified GPU execution and vector path | C6 | Planned |
| C8 | Measured preparation and refresh optimization | C7; M6 gate for incremental Motion work | Planned |
| C9 | Integrated qualification and current architecture reference | C8 | Planned |

Use this as the default execution order. A phase may contain several small commits, but its gate must pass before dependent work is called complete. Planning this sequence does not authorize claiming unfinished rewrite behavior as implemented.

## C0 — Establish the baseline

**Work**

1. Record the starting commit, working-tree changes, build configuration, hardware, native runtimes, and current rewrite obligations. Protect open application sessions and unsaved artwork.
2. Trace every proposed deletion through registrations, call sites, schemas, CLI options, fixtures, scripts, manifests, and documentation. For each surface, record its replacement or explicit unsupported disposition.
3. Capture representative workflows: media ingest and placement; Timeline edit; Composition with Motion and Compositor layers; parameter/keyframe edit; agent proposal/inspection/apply; undo/redo; save/reopen; preview/export.
4. Establish measurements for edit-to-present latency, UI refresh, document decode/validation/preparation, allocations, memory retention, and cancellation. Name scene sizes, sample times, warm/cold conditions, and rendering tolerances before comparisons.
5. Record the desktop-freeze incident as an unresolved acceptance case. Instrument relevant event and worker transitions; do not attribute it to a module without a reproduction or causal evidence.

**Gate:** The deletion inventory has no unassigned callers. Supported behavior has a reproducible baseline and agreed comparison criteria. Existing failures and unavailable native checks are recorded separately from new regressions.

The audit's package-boundary/headless-isolation check and nine boundary-policy tests passed. Those results are limited baseline evidence, not full application qualification.

**C0 completion — 9 October 2026:** [Results and limitations](maintenance/evidence/c0/result.md), [deletion inventory](maintenance/c0-inventory.md), [workflow reproduction](maintenance/c0-workflows.md), and [frozen measurement profile](maintenance/c0-profile.json). Assigned 1,403 references across eight groups and inventoried 15 unchanged project archives. Recorded 84 passing Rust tests, 16 passing Python tests, focused Clippy with existing warnings, and three release benchmark runs. Implementation and evidence are recorded together in the C0 baseline commit.

Native desktop captures failed with SIGSEGV in release and debug; a control also failed with the runtime probe disabled. The cause is unassigned, and a relationship to the earlier freeze is unproven. Native edit-to-present and preview GPU allocation measurements are unavailable; CPU allocator counts are also unmeasured. C0 records these failures under its gate rather than claiming native acceptance. Investigate before C6 native acceptance and establish successful measurements before C8 performance claims. At C0 completion, no new visual feedback had been added and C1 had not begun.

## C1 — Delete superseded generations

**Primary files:** `timeline/src/video.rs`, `timeline/src/package.rs`, `app/src/media_workflow.rs`, `app/src/session.rs`, desktop/CLI entry points, `compositor/src/registry.rs`, `compositor/src/kernel.rs`, provider serialization, and workspace migration code under `crates/`.

**Work**

1. Delete `VideoLayers`, `LegacyLayers`, their provider registrations, two-layer import/export branches, `--layers`, and legacy opacity commands. Rebuild required repository fixtures using current Sequence/Composition documents.
2. Select one current executable schema per provider. Remove Timeline's old-schema branches and Composition's multiple-generation writer/reader behavior. Re-author repository fixtures explicitly; unsupported user payloads remain opaque.
3. Consolidate Grade, Merge, Transform, and Blur operation generations. Retain an older algorithm only if it has an independently specified creative purpose, represented by an explicit current operation or mode. Do not silently substitute algorithms with different pixels.
4. Remove old workspace-version upgrades and retired panel-name rewrites. Unsupported workspace sidecars receive a current default layout without modifying project content.
5. Audit project archive v1/v2 handling separately from provider schemas. Remove historical interpretation/migration branches while retaining the generic opaque-document and unknown-extension guarantees of the supported archive format. Unsupported archive containers fail explicitly and remain untouched.
6. Remove obsolete tests and dependencies; keep meaningful current-behavior tests and negative tests proving old providers/versions are unavailable.

**Gate:** Discovery and runtime dispatch expose only the selected current contracts. Current fixtures save/reopen and render correctly. Unsupported documents round-trip opaquely without execution; unsupported archive containers are not rewritten. Searches confirm deleted symbols have no executable registrations or callers.

**Checkpoint:** Workspace check, full applicable tests, Clippy, package boundaries, and focused save/reopen plus media workflow acceptance.

**C1 completion — 9 October 2026:** [Results and limitations](maintenance/evidence/c1/result.md). Removed two-layer execution, old provider schemas/operations, archive interpretations and workspace upgrades. Re-authored current fixtures through public tools. Workspace check, Clippy and boundaries passed; 760 tests pass after correcting two obsolete test expectations and explicitly running three native fixture tests. 114 other tests remain ignored. C0 desktop failures and Motion acceptance gates remain open; no C2 or M6 work was performed.

## C2 — Establish typed failures and provider capabilities

**UI constraint:** Internal code and machine-readable contracts only. No new labels, indicators, text, popups, or other visible feedback.

**Primary files:** `platform/src/packages.rs`, provider registrations, `app/src/output.rs`, `app/src/preview_worker.rs`, and `ui/src/media_thumbnails.rs`.

**Work**

1. Replace scheduling decisions based on error strings with typed failures for cancellation, temporary resource pressure, invalid input, missing resources, unsupported capability, and execution failure. Keep explanatory text and contextual IDs alongside the category.
2. Distinguish retryable pressure from a permanently oversized request. Preserve bounded retries, cancellation, and admission limits.
3. Reduce the required document interface to identity, support, and validation responsibilities. Register cohesive optional capabilities for authoring, parameters/animation, hierarchy, placement/settings, and image inputs as justified by actual consumers.
4. Replace empty-success defaults that obscure missing capabilities with explicit discovery and unsupported results. Keep static package assembly.
5. Specify the provider-owned scoped proposal interface needed by C4, including preparation, inspection, reconciliation, protection, and diagnostics. Preserve project ownership of candidate lifetime and atomic publication.

**Gate:** Error-message wording cannot affect retry behavior. Capability discovery agrees with actual support. A provider implementing only its required capabilities remains usable. Existing cancellation, conflict, and resource-pressure behavior passes focused tests.

**C2 completion — 9 October 2026:** [Results and limitations](maintenance/evidence/c2/result.md). Typed failures now survive resource admission, nested compilation and evaluation; retry decisions use categories. Optional provider traits replace implicit empty-success defaults, with explicit discovery and minimal-provider coverage. 84 focused tests and affected desktop/headless checks passed. [C4 scoped proposal specification](maintenance/c4-scoped-proposal-interface.md) records the next ownership boundary without implementing it. Existing desktop failures and Motion acceptance gates remain open; C3/C4 implementation and M6 have not begun.

## C3 — Rewrite Compositor preparation

**Primary files:** `compositor/src/operation.rs`, `model.rs`, `graph.rs`, `animation.rs`, `compile.rs`, and `compile_image.rs`.

**Work**

1. Keep native identity-bearing records authoritative for persistence and authoring. Decode supported records once into an immutable validated representation for execution.
2. Replace repeated JSON reconstruction/deserialization through `configuration()` with typed prepared configurations and stable-ID indexes. Sample animated values against the same prepared contracts.
3. Make validation guarantees enforceable: unchecked public mutation cannot produce a value later assumed valid by a panic-based accessor.
4. Keep unknown records preservable and non-executable. Preserve parameter/edge identities, protection, overrides, and extensions through edits.
5. Preserve valid image kernels and shared graph semantics; remove the superseded conversion path.

**Gate:** Current operations, animation, channels, masks, and reusable instances retain specified behavior and pixels. Invalid records return diagnostics. Preparation counters demonstrate configuration decoding is not repeated by each consumer within one preparation.

**C3 completion — 9 October 2026:** [Results and limitations](maintenance/evidence/c3/result.md). Immutable validated Compositor preparation now shares typed operation/animation contracts and stable-ID indexes across execution consumers; native records remain authoritative. Deleted the repeated configuration accessors and their panic assumption. Decode counters prove one operation decode per prepared record, with unchanged counts after traversal, channels, sampling and invocation binding. 69 focused tests and affected desktop/all-target and headless checks passed; one native capture remains ignored. Existing desktop failures and Motion acceptance gates remain open. C4 and M6 have not begun.

## C4 — Rewrite agent dispatch around provider ownership

**Primary files:** `app/src/agent/wire.rs`, `dispatch.rs`, `dispatch/native.rs`, `dispatch/prepare.rs`, observation/error adapters, and provider authoring implementations.

**Work**

1. Move Motion/Compositor parsing, scoped changes, diffing, reconciliation, and protection decisions into the owning providers through C2's interface.
2. Keep app dispatch responsible for request validation, admission, retries, jobs, observation scheduling, and publication through the project coordinator.
3. Remove provider-model knowledge from transport envelopes and generic candidate dispatch. Keep real cross-document application orchestration explicit; do not hide feature logic in downcasts or provider-name switches.
4. Preserve exact retry identity, revision/epoch guards, candidate inspection proofs, cancellation, and atomic mixed-document apply.
5. If the public contract must change, update the single supported contract, discovery, clients, schemas, and Fold skill together. Reject obsolete contracts explicitly; add no aliases.

**Gate:** Generic dispatch does not parse creative documents or import their concrete editing models. Public-tool tests cover create/edit/inspect/apply, stale conflicts, protected controls, rebasing, cancellation, save/reopen, and mixed-document atomicity.

**Checkpoint:** Workspace check, full applicable tests, Clippy, dependency/ownership checks, and public artist-to-agent handoff evidence. Update corresponding M3/M5a/M5c evidence without claiming their remaining gates complete.

**C4 implementation — 9 October 2026:** [Results and verification](maintenance/evidence/c4/result.md). Provider-owned preparation, cached bounded inspection, reconciliation, protection and diagnostics replace creative-model dispatch branches. App retains admission, retries, jobs, observation scheduling and mixed-document coordination; Project retains candidate lifetime and atomic publication. Public schemas, examples and the Fold skill follow provider discovery. Checkpoint passed: 772 workspace tests, 5 explicit native frame tests, 16 Python tests, workspace/headless checks, Clippy and dependency/ownership checks. Existing desktop failures and Motion acceptance gates remain open; C5 and M6 have not begun.

## C5 — Rewrite application coordination

**Primary files:** `app/src/session.rs`, session transport/view modules, project-file jobs, and retained `media_workflow.rs` responsibilities.

**Work**

1. Keep `Session` as the single application coordinator and committed project writer. Move project-file job state, command execution, and transport coordination into focused modules with explicit inputs and completion outcomes.
2. Replace loosely related job flags with explicit job kinds and lifecycle states. Preserve revision guards and stale-result rejection on project replacement.
3. Separate state refresh from expensive preparation. Consolidate equivalent mutation publication/refresh paths without weakening provider validation or undo semantics.
4. Remove legacy media responsibilities left after C1. Give retained output identity and preparation logic a clear home.
5. Reuse independently valid cancellation/admission helpers where duplication is demonstrated; keep differing preview, audio, file-dialog, and export lifetimes explicit.

**Gate:** Worker completions cannot publish into a replaced project. Open/save/export cancellation, dirty state, undo/redo, transient edits, and simultaneous viewer/agent activity pass. The coordinator no longer owns the implementation details of every workflow.

## C6 — Rewrite desktop shell state and routing

**Primary files:** `ui/src/shell.rs`, `shell_linking.rs`, viewer/transport integration, and workspace persistence.

**Work**

1. Replace parallel maps of viewer/editor state with cohesive runtime records keyed by stable panel identity.
2. Separate workspace/docking lifecycle, selection/navigation routing, delivery controls, and viewer drawing. Keep drawing dependent on explicit state and commands.
3. Make selection propagation and linked-panel updates testable without a complete rendered frame. Define ownership and transitions for focused, pinned, following, and closed panels.
4. Preserve useful widgets and panel implementations. Remove the old orchestration once the replacement handles supported transitions.
5. Review native normal/narrow layouts, multiple viewers, project replacement, linked selections, opening/closing panels, and save/restore. Investigate the freeze acceptance case with the captured evidence.

**Gate:** Tests prove no selection feedback loop or stale panel binding across relevant transitions. Native workflows remain usable and responsive. The original freeze is either causally resolved and regression-tested or remains explicitly open; unrelated passing tests do not close it.

**Checkpoint:** Workspace check, full applicable tests, Clippy, headless isolation, and native UI acceptance under the Fold UI skill.

## C7 — Simplify GPU execution and retire duplicate vector rendering

**Primary files:** `render/src/gpu/mod.rs`, `vector.rs`, `vector_legacy.rs`, associated shaders, and preparation/submission adapters.

**Work**

1. Separate request preparation, operation encoding, submission/completion, frame ownership, and statistics into focused modules. Retain the shared host's resource accounting and explicit leases.
2. Enumerate every supported input that reaches `vector_legacy`; qualify those cases in the retained vector implementation, then delete the old path and shader.
3. Preserve independently required image operations and explicit CPU reference execution. Do not remove behavior by treating “GPU-only” as the objective.
4. Verify cancellation and resource release before submission, during in-flight work, on device failure, and under preview/export contention.

**Gate:** Pixel/color/alpha tolerances pass for retained vector, image, and spatial fixtures. Old vector execution is absent. Memory/resource accounting and readiness rules remain valid. Native GPU verification is required; a compile or CPU-only pass is insufficient.

## C8 — Optimize measured preparation and refresh costs

**Work**

1. Compare C0 measurements with the simplified architecture. Rank remaining costs by edit-to-present impact and repeated work.
2. Reuse immutable decoded/prepared documents and stable-ID indexes where measurements justify retention. Cache keys must include relevant document content, provider implementation identity, and invocation context.
3. Distinguish presentation-only changes, semantic edits, time samples, source changes, and output/camera changes. Verify transient roots with the same committed revision never share stale results.
4. Bound retained bytes and verify eviction and project replacement release memory. Avoid duplicate caches at multiple layers for the same prepared result.
5. Move expensive synchronous refresh work to bounded workers where measured, preserving responsive selection and explicit pending/error states.
6. Track full incremental Motion dependency compilation and field execution in M6. Do not implement a competing evaluator or begin that work before M5/M5a/M5b/M5c acceptance. Record this dependency explicitly if it limits C8 scope.

**Gate:** Recorded workloads improve against the baseline without violating frozen correctness tolerances, cancellation, or memory limits. Each optimization has before/after evidence; no blanket performance claim is inferred from code structure. C8 can close for measured local preparation improvements while separately identified M6 expansion remains governed by the rewrite plan.

## C9 — Qualify and publish the maintained foundation

**Work**

1. Run the full applicable workspace checks, tests, Clippy, formatting, and boundary checks. Fix introduced failures and list inherited/environmental failures with their disposition.
2. Add architectural checks for deleted provider registrations and prohibited feature-model coupling where the crate graph alone is insufficient. Prefer structural or behavioral checks over broad keyword bans.
3. Add Clippy to CI. Inventory ignored tests by required environment and arrange explicit native/GPU qualification with durable evidence. Do not count ignored tests as passing or enable all of them without their prerequisites.
4. Re-run the integrated C0 workflows against the final build, including public authoring, native artist continuation, preview/export agreement, and save/reopen preservation.
5. Update `architecture.md` to the implemented ownership map. Keep one concise current handoff and link dated historical evidence; reconcile current product, skill, schema, and CLI guidance.
6. Review the entire maintenance diff for duplicated behavior, shallow pass-through modules, unnecessary abstractions, obsolete dependencies, and unrelated changes.

**Gate:** The maintained foundation has no executable superseded paths in this plan's scope. Current workflows and required native checks have evidence. All open issues have an explicit status; this phase does not claim completion while its own required checks or freeze acceptance remain unresolved. Motion phase completion is recorded only in its owning plan.

## Rewrite and deletion assignments

| Candidate at audit baseline | Treatment | Phase |
| --- | --- | --- |
| `crates/timeline/src/video.rs` and two-layer callers | Delete obsolete model and workflow | C1 |
| Old provider schema readers and workspace migrations | Remove compatibility execution; retain generic opaque handling | C1 |
| `crates/platform/src/packages.rs` — 1,025 lines | Redesign capability interfaces and registration | C2 |
| `crates/compositor/src/operation.rs` — 279 lines | Rewrite record preparation; retain native identity model and valid kernels | C3 |
| `crates/app/src/agent/dispatch.rs` / `dispatch/native.rs` — 957 / 613 lines | Rewrite provider dispatch; retain valid job/retry/candidate infrastructure | C4 |
| `crates/app/src/session.rs` — 1,171 lines | Rewrite coordinator responsibilities and lifecycle outcomes | C5 |
| `crates/app/src/media_workflow.rs` — 594 lines | Delete old workflow, relocate remaining output responsibilities | C1/C5 |
| `crates/ui/src/shell.rs` — 2,438 lines | Rewrite runtime state ownership and workspace routing | C6 |
| `crates/render/src/gpu/mod.rs` — 1,386 lines | Rewrite orchestration; preserve qualified execution kernels | C7 |
| `crates/render/src/gpu/vector_legacy.rs` and shader | Delete after retained implementation covers required cases | C7 |

Line counts include inline tests where present and are baseline observations, not size limits. Large tests and the Motion reference evaluator are not wholesale rewrite targets without separate evidence.

## Evidence and completion records

During iteration, format touched files, check affected crates, and run focused behavior tests. Run broad validation at C1, C4, C6, and C9, plus any checkpoint required by the Motion rewrite plan. C7 requires its native rendering checks. Avoid repeating unchanged checks without a new concern.

Each phase's completion record must include:

- Implementing commits and final ownership/interface changes.
- Deleted implementations, registrations, schemas, dependencies, fixtures, and aliases.
- Exact checks and results, including ignored/deferred checks and required environments.
- Behavioral, public-tool, native, and performance evidence appropriate to the phase.
- Remaining failures, limitations, and links to affected Motion acceptance records.

Store new durable evidence under `docs/maintenance/evidence/` when implementation begins. Do not create success records or mark phases complete from planning alone.
