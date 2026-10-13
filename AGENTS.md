# Working on Fold

Fold is a Rust video editor, compositor and graphics application with Dear ImGui UI.

## Read for the task

- Architecture or cross-package changes: [system map](docs/architecture.md).
- Motion/3D replacement, rendering integration or acceptance work: [current work](docs/current-work.md).
- Build, SDK, packaging or validation setup: [development](docs/development.md).
- UI changes, including labels and interactions: [fold-ui skill](.agents/skills/fold-ui/SKILL.md).
- Artwork, demos or public agent workflows: [Fold skill](skills/fold/SKILL.md). Artwork must use public tools, not Rust internals or saved-payload edits.

## Engineering constraints

- Keep feature models in their owning packages; connect them through stable IDs and explicit services. Fold owns documents, evaluation and exact rational time; rendering engines stay behind adapters.
- Artist and agent mutations use the same guarded transactions and Undo. Render immutable snapshots; keep media processing and compilation off the UI thread.
- Keep color, alpha and resource ownership explicit. Preserve unknown extension data and unsupported documents through generic opaque storage.
- Replace superseded implementations rather than adding compatibility engines, protocol aliases or fallback renderers. Retain independently used shared services.

## Validation and documentation

- Format touched code and run affected checks or regressions. Exercise real Session lifecycle bugs at that seam. Use optimized builds for artist review.
- Run workspace checks, tests and Clippy at integration checkpoints; report omitted checks and unrelated failures. Tests, rendered pixels and native artist acceptance are separate evidence.
- Update the owning document, not a new handoff or report. Keep open replacement work in `docs/current-work.md`; store detailed results with fixtures. Documentation structure and retention rules are in [docs/README.md](docs/README.md).
