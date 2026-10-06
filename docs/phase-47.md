# Phase 47 — Agent authoring overhaul

Status: implementation in progress. Private candidates, bounded native SDK execution, typed expressions, geometry probes and sampled checks are implemented; the complete semantic overhaul and Luna/high qualification are not complete. See [implementation evidence](phase-47-implementation.md).

## Goal

Make agent control a first-class Fold product capability. An agent must understand an existing composition, make a precise edit, evaluate and correct the result, and leave native artwork that an artist can continue editing. This is a substantial overhaul of authoring, discovery, observation, and verification, including changes to Motion's internal model where necessary; schema fixes alone do not complete this phase.

The current interface requires too much low-level graph serialization and inference from limited runtime feedback. Replace that experience with fluent typed authoring, explicit motion semantics, candidate evaluation, and executable outcome checks.

Product grounding: [architecture proposal](Fold_Application_Product_Architecture.md), particularly sections 4–6, 11–15, and 18; [development plan](../dev-plan.md); [Fold UI skill](../.agents/skills/fold-ui/SKILL.md). Existing implementation and evidence: [phase 43](phase-43.md), [phase 45](phase-45.md), and [phase 46](phase-46-motion-performance.md).

## Scope and architectural decisions

Retain Rust application logic, independent Motion/compositor/timeline packages, immutable render snapshots, exact rational time, transactional undo, stable identities, and unknown extension data. Keep feature-specific models out of project/render infrastructure. Preview, export, and headless operation continue to share evaluation semantics.

Overhaul the public authoring contract, capability/property definitions, Motion authoring semantics, runtime inspection, provisional editing, verification, and skill delivery. Keep the current interface as a compatibility adapter during migration; retire superseded paths after equivalent coverage is demonstrated.

The preferred initial language direction is a typed TypeScript authoring SDK backed by Rust commands. TypeScript orchestrates edits; Rust owns document meaning and execution. Validate that choice with an end-to-end prototype before fixing its syntax and runtime dependencies. A familiar-language wrapper that leaves the existing semantic ambiguities intact does not satisfy this plan.

The canonical editable document remains authoritative. Code submits targeted changes through stable handles. It must not become an independent source of truth that overwrites subsequent visual edits. Distinguish code that constructs/edits a composition from expressions evaluated per frame or element. Retain Rust/Wasm Wrangle for custom behavior while ordinary fields and bindings use native, inspectable operations.

Adopt the useful MCP for Blender pattern: broad programmatic authoring, compact semantic inspection, and convenient visual observation behind a small public tool surface. Avoid requiring a bespoke MCP tool for every creative operation. Preserve Fold's candidate isolation and atomic commits underneath code execution; unrestricted scripts mutating live application state are not the target architecture.

## Ownership

| Owner | Responsibility |
|---|---|
| Project infrastructure | Candidate lifecycle, revisions, atomic commit, undo, bounded retention |
| Motion package | Motion semantics, typed expressions, authoring operations, evaluated probes, motion checks |
| Compositor and timeline packages | Their own authoring and inspection capabilities through shared contracts |
| Application automation | Sessions, capability discovery, jobs, MCP/CLI adapters, structured results |
| Shared UI SDK and feature UI | Controls, selection, previews, contextual comparison and diagnostics |
| Acceptance harness | Independent outcomes, preservation, reliability, performance evidence |

Start at existing seams such as `crates/app/src/automation`, `crates/app/src/session_automation.rs`, `crates/platform/src/authoring.rs`, and `crates/motion/src/automation`. Do not add a universal creative model or introduce a new crate solely to mirror this table. Keep module interfaces small while their implementations own validation, execution, and useful diagnostics.

## Milestones

### 47.1 — Acceptance baseline and replacement contract

