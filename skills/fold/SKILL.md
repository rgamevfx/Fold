---
name: fold
description: Create and edit artwork in a running Fold project through MCP or fold-cli agent. Use for motion graphics, compositing, timelines, and reusable artist-editable setups; repository implementation follows the repository's development instructions.
---

Operate Fold through its public tools. Use the live catalog, inspected documents, and this skill's references as the authoring contract. Routine artwork must not require reading Rust crates, editing project archives, or calling private native APIs. If the contract leaves a capability unclear, inspect a tool-created example or make a small reversible probe; report a missing capability when neither resolves it.

## Establish the task

1. Discover `sessions`; select the intended project by path and documents. Reuse that session throughout the exercise. Before launching a desktop, check for an existing instance; when a release launch is needed, open the requested project in one instance and rediscover it. Resolve ambiguity between matching sessions. Connections require Agent → Allow local connections, or launch with `--agent`.
2. Read `state`, then scope `catalog` by `method` for request schemas or `type` and `query` for package operations and nodes. Inspect existing targets with JSON pointers and pagination. Poll inspection jobs and read their nested outcome before using the data.
3. For a visual reference, inspect actual images and multiple animation times. Match cached media to the supplied source before using it. Establish composition, timing, and likely artist edits before construction; resolve whether visible labels, guides, and selection markers belong in the recreation. If the reference is inaccessible, obtain the missing visual evidence rather than inventing its contents.

For CLI access: `fold-cli agent sessions`, then `fold-cli agent call SESSION METHOD '{}'`. Pass `-` instead of JSON to read long arguments from stdin. MCP tool names have the `fold_` prefix; both transports use the same authoring contract.

## Prove the construction

For unfamiliar concepts or a new procedural design, read the [authoring glossary](references/glossary.md). It distinguishes fields from uniform values, sampling domains, coordinate spaces, and reusable definitions from instances.

For Motion objects, text, copies, or group interfaces, read [Motion authoring](references/motion.md) before building. It supplies the coordinates, resource setup, and typed wiring conventions that node signatures alone do not explain.

Build the smallest useful part first: one visible source, its placement, and one driver or published control. Render it before multiplying copies or assembling a large graph. A successful validation proves structure, not appearance or editability.

Design around the artist's next edit. Keep content, distribution, and drivers independently editable; reuse sources across copies. Prefer shipped tools and editable groups. Name scene objects and expose useful controls with units, sections, and sensible defaults. Keep internals inside groups and distinguish whole-object from per-copy changes. Make referenced sources reachable from the artist-facing controls, including hidden text/shape targets. Put key moments on the layer's native animation track. Change instance inputs for local revisions; edit shared definitions only when all instances should change.

## Commit against observed state

Submit related edits in one labeled `batch`, using the observed `revision` and `project_epoch`. Allocate identities through `ids` and `{"$ref":"name"}`; artwork strings remain literal. Prefer targeted property and connection operations, preserving user animation, disconnected work, unknown data, and world placement.

Every distinct proposal gets a new `request_id`, including committing a previously validated proposal. Reuse an ID only for an identical retry after a lost response. Poll `jobs` until completion and inspect `outcome.ok`; acceptance is not commitment. Consume results promptly because retention is bounded.

On a conflict, refresh state **and affected content**, reconcile the user's changes, then propose a new batch. Refresh IDs after project replacement or reopening. During an active user edit, continue independent reads and retry once it finishes. Global undo may include user work: check the current revision and undo label first. Cancellation cannot reverse committed work.

## Verify and hand back

- Render representative times and animation boundaries. For loops, compare the start and one full period at the same revision and dimensions. Exact rational times avoid rounding drift.
- Exercise likely control edits in an isolated copy when practical, then remove test work. Otherwise distinguish rendered behavior from untested control interactions.
- Use `viewers` and `capture` to inspect the displayed result; check identities and stale status. Captures exclude chrome, overlays, and window zoom. `navigate` deliberately changes the named viewer and stops its playback. Use portal `screenshot` when the actual interface needs review.
- MCP `jobs` returns images directly; CLI image results contain PNG base64 to decode and view. Report rendering/runtime failures as errors, not black artwork. Keep environment repair separate from creative edits; use documented installation guidance instead of reconstructing runtime internals.
- Save to the requested or agreed destination with a fresh revision guard and verify the save outcome. Saving can change `project_epoch`; refresh state afterward. When reopening is requested, reopen the saved file, rediscover the session, inspect retained controls/resources, and render it again before claiming completion. Identify the objects and controls to edit, and disclose material unverified behavior.
- For a repository demo, save the current live revision under `demos/`, including the user's edits since the original construction. Pair it with a short artist guide naming the source-edit path and timing controls. Label previews from older revisions explicitly. Record verified workflow lessons and missing public capabilities in the skill references and `dev-plan.md`.

For Motion → Composite → Sequence authoring, adapt [the executable cross-feature example](scripts/demo.py). Running it adds documents; use it only when those additions fit the task. The [animated grid example](references/motion.md#example-animated-grid-with-cell-text) explains the reusable construction from a reference-recreation exercise without baking in that project's IDs or dimensions.

For morphing layouts, live coordinate labels, sampled stipple, synchronized overlays, or editable glow, read [the procedural dot-study workflow](references/procedural-dot-study.md). It supplies the demo's controls, formulas, timing, typed edit fragments, and verification steps. Adapt its shared relationships to the brief; its counts and aesthetics are example choices.

For particle attraction into editable text or shapes, read [the particle workflow](references/particles.md). It covers the shipped tool, independent point/attractor/palette inputs, layer keyframes, Solver limits, and reuse checks.
