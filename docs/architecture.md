# Workspace ownership and dependency policy

The workspace follows sections 3, 16, and 17 of
`Fold_Application_Product_Architecture.docx`. All crates are statically linked.
Foundation implements stable IDs and exact rational time. Project implements
feature-neutral document/asset records, immutable snapshots, transactions,
bounded undo/redo, preview sessions, and versioned persistence. The foundation
checkpoint adds a motion-owned solid source, static video-provider registry, and
bounded CPU solid rendering with explicit linear-sRGB to RGB8 PPM delivery.
The phase 3 shell adds a private Dear ImGui/winit/wgpu presentation backend in
`fold-ui`, exposed to app through `run_desktop(preview)`. Phase 4 adds a one-shot
worker-rendered solid demo in the viewer; inspector and timeline remain docked
placeholders, not creative package models. App assembles the committed source and
provider; render produces owned opaque sRGB display bytes, re-exported by platform.
UI polls a bounded result channel and owns only the uploaded presentation copy.

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
cargo run -p fold-app --bin fold-cli --no-default-features --locked -- checkpoint demo.fold demo.ppm
cargo run -p fold-app --bin fold-cli --no-default-features --locked -- render demo.fold rerender.ppm
cargo run -p fold-app --bin fold-desktop --features desktop --locked
python3 scripts/check_boundaries.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
```

The default workspace member is app, with no default features. `desktop` opts
into `fold-ui`; the desktop target cannot build without that feature. Both
use separate entry points: the desktop opens a single docked native window; the CLI supports
`checkpoint` (create/edit/undo/save/reopen/render) and `render` (reopen/render).
The initial CLI requires exactly one document, uses its `video` output at exact
time zero and 64×64 resolution, and refuses to overwrite image outputs. Checkpoint
also refuses an existing project path. Desktop uses dear-imgui-rs, dear-imgui-winit,
and dear-imgui-wgpu 0.18 with winit 0.30 and wgpu 27. UI GPU resources are currently
presentation-only: phase 4 uploads a CPU engine result once into a host-registered
texture, with no engine GPU pool or zero-copy claim. The opaque SDR path requires a
non-sRGB RGBA8/BGRA8 surface to preserve the engine's exact output transform and
avoid the UI backend's approximate gamma correction. Layouts
are session-only and never written to project content or a working-directory ini.
No codec is selected. If feature packages later contribute UI, add explicitly
optional UI dependencies and update
the policy deliberately, preserving the headless graph check.

`scripts/check_boundaries.py` checks Cargo metadata for all features and for no
default features. It checks every declared workspace dependency, including
renamed, optional, target-specific, dev, and build dependencies. Resolved
transitive dependencies must not introduce creative packages or UI into shared
infrastructure, peer implementations or UI into creative packages, or UI/ImGui
into the headless app. Adding a workspace member
requires an explicit policy entry. CI runs this checker and its regression tests
alongside the workspace build/test checkpoint.

This enforces crate dependency boundaries, not runtime semantics. Foundation and
project behavioral tests cover exact time, transaction atomicity, immutable
snapshots, history, preview isolation, and unknown-data preservation (phase 2).
Checkpoint tests cover solid pixels, output color conversion, CPU allocation
limits, provider errors, snapshot isolation, and CLI persistence/render equivalence.
Graph evaluation and GPU resource ownership need tests in subsequent phases. Keep
future public contracts separate from service implementations; do not create a
universal creative model in infrastructure.

## Project state and persistence

`Project` is the single writer. Callers submit `EditBatch` values against an
expected revision; declared document/asset references and cycles are checked
against the complete staged state before publication. Unchanged document roots
are shared via `Arc`. Undo/redo restores content under a fresh project revision,
so old asynchronous proposals remain stale. History has a configurable entry
limit (not a byte budget). `EditSession` holds a transient overlay; updates replace
the staged batch, dropping cancels, and committing adds one history entry.
Preview state cannot be passed to the committed-snapshot save API.

Persistence is synchronous I/O intended for worker threads using pinned snapshots.
The v1 `.fold` archive is a JSON container with a versioned manifest and independent
opaque payload byte arrays. Save flushes and syncs a same-directory temporary file,
atomically replaces the destination, and on Unix syncs its parent directory.
`SaveError::DirectorySync` explicitly means replacement occurred but durability
could not be confirmed. An unfinished temporary write does not replace the old
archive. Linux replacement behavior is tested; cross-platform crash durability and
recovery snapshot retention remain hardening work.

Unknown document schemas need no installed provider to load/save. Payload bytes,
unknown JSON fields, dependencies, and asset records are retained; the core neither
migrates payloads nor automatically prunes assets. Known typed schemas, capability
and port validation, and provider migrations belong to later package integration.
Missing external media is retained in the manifest; missing declared manifest IDs
are errors. Undo history, preview overlays, caches, and workspace layouts are not
persisted. See `docs/phase-2.md` for acceptance evidence and deferred scope.
