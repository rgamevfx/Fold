# C9 maintenance review — 9 October 2026

Review range: audit baseline `6280fb6` through C8 `f8e7abb` plus C9 working changes.
Reused C0's assigned deletion inventory and C1–C8 per-phase reviews/evidence.
The complete changed-file inventory separates production/tests, explicit fixture
reauthoring, public guidance and dated evidence; large raw evidence additions are
not new execution paths. C9 adds no production Rust behavior or dependency.

| Area | Review finding |
| --- | --- |
| C1 retirement | Old two-layer model/provider/CLI branches, schema readers, archive upgrades and workspace migration are deleted. Current native records and unsupported opaque archive payloads have separate behavioral tests. C9 extends missing retired Sequence/Compositor schema cases in the existing test. |
| C2 failures/capabilities | Failure adapters translate categories at real service boundaries; the proposal error vocabulary additionally carries conflict/protection/source diagnostics. These are not competing retry implementations. Optional provider capabilities replace empty-success defaults. Scheduling decisions use typed pressure/cancellation with bounded retries. |
| C3/C8 preparation | Native records remain authoritative; immutable prepared configs/indexes serve execution, sampling and channels. No mutable unchecked cache or parallel persisted model is introduced. The empty-track guard follows structural validation and retains no bytes. |
| C4 proposals | Creative policy moved to providers; App's remaining mixed-document assembly is explicit application orchestration. Project owns candidate lifetime/publication. Shared inspection budgets and immutable validated changes justify the platform interface. Small error/budget adapters retain provider-specific meaning rather than introducing generic job frameworks. |
| C5 coordination | Session remains the sole coordinator; Jobs owns actual lifecycle/admission and returns completion data. Agent owns endpoint/dispatcher state and changed-state outcomes. Output modules own identity, scene requests and delivery; they do not wrap the deleted media-workflow implementation. Saved-state refresh avoids output preparation. |
| C6 desktop | Cohesive panel records replace parallel maps. Workspace/linking/delivery/viewer modules contain their own transitions or drawing inputs, not a second Shell coordinator. Shared layout restore fixes the demonstrated C-string hazard. No new maintenance UI feedback. |
| C7 GPU | Request/encoding/submission/frame boundaries transfer leases and readiness state. Host synchronization and mapped-state decoding retain completion safety. Old vector Rust/WGSL execution and counters are deleted; explicit CPU reference geometry remains independently useful. No tolerance relaxation. |
| Dependencies | The only maintenance library dependency addition is Compositor's already-locked `schemars`, used by provider-owned request schemas. App adds the opt-in maintenance example. No legacy renderer/compiler dependency or new framework is introduced in C9. |
| Public contract/docs | C4's schema relocation is reflected in discovery, CLI/MCP clients, demos and Fold skill. C5–C9 do not change public authoring contracts. C9 removes stale architecture references to automation, Rust assets, Solver, old UI policy and retired roadmap authority; one handoff links dated records. |
| C9 checks | Reuse existing provider retirement/opaque persistence tests and Cargo boundaries. Extend the existing scoped ownership check to recursive modules, aliases/grouped imports, comments/literals and trailing production after test modules; four focused policy tests exercise violations and legal references. CI runs it and Clippy. No broad repository keyword prohibition. |

The source seam check is deliberately lexical and scoped; it is not proof against
macro-generated or re-exported coupling. Behavioral capability, proposal atomicity,
transient-root, native rendering and cancellation tests remain necessary.
No unrelated refactor, new evaluator/cache, compatibility wrapper, new UI surface
or M6 implementation was found/added in this maintenance review. Existing known
failures remain in result.md; passing review does not waive their gates.
