# C4 provider-owned scoped proposal contract

C2 specification, 9 October 2026. This is the interface C4 must implement after C3; it is not a registered capability or an implemented authoring path. Existing public envelopes and candidate dispatch remain authoritative until that change qualifies.

## Boundary

Platform will define an optional `ScopedProposals` capability beside the document capabilities. Static provider registration exposes it only when its complete implementation is available. The interface accepts immutable `Snapshot`/`Document` values, stable IDs, exact time, a cancellation token and bounded requests. It never receives a mutable `Project`, `Session`, candidate store, transport, or renderer. Provider types and downcasts do not cross this boundary.

The app validates envelopes, request size, permission, retry identity, project epoch and revision, then resolves the provider by document identity/schema and capability. The provider validates and interprets the versioned provider-owned request body. Unknown kinds, schemas, scopes and operations fail explicitly; they do not select a fallback parser.

## Operations and values

The Rust shape to implement in C4 is:

```rust,ignore
trait ScopedProposals: Send + Sync {
    fn prepare(&self, base: &Snapshot, request: &ScopedRequest,
               cancel: &Cancel) -> Result<Box<dyn PreparedProposal>, ProposalFailure>;
}
trait PreparedProposal: Send + Sync {
    fn scope(&self) -> &ProposalScope;
    fn inspect(&self, request: &InspectionRequest,
               cancel: &Cancel) -> Result<Inspection, ProposalFailure>;
    fn reconcile(&self, current: &Snapshot,
                 cancel: &Cancel) -> Result<Box<dyn PreparedProposal>, ProposalFailure>;
    fn publication(&self, current: &Snapshot,
                   cancel: &Cancel) -> Result<ValidatedChanges, ProposalFailure>;
}
```

- `ScopedRequest`: stable document ID, optional object/network/control IDs, operation/version, and bounded provider payload. Scope is explicit; omitted scope never expands permission. Base revision and exact retry identity come from the host envelope, not provider source text.
- `ProposalScope`: owning document identity/schema, affected stable object/control IDs, read dependencies and their revision/content guards, and the allowed write set. It must be inspectable without importing creative models.
- `PreparedProposal`: immutable, provider-owned decoded base/proposed records and reconciliation/protection facts. Preparation parses once, validates references and identities, checks resource limits, computes a semantic diff and preserves unknown records/extensions. No committed writes occur.
- `InspectionRequest`: source/diff/diagnostics selector plus bounded scope. `Inspection` carries provider-owned source, stable-ID changes, diagnostics and inspection content identity. The host binds inspection proofs to candidate ID, project epoch, candidate generation and exact inspection content. The provider cannot mint a host proof or bypass observation requirements.
- `ValidatedChanges`: replacement document envelopes and declared dependencies/write set, tied to the current revision and prepared content identity. No feature-specific project mutations escape in an unvalidated payload. The host validates cross-document dependencies and the combined edit batch before atomic publication.
- `ProposalFailure`: typed invalid request, unavailable capability/resource, cancelled, resource pressure/oversized, execution, stale/conflict, and protected-edit failures. Diagnostics retain provider ID, document ID, optional object/control/edge IDs, source range, code and explanatory text. Text never controls retries or protection policy.

## Reconciliation and protection

The provider owns native parsing, scoped edit semantics, stable-ID correspondence, semantic diffing and three-way reconciliation against the original base. It must preserve unrelated current edits, artist overrides, protected controls, animation and unknown extension data. Conflicting edits, removed targets, changed dependencies, ambiguous correspondence and attempted scope expansion return structured conflicts. A successful rebase produces a new immutable prepared value; it cannot silently mutate a previously inspected candidate.

Protection is enforced during preparation, reconciliation and publication, using current locks and current document state. Agent requests cannot clear artist locks. Trusted artist unlocking stays in its separate capability. Any reconciled content change invalidates the previous inspection proof and requires the host's existing inspection/observation gates again.

## Host and project authority

The project coordinator retains candidate creation, retention/expiry, cancellation, retry cache, generations, epoch/revision guards, observation scheduling, undo/redo and atomic commit. The provider cannot publish or extend candidate lifetime. The app constructs cross-document orchestration explicitly and composes provider changes into one project transaction; if any provider fails, none publish. Final publication rechecks epoch, revision, proof, dependencies, scope and protection under the existing commit authority. Cancellation before publication leaves committed state untouched.

C4 must delete the concrete Motion/Compositor parsing and reconciliation branches it replaces, with no aliases or parallel dispatch. Its gate must cover scoped create/edit/inspect/apply, exact retries, stale conflicts and successful disjoint rebases, protected controls, cancellation, save/reopen, inspection invalidation and mixed-document atomicity through public tools. This specification closes no Motion or native acceptance gate and authorizes no M6 work.
