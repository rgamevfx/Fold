We are building Fold, a modular VFX, Editing, and Graphics engine and application.

For product scope, architecture, or workflow decisions, consult the relevant sections of [the product and architecture proposal](docs/Fold_Application_Product_Architecture.md). Its diagrams are linked from `docs/media/`; current implementation decisions and phase priorities are in [dev-plan.md](dev-plan.md).

Before creating or changing UI, read and apply the [fold-ui skill](.agents/skills/fold-ui/SKILL.md), including UI work within larger features.



\# Development rules

\- Use Rust for application logic and Dear ImGui through the shared UI SDK.

\- Keep timeline, compositor, and motion graphics as independent feature packages.

\- Keep project and render infrastructure free of feature-specific models.

\- Connect packages through stable IDs, capabilities, and explicit services.

\- Route project mutations through transactions with undo support.

\- Render from immutable snapshots; never block the UI with media processing.

\- Use exact rational time and explicit color, alpha, and resource ownership.

\- Share evaluation semantics between preview, export, and headless operation.

\- Preserve unknown extension data when loading and saving projects.

\- Prefer small, direct changes; add abstractions only for demonstrated needs.

\- Run relevant checks and report failures or unverified behavior.



\- Make the smallest complete change; avoid unrelated edits.

\- Prefer explicit code; avoid speculative abstractions and unnecessary dependencies.

\- Keep modules focused on one responsibility; split by responsibility, not arbitrary size.

\- Reuse existing logic; remove code made obsolete by your change.

\- Before finishing, review the diff for duplication, unnecessary complexity, and unrelated changes.



\- During iteration, format touched files and check only affected crates.

\- Run focused tests for changed behavior; avoid repeating checks on unchanged code.

\- Defer workspace checks, full test suites, and Clippy to vertical-slice checkpoints.

\- Fix failures introduced by the change; report unrelated failures and deferred checks.
