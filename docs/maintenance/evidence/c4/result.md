# C4 — provider-owned scoped proposals

9 October 2026. Base: C3 commit `f6e7f8b` (C2 `6f6f161`, C1 `f80804f`).
The first `git status --short` was empty; C3 was already committed. No C3 work
was discarded. The C0 inventory was reused rather than regenerated.

## Implementation

- Platform registers optional `ScopedProposals` capabilities for Motion
  constructions, Motion scenes, Compositor and Composition. Providers accept
  immutable snapshots, explicit scope/version, bounded opaque bodies and
  cancellation. They return immutable prepared records, bounded inspections,
  reconciliation results and validated document replacements. They cannot access
  a mutable Project, candidate store, transport or renderer.
- Motion owns graph source/patch parsing, definition correspondence, semantic
  diffing, graph/scene protection, operation/nesting admission, source diagnostics
  and scene/graph reconciliation. Compositor owns scoped source and record edits,
  protection and native reconciliation, using C3's validated preparation.
  Composition owns its record edits and inherited layer protection. Concurrent
  Composition changes still conflict; C4 does not invent a new merge policy.
- Common prepared state retains decoded records, immutable source/diff content,
  stable affected identities, allowed document writes and dependency content
  guards. Source/diff/diagnostics inspections have explicit scope and byte limits.
  Exact replacement and inspection bytes bind content identity. Partial views
  are bound to the returned content; App checks full inspection for publication.
  Reconciliation returns a new value and cannot amend
  an inspected generation. Unknown extension data survives native edits and
  envelope reconciliation.
- App retains envelope validation, byte-for-byte retry admission, jobs, worker
  cancellation, observation scheduling and publication coordination. The new
  duplicate-key check rejects ambiguous keys even inside opaque provider bodies.
  Existing project candidate retention, expiry, epoch/revision guards, receipt
  proofs, cancellation and single-transaction commit remain authoritative.
  App explicitly assembles create/layer/connect operations and validates the
  combined dependency graph before publication. A failed batch publishes nothing.
- Removed App's concrete candidate enum, Motion source/patch preparation,
  Compositor/scene parsing and reconciliation, creative protection checks, budget
  module and measurement implementation. Error adapters map typed provider
  failures without interpreting creative errors. Optional observation providers
  now own measurements and frame provenance; App still owns workers and GPU
  rendering. No provider-name switches or downcasts replace the deleted paths.
- Wire version 2 remains the sole transport. Provider request schemas and Motion's
  catalog move to `scoped_proposals[].requests`; project orchestration remains
  under `project_authoring.schema`. Provider bodies are validated by their owners,
  not imported into the generic schema. Updated the shipped consumers and Fold
  skill together. Compositor now directly depends on the already-pinned Schemars
  package to publish its owned schema; no new package/version was introduced.
- No UI feedback, compatibility path, dynamic plugin framework, new crate, C5
  coordination rewrite, or M6 evaluator work was added.

## Verification

The initial focused run exposed the sandbox's local-socket restriction and two
rebase diagnostic expectations. Public handoffs passed after enabling local
socket access. Newly protected scene edits remain conflicts; changed dependencies
now conflict before a possible cycle is assembled, as required by the C2 boundary.
The cycle test now checks that explicit dependency conflict and unchanged state.
[Initial run](initial-tests.log), [85 passing focused tests](focused-tests.log).

The final settled-source checkpoint passed workspace check, all applicable
workspace tests (**772 passed, 117 ignored**), Clippy, dependency/headless-boundary
checks, the scoped ownership regression check, and **16 Python tests**. Clippy
retains existing warnings; no warnings point to the new provider modules.
See [commands/status](checkpoint.json), [workspace check](workspace-check.log),
[full tests](workspace-tests.log), [Clippy](clippy.log),
[boundaries](boundaries.log), [ownership](ownership.log), and
[Python tests](python-tests.log). All four changed demo scripts also passed
Python compilation.

An intermediate checkpoint overlapped the last interface cleanup and encountered
stale doctest artifacts, plus a string-concatenation compile error in the newly
added budget regression. The final full checkpoint ran again after correcting
the test and settling source; all doctests and the regression now pass.
Final review retained Compositor's 1 MiB source contract (including a public
70 KiB source edit regression), preserved existing undo labels, and removed the
superseded Motion candidate aliases and their consumers.
The explicit [native frame rerun](native-frames.log) passed all **5 tests**:
Compositor public edits versus preview, transparent-title scene content,
revision/candidate frame provenance, mixed candidate versus committed pixels,
and CLI/MCP frame parity. These are five of the ordinary suite's ignored tests,
run separately with the existing native Vulkan/color runtime. The separate
[headless check](headless-check.log) also passed.

Commands use `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and `--offline --locked`.
Tests run with `--test-threads=1`. Native runtime roots use the existing
`/home/rgame/Documents/fold-native-pipeline/release-package` configuration.

Coverage includes source/patch/create/edit/inspect/apply, exact retries, stale
revision/epoch and expected-value conflicts, protected controls and inherited
locks, disjoint rebases and fresh receipts, dependency changes/cycles, candidate
and job cancellation, unknown extensions, undo/redo, save/reopen and mixed-document
atomicity. New tests cover provider capability/version/schema admission, immutable
inspection identity and selection budgets, duplicate opaque-body keys, and mixed
creation/cancellation/publication/save through both real CLI and MCP transports.

Public handoff checks use the actual `fold-cli protocol` and `fold-cli mcp`
adapters against the Session socket. They also verify provider catalog discovery
used by the updated examples. These are automated artist-service-to-agent handoffs,
not physical native pointer/keyboard acceptance.

## Acceptance limits

C0's desktop SIGSEGV captures and the original freeze remain unresolved. No new
native desktop interaction/capture, latency or allocation measurement is claimed.
The 117 tests ignored by the ordinary suite are not counted as passes; explicit
native reruns are reported separately. M3–M5/M5a–M5c artist/rendering acceptance
remains open, including universal-reference gaps and the existing physical
handoff obligations. M6 remains gated. This phase changes ownership and protocol
implementation, not those acceptance decisions.

Final diff review and `git diff --check` passed. No C5/M6 or unrelated source
changes were included; existing desktop failures and acceptance limits above
remain unchanged.
