---
name: fold
description: "Author or inspect artwork in Fold through its public tools. For repository implementation, follow AGENTS.md."
---

# Fold artwork

Use public tools, not Rust internals or direct edits to saved project payloads.
Artists and agents share one authoritative document and guarded Undo history.

1. Discover the connected build's epoch, revision, providers, schemas and limits.
2. Inspect the intended document at that revision. Preserve stable IDs,
   protections, unknown fields and unrelated artwork.
3. Propose typed native edits or a discovered project transaction. Poll jobs,
   inspect the immutable candidate and its diff, then review available pixels.
4. Apply the inspected receipt against the current revision. Concurrent edits
   require fresh inspection/reconciliation. Review the committed output.
5. Save committed artwork when requested; confirm the save job's result.
   Report unverified rendering or artist behavior.

Read only the needed reference:

- [Public workflow](references/public-workflow.md): connection, guards, candidates,
  jobs, frame review and saving. Preserve lossless JSON expected-value guards.
- [Native recipes](references/native-recipes.md): Motion objects, images, script
  conversion and path/animation edits. Use discovery for full schemas.
- [Motion craft](references/motion-craft.md): design, scripting and animation review.

A Motion script is one object regardless of generated shape count. Its graph
projects the document rather than storing a second model. Composition/Compositor
and Timeline consume outputs; 3D owns spatial scenes. Unsupported documents stay
opaque. Historical demos are not evidence that a retired provider is available.
