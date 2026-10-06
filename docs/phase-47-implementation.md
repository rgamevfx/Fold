# Phase 47 implementation evidence

Status: implementation and developmental trials in progress. Phase 47 is incomplete. No qualifying reference recreation or 95% task-suite result is claimed.

## Implemented

- Project-owned candidates stage immutable private roots, retain their base revision, and commit as one named undo transaction. Failed edits preserve the previous generation. Public generation/revision/epoch checks protect concurrent artist edits; cancellation and retry records use the existing job lifecycle. Limits: four candidates, 900 idle seconds, 4096 mutations and 64 MiB retained payload per candidate.
- Public native `author` executes the shipped JavaScript SDK with prebound `plan` and `expr`, then stages ordinary Rust-owned operations atomically. Limits: 64 KiB source, 32 MiB heap, 256 KiB stack, two seconds and 1 MiB output. Filesystem, network and process APIs are absent. Stable returned handles support targeted continuation; exceptions leave candidates unchanged. Client-side command construction remains available.
- Package-owned Rust types generate operation schemas and TypeScript declarations for core transactions, Motion and timeline. Exact-time denominators reject zero in the advertised schema. The expression settings schema is authoritative; complete property/settings/inspector generation across all packages remains unfinished.
- Native typed expressions retain dependencies, input types, exact-time sampling and bounded evaluation. Whole-object transforms can consume them without per-glyph Wrangle. Expression AST patches replace the tagged expression atomically.
- Document inspection exposes exact renderable output references and availability; invalid output errors list valid ports. Motion exposes source/transform/appearance roles, targeted object properties, text controls, ordering and validated embedded font import. Existing manual properties survive untargeted SDK edits.
- Text metrics report embedded font identity, shaped clusters and advances, source-local ink/layout bounds and per-line baselines. Text Select provides an editable text sequence driven by a numeric index; Align Content follows changing outline bounds; Shape Points explicitly converts filled geometry into flat grid points with cell-coordinate identities. Native evaluation tests cover sequence replacement, centering and looped point identity.
- `inspect`, `render` and `observe` support private candidates. Observation samples 1–12 exact times from one pinned root, returns an unresampled strip and selected package measurements, and preserves artist playback. Pixel, query and response budgets are enforced.
- Evaluated probes expose recursive counts, top-level domains, transforms, opacity, outline bounds, path signatures and glyph metrics. Parameter probes expose bindings, defaults, keyframes and uniform evaluated values; zero matching domains are diagnosed. Unknown query arguments fail explicitly.
- Sampled movement, direction, displacement, periodicity, nonempty, relative-position and private control-response checks expose measurements and limitations. Periodicity compares identities, paths, paint and transforms; truncated evidence and effects/masks are inconclusive. These checks do not certify pixel visibility or readability.
- Observation evidence includes source-build/profile/target/features, backend, document payload hashes and catalog asset identities. External asset bytes are not freshly verified. Render timing includes attached queries; per-stage costs remain unavailable.
- Initialization and catalog deliver version-matched guidance, SDK source and declarations without repository access. Reference-specific solutions are not shipped as examples.
- Development trials reproduced a renderer defect: 200 ordinary circular cells were rejected by a conservative subdivision estimate. Filled paths now count bounded actual curve subdivisions before tessellation. Triangle, cancellation and precision limits remain enforced; the focused regression and renderer unit suite pass.

## Verification

Latest focused results:

| Check | Result |
|---|---|
| App public automation integration | 10 passed, including native code failure/replay/preservation and candidate observation/commit |
| Motion agent graph integration | 11 passed, including text metrics/selection/points, false-periodicity rejection, relative position and connected/disconnected control response |
| Particle integration after shared sampling refactor | 6 passed |
| Renderer unit tests after circular-cell fix | 15 passed |

Earlier this implementation run, SDK tests, two bounded-runtime unit tests, the analytic bounds test, project candidate tests and the isolated public MCP author/observe/commit/undo script passed. Workspace checking and desktop compilation passed before the latest native nodes and renderer change; the final build/checkpoint remains pending.

A broad checkpoint stopped at an observation job's ten-second timeout while large builds ran concurrently. Runtime hashing of the large executable was replaced by a build-time source identity; the complete ten-test automation suite then passed in 0.39 seconds. This does not replace the remaining broad checkpoint.

Socket tests require an isolated temporary XDG runtime directory outside the sandbox because Unix socket creation is blocked inside it. Color-managed trial sessions required the existing pinned OCIO runtime to be built and packaged. These environment repairs are recorded separately from artwork outcomes.

Strict Clippy previously reported existing media/Motion warnings. A final current-tree Clippy run and normal/narrow native UI review remain unperformed. Existing artist demo modifications are untouched.

## Developmental agent evidence

The initial clean Luna/high text bootstrap completed static `i love katie` in 25 reported calls; it did not establish reference recreation, animation or persistence.

The first reference command trial failed. See [its preserved native artifact, preview and review](evidence/phase-47/baseline-command-01/review.md). It encountered runtime setup, unsupported Text keyframes, geometry-domain mismatches and circular-cell renderer limits. The final sampled artwork was static and mispositioned. The agent's own report is retained alongside the independent failure verdict.

The first SDK trial also failed: [independent full-cycle review](evidence/phase-47/sdk-01/review.md). Its saved native file has been reopened and launched for review; post-trial captures confirm letter fades but inadequate reference fidelity. A second command trial runs on that same pinned executable, and a fresh SDK trial runs on a newer build with native text/point tools and the renderer repair. They are developmental trials: simultaneous builds and environment setup confound performance comparison. Model configuration is requested `gpt-6-luna`/high with no inherited context; resolved runtime identity/tokens are unavailable. Current trials receive no artwork coaching and keep their original time/call budgets. Their results do not constitute the controlled five-per-interface comparison or the final five-of-five gate.

The full reference timing/contact sheet and grading rubric were frozen before artwork trials under [reference evidence](evidence/phase-47/reference/rubric.json).

## Remaining required scope

Every unchecked criterion in [Phase 47](phase-47.md) remains required. Significant gaps include complete authoritative property/settings/inspector generation; explicit descendant/domain/space semantics; broader compositional SDK coverage; dependency-specific candidate conflicts and resource fingerprints; cancellation/race/exhaustion coverage; native candidate comparison UI; crops, provenance, causal pixel-visibility diagnosis and stage costs; visibility interval checks and stronger intended-control-response assertions; full native artist continuation and preview/export/persistence qualification.

The 30-task inventory still needs executable pinned fixtures, held-out allocation, baseline measurements and budgets. Controlled command-versus-SDK trials, five successful clean Luna/high reference recreations on a frozen final build, and the wider repeated task gate remain unperformed. Engineering tests cannot substitute for these acceptance results.
