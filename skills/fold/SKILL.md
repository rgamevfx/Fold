---
name: fold
description: "Author or inspect artwork in Fold through its qualified public tools. For repository implementation, follow the repository development instructions."
---

Use the running build's public discovery before authoring. The replacement Linux
build exposes MCP tool `fold_request` and `fold-cli protocol`; both use wire version
2 and the same generated request schema. For connection, jobs, guarded edits,
verified proposal/inspection/Apply examples and the integrated Motion demo,
read [Public workflow](references/public-workflow.md). For a local hands-on demo,
use the [demo launcher and edit guide](../../demos/README.md).

When exposing controls or linking parameters, read the
[parameter-reference workflow](references/public-workflow.md#parameter-references).
Use container inputs and inline expressions by default; do not create a separate
primitive node for each control or simple arithmetic expression. Discover the
running build's supported reference scope before proposing a link.

1. Discover the current epoch, revision, supported documents, provider schemas and limits.
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

M5a exposes public Composition/scene creation, hierarchy edits, object-scoped
source edits and mixed-document candidates. Use `project_authoring.schema` for
project assembly and `scoped_proposals[].requests` for the owning provider's
source/patch/edit schemas. Motion's operation catalog is under that provider's
`requests.catalog`. Inspect all changed documents before Apply. Current Composition writes
schema 3; use discovered records (Grade/Transform/Blur/Merge v3, Read/Shuffle v2, Write v1). C1 retired
the two-layer Timeline provider and older executable schemas. The new
[Make it move workflow](../../demos/make-it-move.py) uses three independent scene
layers with shared Composition keys and no exposed controls. The separate
[mixed-scene workflow](../../demos/m5a-mixed-scene.py) qualifies native compositor
records/scoped source between two scenes. Physical artist acceptance remains
unverified; see the linked evidence for automated and rendered qualification.

For repository implementation status, consult the
[Motion System Rewrite Plan](../../docs/Motion_System_Rewrite_Plan.md).

For Grid point sources, Attribute Create/Randomize/Transfer, or live Rhai Wrangle,
read [Geometry attributes and Wrangle](references/wrangle.md). Discover the
running host's catalog before using the examples; implementation and acceptance
status remain in rewrite phase M5b.

For Image Texture and Sample Image, read
[Image sampling](references/image-sampling.md). Use the public CLI example to
check sampled clone colors and source provenance.