- [x] Define 30 representative tasks covering creation, existing-project edits, troubleshooting, and artist handoff in [the task inventory](phase-47-tasks.json). Fixture pins, held-out assignments, budgets and baseline results remain open.
- [ ] Include scrolling text, nested groups, glyph animation, masks, instances, reusable controls, fonts, stateful regions, and Motion → Composite → Sequence workflows.
- [ ] Record baseline completion, corrective iterations, elapsed time, tokens/tool calls, unintended changes, and native editability.
- [ ] Define a versioned contract around discover, observe, author, evaluate, verify, commit, and explain. These are interface responsibilities, not a prescribed tool count.
- [ ] Pin fixtures, model/configuration identities, budgets, and independent expected outcomes; separate held-out variants from development cases.
- [ ] Define controlled Luna/high comparisons of typed command authoring and compositional SDK code using equivalent capability coverage, observation tools, task briefs, and budgets. Keep interface effects separate from semantic-engine improvements.

Acceptance: failures reproduce from saved fixtures, and artwork agents can run the suite without repository access or hidden reference answers. Benchmark definitions are fixed before comparing implementations.

### 47.2 — Authoritative capability and property definitions

- [ ] Replace incomplete/default-derived descriptions with explicit package-owned definitions of operations, properties, settings, and configured sockets.
- [ ] Describe types, units, spaces, domains, defaults, animation/binding support, availability, and execution limits.
- [ ] Generate tool/SDK schemas, reference documentation, and inspector metadata from the same public definitions.
- [ ] Make every registered public authoring operation available through SDK generation and discovery, with explicit unsupported cases. Test the externally advertised and callable surface; an internal helper does not count as agent-accessible functionality.
- [ ] Cover font discovery/import, masks, visibility, ordering, published controls, resources, object selection, and inspector/viewer targeting.
- [ ] Return structured errors identifying the operation, property, expected value/type, and supported repair options.
- [x] Reject unknown query arguments and expose unsupported capabilities explicitly instead of ignoring requests.

Acceptance: an unfamiliar agent can discover supported operations and complete edits without probing undocumented JSON shapes. Contract tests cover configured signatures and errors as well as successful calls.

### 47.3 — Motion authoring semantics

- [ ] Make whole-object, child, descendant-glyph, instance, and path-point scopes explicit, with deliberate selection and domain conversion.
- [ ] Distinguish local/document/screen spaces; authored bases, keyframes, expressions, and combined contributions; pure animation and stateful simulation.
- [ ] Expose text layout bounds, ink bounds, advances, shaped glyph clusters, and reproducible font identity.
- [ ] Diagnose zero matching elements and incompatible scopes without silently treating them as successful motion edits.
- [x] Introduce a shared typed expression representation for ordinary fields/bindings, preserving explicit dependencies and native evaluation.
- [ ] Preserve stable generated identities where defined, and declare identity changes caused by topology operations.

Acceptance: constructions mean the same thing in the inspector, graph, agent interface, preview, and export. Scope, text shaping, transform composition, seeking, and simulation invalidation have focused fixtures. Document migrations accompany any persisted semantic changes.

### 47.4 — Fluent programmatic authoring

- [ ] Begin an end-to-end typed SDK/code-authoring prototype alongside 47.2, using existing transactions and available semantics; do not wait for all of 47.3. Deliver the full bounded MCP/CLI authoring implementation as the semantic model matures.
- [ ] Provide compositional operations for text/layout, transforms, repetition, signals, timing, fields, and control bindings, plus precise low-level graph editing.
- [ ] Support existing stable handles and deterministic update identities; repeated execution must not accidentally duplicate constructions.
- [ ] Keep generated output native and editable; preserve manual graph/inspector changes unless an operation explicitly targets them.
- [ ] Generate discoverable SDK references and executable examples from the public contract.
- [x] Bound execution time, memory, output, and accessible operations; resource import uses explicit commands rather than unrestricted application-process execution.

Acceptance: an agent creates an effect, an artist changes it through the inspector/graph, and the agent continues correctly from the changed state. Inspectable native operations handle ordinary animation; writing per-element Wrangle code is not required to move a whole phrase.

