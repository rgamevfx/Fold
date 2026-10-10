# Current Fold handoff — 9 October 2026

C9 implementation and available qualification are recorded after C8 `f8e7abb`, C7 `c6cd6bf`, C6 `ecf016f`
and C5 `2e540bf`. [C9 evidence](../maintenance/evidence/c9/result.md) owns current
checks and blockers. [Architecture](../architecture.md) maps current ownership.
Read the [maintenance plan](../Codebase_Maintenance_Plan.md) and
[Motion rewrite plan](../Motion_System_Rewrite_Plan.md) before continuing.

## Preserved boundaries

- C4: providers own creative proposal preparation, inspection, reconciliation and
  protection; App owns transport/jobs; Project owns candidate publication.
- C5: Session is the single application coordinator; worker completions carry
  project-generation/revision guards. Cancellation retains admission until acknowledgement.
- C6: stable panel runtime records own viewer state; workspace, linking, delivery
  and drawing have distinct responsibilities. Empty-layout restore is fixed.
- C7: one vector path; GPU completion/device-failure checks preserve leases,
  readiness, cancellation and resource accounting.
- C8: measured local Compositor preparation only (256-node decode/validation
  27–35%, preparation 53–55%). No cache or refresh/native-latency improvement claim.

Preserve immutable snapshots, stable IDs, rational time, explicit color/alpha,
opaque unsupported payloads and transaction/undo authority. No new UI feedback,
unrelated refactoring or M6 work is authorized by maintenance.

## Open gates

1. Original unknown-trigger desktop freeze: unresolved; unrelated passing tests
   do not close it. [Incident](evidence/m5a-desktop-freeze.md).
2. C0 separate unbound shutdown SIGSEGV: unresolved. C6's causally fixed
   empty-layout restore is a distinct incident. [C6 evidence](../maintenance/evidence/c6/result.md).
3. Spatial extrusion ray-depth test: failed/open, unchanged tolerance. C6 and C7
   hardware binaries reproduce identical values. User acceptance of C7's specific
   exception does not waive C9 or M4. [C7 evidence](../maintenance/evidence/c7/result.md).
4. Physical artist continuation/acceptance: unverified. Automated native panels,
   offscreen GPU frames and desktop submission are separate evidence. Rediscover
   processes before using any session; preserve live windows and unsaved artwork.
5. M3–M5/M5a–M5c remaining gates: open. M6 incremental execution stays blocked.
   M5c exposes provider-owned stable parameter targets and exact occurrence-clock
   inspection/sampling, but `cross_document: false`; cross-provider expression
   bindings, complete reusable-effect occurrence support and universal UI editing
   remain incomplete. [M5c evidence](evidence/m5c-result.md).

C9 covers 795 distinct non-ignored workspace tests passing and 112/119 explicit
native tests passing. Seven inherited native failures remain open: color publication
timeout, cloner/ray/box depth, two mixed-color precision cases and extrusion depth.
The C9 ledger retains current logs and pre-maintenance controls. The current helper
qualifies with its pinned development SDK and isolated relative-library package;
the installed release helper has a stale private protocol.

C9 cannot be called complete while its required checks or freeze acceptance remain
unresolved. Do not turn ignored tests into passes or automation into artist approval.

## Reproduction and public guidance

[C0 workflows/profile](../maintenance/c0-workflows.md) define integrated checks;
C9 records final-build reruns and ignored-test prerequisites. Use offline locked
Cargo dependencies, two jobs, no incremental compilation and dev/test debug info
disabled. Native runtime paths belong in evidence/environment, not global config.

[Fold skill](../../skills/fold/SKILL.md) and its public workflow use live discovery,
version-2 `fold_request`, `fold-cli protocol`/`mcp` and provider-owned schemas.
Public contracts did not change in C5–C9. Artwork authoring must not require Rust
source inspection. [Demo guide](../../demos/README.md) identifies current saved
fixtures and launchers; temporary sessions from earlier records are not current endpoints.

Dated history: [M5a cutover](evidence/m5a-compositor-cutover/result.md),
[M5b](evidence/m5b-result.md), [C6](../maintenance/evidence/c6/result.md),
[C7](../maintenance/evidence/c7/result.md), [C8](../maintenance/evidence/c8/result.md).
Those records retain their original qualification limits and do not close newer gates.
