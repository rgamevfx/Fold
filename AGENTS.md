We are building Fold, a modular VFX, Editing, and Graphics engine and application.

## Active work

For motion, rendering integration, MCP, CLI authoring, agent protocols, SDKs or motion demos, read the [Motion System Rewrite Plan](docs/Motion_System_Rewrite_Plan.md) and relevant sections of its [architecture and node-set proposal](docs/Fold_Motion_Architecture_and_Node_Set_Proposal.md). The plan owns phase order, removal scope, acceptance and current status.

This is a complete motion-system and surrounding agent-protocol rewrite. Delete superseded implementations and dependencies as required by the plan. Do not retain legacy engines, SDKs, protocol aliases, compatibility shims or fallback paths. Preserve historical source through Git; preserve unsupported project payloads through generic opaque-document handling.

For shared product scope and non-motion workflows, consult the relevant sections and diagrams of the [product specification](docs/Fold_Application_Product_Architecture.md). Use [architecture.md](docs/architecture.md) as the pre-rewrite implementation inventory until updated with phase evidence. The [retirement register](docs/Retired_Documentation.md) identifies historical documents. `dev-plan.md` and old phase records are evidence only; do not resume their work or treat their acceptance as rewrite acceptance.

Before creating or changing UI, read and apply the [fold-ui skill](.agents/skills/fold-ui/SKILL.md), including UI work within larger features.

When creating or refining `demos/` workflows or MCP skills, use and update the [Fold agent skill](skills/fold/SKILL.md) and its references with verified public-tool examples. During the rewrite, replace old protocol guidance as capabilities qualify; track missing capabilities and evidence in the rewrite plan. Artwork workflows must run without reading Rust crates.

## Development rules

- Use Rust for application logic and Dear ImGui through the shared UI SDK.
- Keep Timeline, Compositor and Motion as independent feature packages. Keep project and render infrastructure free of feature-specific models.
- Connect packages through stable IDs, capabilities and explicit services. Keep Bevy runtime state behind the Fold rendering adapter; Fold owns document state, evaluation and time.
- Route project mutations through transactions with undo support. Render from immutable snapshots; never block the UI with media processing.
- Use exact rational time and explicit color, alpha and resource ownership. Share evaluation semantics between preview, export and headless operation.
- Preserve unknown extension data when loading and saving projects.
- Make the smallest complete phase change. Prefer explicit code, reuse independently valid shared services, and add abstractions only for demonstrated needs.
- Keep modules focused on one responsibility. Remove obsolete code, tests, dependencies and documentation in the phase that replaces their behavior.
- Before finishing, review the diff for duplication, unnecessary complexity and unrelated changes.

## Validation and evidence

- During iteration, format touched files and check affected crates. Run focused tests for changed behavior; avoid repeating checks on unchanged code.
- Run workspace checks, full test suites and Clippy at the rewrite plan's vertical-slice checkpoints.
- Fix failures introduced by the change. Report unrelated failures, deferred checks and unverified behavior.
- Update the rewrite plan's phase status and evidence before claiming completion. Distinguish implemented behavior, automated verification and native artist/agent acceptance.