### 47.5 — Persistent candidate edits

- [x] Extend transactional infrastructure with bounded, temporary candidate snapshots supporting multiple edits, evaluation, comparison, and discard.
- [x] Keep candidate evaluation separate from committed state and artist playback; do not move the playhead to inspect a candidate.
- [ ] Associate results with exact candidate generations, assets, and evaluation settings; subsequent changes invalidate stale evidence.
- [x] Commit the evaluated candidate as one named undo action against an expected revision.
- [ ] Provide targeted conflict reports, cancellation, retry-safe requests, cleanup on disconnect/expiry, and explicit resource budgets.
- [ ] Allow automatic rebasing only where recorded dependencies establish safety; begin with explicit conflict recovery.

Acceptance: failed experiments leave no partial committed changes or undo clutter. Concurrent human edits cannot be overwritten silently. Discard, cancellation, disconnect, conflicts, retries, and resource exhaustion have regression coverage.

### 47.6 — Semantic and visual debugging

- [ ] Inspect evaluated counts/scopes, transforms, bounds, parameter contributions, visibility, clipping, masks, and empty outputs.
- [ ] Expose build/backend identity, costs by execution stage, resource limits, and fallback reasons through public diagnostics.
- [ ] Add exact-time multi-frame capture, contact sheets, short previews, full-resolution crops, and optional object-identity overlays.
- [x] Provide a unified observation request that returns a timestamped animation strip and selected evaluated measurements from one pinned snapshot/candidate. Support target/field filters, bounded summaries, and adjustable image detail; preserve playhead and viewer state. Report missing/degraded modes explicitly rather than silently substituting unrelated evidence.
- [ ] Trace visible elements to their authoring nodes where supported; report unavailable provenance explicitly.
- [ ] Return causal explanations for supported failures, including invisible output; distinguish unknown causes from established explanations.
- [ ] Identify every result by snapshot/candidate generation, exact time, output, dimensions, quality/color settings, and relevant environment identity.

Acceptance: agents diagnose seeded blank-output, overlap, incorrect-domain, stale-preview, and excessive-cost failures using public tools. Mutations, structural validation, runtime evaluation, and successful rendering have distinct result states.

### 47.7 — Executable motion verification

- [ ] Add package-owned checks for movement, direction, displacement, periodicity, visibility intervals, relative positioning, and control response.
- [ ] Return measured values, failing times, tolerances, and supporting images.
- [ ] Distinguish sampled evidence from analytical guarantees, and support explicit unknown/inconclusive outcomes.
- [ ] Define boundary sampling consistently with exact time and half-open export ranges; inspect looping behavior without requiring an invalid export endpoint request.
- [ ] Keep readability and aesthetic review grounded in rendered output; bounding-box overlap alone does not establish illegibility.
- [ ] Keep benchmark reference checks independent of the acting agent's generated checks.

Acceptance: intentionally broken animations fail for the correct reasons. Static output cannot pass a movement task merely because frames one period apart are identical. Exposed controls must measurably affect their intended output.

### 47.8 — Artist handoff, skills, compatibility, and qualification

- [ ] Give generated effects meaningful names, controls, graph organization, and normal selection/undo behavior.
- [ ] Add contextual candidate previews/comparison and diagnostic access through shared UI patterns; apply the Fold UI skill and inspect normal/narrow layouts.
- [ ] Ship short, version-matched agent guidance with searchable references, capability-specific examples, and a documentation fallback for hosts without skill discovery support.
- [ ] Deliver essential discovery/author/observe/verify guidance through MCP initialization and tool descriptions. Test fresh-agent bootstrap with no preloaded Fold skill; deeper shipped guidance must be discoverable from the connected application, not dependent on a repository path or a user-invoked prompt.
- [ ] Update [the Fold skill](../skills/fold/SKILL.md) and its references with verified public-tool examples and reusable lessons. Execute examples in contract tests and keep remaining capability gaps in `dev-plan.md`.
- [ ] Preserve existing IDs, animation, references, and unknown data through required migrations. Verify compatibility by save/reopen and rendered comparisons.
- [ ] Qualify external clean-session agent runs, native artist continuation, undo/redo, persistence, preview/export equivalence, and performance before retiring legacy paths.

