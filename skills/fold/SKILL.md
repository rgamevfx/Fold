---
name: fold
description: "Author or inspect artwork in Fold through its qualified public tools. For repository implementation, follow the repository development instructions."
---

Use the running build's public discovery before authoring. The replacement Linux
build exposes MCP tool `fold_request` and `fold-cli protocol`; both use wire version
2 and the same generated request schema. For connection, jobs, guarded edits,
verified proposal/inspection/Apply examples and the integrated Motion demo,
read [Public workflow](references/public-workflow.md).

1. Discover the current epoch, revision, supported documents, catalog and limits.
2. Inspect the exact document revision and intended scope. Preserve stable IDs,
   protected controls, layout and unrelated source.
3. Propose a scoped source edit or expected-value patch with a fresh request UUID.
   Poll the returned job handle until it completes or reports an explicit error.
4. Inspect the returned immutable candidate. Review its source and semantic diff;
   use its receipt to Apply against the current revision. A changed base requires
   explicit rebase followed by inspection of the new generation.
5. Inspect CPU measurements with `observe`. When discovery reports frame observation
   available, request `observe_frame` and review decoded pixels and provenance as
   described in the public workflow. Record the applied revision and remaining
   limitations. Save committed artwork with `save_project_copy` when requested;
   follow the public workflow for publication and retry semantics. Media export
   and the full artist handoff remain open.
   Preserve original artwork and unsupported opaque documents.

Exact retries reuse the same Fold request UUID and identical argument bytes.
Changed edits use a new UUID. An active artist gesture causes Apply to conflict;
keep the gesture and submit a fresh request after it finishes. Agent operations
cannot unlock protected controls. Use public discovery/errors to resolve missing
capabilities; artwork work must not require reading Rust crates or editing project
payloads directly.

For repository implementation status, consult the
[Motion System Rewrite Plan](../../docs/Motion_System_Rewrite_Plan.md).
