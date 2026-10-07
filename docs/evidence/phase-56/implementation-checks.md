# Phase 56 implementation verification

2026-10-07. This records the initial vertical slice, not completion of the broader mograph language proposal.

## Delivered behavior

- Rust source bodies compile through the existing bounded Rust/Wasm compiler into SDK v1 descriptions. Native validation lowers descriptions to persistent groups, typed ports, fields, stable node IDs and source metadata. Existing Wrangles provide custom runtime algorithms.
- Stable control IDs preserve instance values, links, keys, placement and extensions. Generated node layout survives revisions. Removed/retyped controls and semantic edits to generated internals produce conflicts. New defaults affect new instances; new controls receive defaults on existing instances.
- `fold.app.create-rust-asset` creates a hidden Motion source plus its common Composition layer in one transaction. Compilation runs on a worker. Public candidate, catalog, inspection and observation use the existing versioned transport.
- Native controls appear in the normal inspector. “Edit Rust source” opens a contextual editor; background compilation applies one transaction only if its pinned source document remains current. Cancellation drops the job and cancels its token. Source entry is disabled while compiling so visible code cannot drift from the in-flight program.
- The shared compiler preserves cold-request deduplication; simultaneous identical compilation requests reuse the compiled module rather than queueing redundant rustc processes.

## Focused checks

- `cargo test -p fold-motion --test program --test wrangle --offline`: 4 Rust-asset tests and 10 Wrangle tests passed; one existing manual performance comparison ignored. Covers evaluated movement, native keys and connected drivers surviving revision, stable identities/layout, new-default semantics, metadata/reload, interface/internal conflicts, failed compiler/fuel/macro atomicity, cancellation before work, text/Points/Wrangle composition, existing Wrangle isolation and exact-time behavior.
- `cargo test -p fold-app --test rust_asset --offline`: public batch creates common Composition and matching-format source, saves/reloads source, and restores exact documents through undo/redo. Passed.
- Native ImGui source-result test verifies one transaction and stale-result rejection without overwriting an artist edit. Passed.
- Normal/narrow inspector and source-window captures use the actual native Vulkan backend (llvmpipe), saved under [ui/](ui/). The fixture creates a real scene-owned asset. These are native component captures, not a launched full desktop screenshot or proof of physical keyboard/mouse accessibility.
- Release `fold-cli` and `fold-desktop` build passed. Fresh isolated stdio MCP discovered the new guides and SDK. Local sockets require execution outside this environment's sandbox; the isolated smoke was approved by automatic review.

The targeted `ui::` checkpoint with native captures enabled passed all 28 tests, including the new source-result lifecycle test and existing undo, cancel, selection and source-local-time workflows. Final release build and artwork trial details are recorded in the phase document. Unrelated `fold-render` feature-combination dead-code warnings (`Layout.format`, `Layout.parameters`) remain; no workspace-wide Clippy or full suite was run.

## Limits

No claim of sustained playback speed, hardware GPU throughput, export throughput or comparison with fframes follows from these checks. There is no second renderer. Source-to-native arbitrary graph merging, parameter migration, soft control ranges, asset-library packaging and richer timing/layout helpers remain future work. The compiler uses the installed rustc/wasm32/prlimit toolchain; runtime Wrangles still require it on cache miss. Mid-rustc cancellation and the desktop Cancel button have not been exercised interactively in this slice; pre-cancel and stale-result safety are tested.

The artwork trial uses a frozen executable hash. Compiler cache deduplication and source-editor-only refinements made during independent review are verified separately; the trial log must not be represented as having used a later binary.

Final documentation-embedded release hashes and the 43 focused passing tests are recorded in [final-build.json](final-build.json). A fresh isolated MCP smoke confirmed that this release serves the updated verified-workflow guide. Diff whitespace checks passed; unrelated artist changes to `demos/timeline-playground.fold` were preserved.