Acceptance: an external agent uses only shipped capabilities/guidance to finish the task, and an artist can continue editing the saved native result. Documentation, transport, and implementation agree.

## Primary acceptance workflow: reference recreation from scratch

The flagship acceptance workflow is [the reference recreation benchmark](phase-47-reference-benchmark.md): recreate [mcp-test.gif](media/mcp-test.gif) from a blank project as native procedural Fold artwork. Follow that document's fixed brief, pinned reference, grading dimensions, isolation rules, and evidence requirements.

The implementation agent must actually execute at least five independent trials using fresh `gpt-6-luna` subagents with `reasoning_effort: "high"` and `fork_turns: "none"`. Do not replace them with the parent agent, a stronger model, or a coached continuation. Missing model/tool availability is an unperformed gate, not grounds for substitution. All five flagship trials must pass; record failures before fixes and rerun the full gate on the qualified build.

The reference is a 1080×1080, 44-frame, 3.66-second repeating animation of circular-cell letterforms with gold shading and colored transition accents. Inspect the full reference to establish its exact sequence and timing. Agents must construct the artwork through public MCP tools, expose meaningful controls, render/compare the whole cycle, and prove follow-up editability and persistence. Importing the reference as finished artwork or using baked frames does not count.

This replaces Katie as the primary acceptance workflow. Retain Katie below as an early vertical slice and a separate existing-project preservation regression; success on Katie alone cannot complete phase 47.

## Supporting vertical slice: Katie scrolling text

Use an isolated fixture derived from the existing Katie composition; do not run destructive acceptance experiments against an artist's live project.

1. Discover the text and its existing effects through the new interface.
2. Change it to `i love katie` using fluent authoring and measured text layout.
3. Add a right-to-left loop with an eight-second default cycle and a working exposed speed control.
4. Evaluate and refine privately, including wrap boundaries and visible spacing.
5. Verify movement, direction between wraps, periodicity, control response, and preservation of unrelated content.
6. Commit once; save/reopen and verify persistence.
7. Have an artist adjust it through native controls, then let the agent make a further targeted revision.

For a repeated strip with period length P, default speed is P / 8 seconds. Define the control relationship explicitly: changing speed changes the cycle duration when spacing is fixed. Do not present independently editable speed and period as if they can both remain fixed. Preserve shaped text and existing effects; per-glyph motion is only appropriate when requested.

## Dependencies and rollout

Start with 47.1, then develop 47.2 alongside the early 47.4 code-authoring prototype and a minimal 47.6 observation loop. Exercise an end-to-end public workflow before completing the semantic redesign. Develop 47.3 and 47.5 against the demonstrated requirements, then complete 47.4/47.6 and 47.7. Milestone numbers identify scope, not a strictly serial implementation order. Compatibility and artist-handoff fixtures run throughout; final acceptance still requires every milestone.

Compare the repaired existing interface, fluent authoring over the current graph, and revised semantic implementations on the same tasks. The overhaul is committed scope; these comparisons determine implementation choices and demonstrate which changes earn their complexity. Keep working reference paths until replacements pass equivalent checks, then remove obsolete scaffolding.

Within a fixed implementation/build, compare typed command authoring with compositional SDK code under the same Luna/high settings and budgets, as specified in the reference benchmark. Do not attribute gains to syntax when the variants differ in available operations or feedback. The final release gate runs on the selected shipped interface with its discoverable public guidance.

