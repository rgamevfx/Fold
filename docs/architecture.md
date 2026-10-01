# Workspace ownership and dependency policy

The workspace follows sections 3, 16, and 17 of
`Fold_Application_Product_Architecture.docx`. All crates are statically linked.
Crate libraries currently contain ownership documentation only: there are no
placeholder domain types, registries, renderers, or UI APIs to depend on yet.

| Crate | Responsibility | Allowed Fold dependencies |
| --- | --- | --- |
| `fold-foundation` | Stable IDs, rational time, math, diagnostics | None |
| `fold-project` | Generic envelopes, assets, revisions, transactions, persistence | foundation |
| `fold-media` | Media types/adapters, separate video and audio concerns | foundation |
| `fold-render` | Render IR, scheduling, caches, GPU/resource ownership | foundation, media |
| `fold-platform` | Commands, capabilities, explicit services, static registration contracts | foundation, project, media, render |
| `fold-ui` | Shared Dear ImGui UI SDK, components and presentation | foundation, platform |
| `fold-timeline` | Sequences, tracks, clips, commands and compilation | foundation, project, media, render, platform |
| `fold-compositor` | Typed authoring graphs, commands and compilation | foundation, project, media, render, platform |
| `fold-motion` | Geometry, fields, controls, scene authoring and compilation | foundation, project, media, render, platform |
| `fold-app` | Desktop/CLI assembly of concrete implementations | All |

Dependencies flow toward shared contracts/infrastructure. Creative packages are
peers and never import one another. Project and render know no feature models.
Cross-feature references must use stable IDs and capability/service contracts.
Only the app wires concrete implementations together. The UI does not own
project state or authoritative render logic; future controls submit commands and
edit sessions through the platform. Render workers use immutable snapshots.

## Build and validation

```sh
cargo run -p fold-app --bin fold-cli --no-default-features --locked
cargo run -p fold-app --bin fold-desktop --features desktop --locked
python3 scripts/check_boundaries.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
```

The default workspace member is app, with no default features. `desktop` opts
into `fold-ui`; the desktop target cannot build without that feature. Both
binaries only announce scaffold status. No Dear ImGui, GPU, window, or codec
dependency is selected yet. Phase 3 will choose compatible UI backends. If feature
packages later contribute UI, add explicitly optional UI dependencies and update
the policy deliberately, preserving the headless graph check.

`scripts/check_boundaries.py` checks Cargo metadata for all features and for no
default features. It checks every declared workspace dependency, including
renamed, optional, target-specific, dev, and build dependencies. Resolved
transitive dependencies must not introduce creative packages or UI into shared
infrastructure, peer implementations or UI into creative packages, or UI/ImGui
into the headless app. Adding a workspace member
requires an explicit policy entry. CI runs this checker and its regression tests
alongside the workspace build/test checkpoint.

This enforces crate dependency boundaries, not runtime semantics. Transactions,
unknown-data preservation, snapshot evaluation, and resource ownership require
implementation and behavioral tests in subsequent phases. Keep future public
contracts separate from service implementations; do not create a universal
creative model in infrastructure.