Phase 47 incorporates phase 46's proposed graph-generation/native-expression direction where it serves agent authoring. GPU field execution and unrelated rendering optimization retain their own correctness/performance qualification; they are not prerequisites for declaring an authoring syntax usable or substitutes for this phase's outcome tests.

## Release criteria and verification

- Require the five-of-five clean Luna/high reference-recreation gate in [the benchmark](phase-47-reference-benchmark.md), including full-cycle fidelity, native construction, follow-up edits, and persistence.
- Target at least 95% task-run success on the agreed core suite within predeclared budgets using fresh `gpt-6-luna` subagents with high reasoning effort and no inherited conversation, with at least five clean-state repetitions per task. A second configuration may supply comparative evidence but cannot replace or offset the required Luna/high results. Report first-attempt success, eventual success, per-task consistency, and held-out results; aggregate success must not hide a persistently failing task family.
- Treat unintended edits, failed persistence, and broken artist controls in deterministic regression fixtures as release blockers.
- Require fresh-agent bootstrap, public capability coverage, and unified same-snapshot observation evidence in the benchmark. Code that edits and then fails must leave committed state unchanged unless an explicit prior commit succeeded.
- Record elapsed time, tokens, tool calls, corrections, artist intervention, and resource use. Establish latency/cost budgets from 47.1 before comparing candidates; do not invent performance claims.
- Include non-Latin shaping, longer replacement text, changed font/size, nested transforms, frame-rate changes, user edits between calls, resource failures, and cancellation in task variations.
- Run focused module/contract/integration tests during implementation. At vertical-slice checkpoints run workspace checks, full relevant suites, Clippy, package-boundary checks, live MCP/CLI trials, and native UI review as required by the development plan.
- Verify out-of-order evaluation and matched preview/export settings, native editability, undo/redo, and save/reopen. Record CPU/GPU differences, unsupported checks, pre-existing failures, and unperformed native interaction explicitly.
- Store fixture identities, tool transcripts, diagnostics, rendered evidence, configuration/build identities, and benchmark results with the phase evidence. Mark this phase complete only after its acceptance criteria pass.

## Research informing the design

These are architectural precedents and published experiments, not evidence that Fold has achieved comparable results:

- [MCP for Blender server](https://github.com/ahujasid/mcp-for-blender/blob/main/src/blender_mcp/server.py) and [observation scripts](https://github.com/ahujasid/mcp-for-blender/blob/main/src/blender_mcp/blender_scripts.py): broad Python authoring, compact scene queries, and multi-frame visual feedback. Source review on 2026-10-05 found configured-node introspection in the add-on but not exposed in the reviewed MCP server, and guide tools explicitly disabled for evaluation. Verify actual registration/discoverability instead of assuming helper presence establishes public capability. This was a source review, not a live reliability benchmark; retain Fold's stronger transaction/snapshot requirements.

- [Rive RML](https://github.com/rive-app/rive-docs/blob/main/runtimes/advanced-topic/rml.mdx): text authoring connected to native editable content and separate compile/inspect/render checks.
- [Houdini APEX Script](https://www.sidefx.com/docs/houdini/character/kinefx/apexscriptbasics.html): code that builds inspectable graphs.
- [MoVer](https://mover-dsl.github.io/): executable spatial/temporal requirements and correction feedback.
- [LogoMotion](https://arxiv.org/html/2405.07065v2): visual context, localized repair, and direct editing controls.
- [Theatre.js Studio](https://www.theatrejs.com/docs/latest/api/studio): transactions and provisional edits.
- [tldraw agent kit](https://tldraw.dev/starter-kits/agent): structured and visual observations plus human interaction context.
- [Remotion determinism](https://www.remotion.dev/docs/flickering) and [Motion Canvas time events](https://motioncanvas.io/docs/time-events/): predictable temporal evaluation and artist-adjustable timing.
- [Code4Scene](https://arxiv.org/html/2609.36777v1): evaluate native artifacts and preservation separately from visual plausibility.
