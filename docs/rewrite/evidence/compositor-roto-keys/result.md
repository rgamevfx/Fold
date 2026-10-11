# Compact Roto shape keys — 10 October 2026

The selected shape has one compact row above the shape list: previous key, a
filled/outline key diamond that removes/adds the current key, next key, and Auto
key. Navigation is disabled at the ends. Groups and locked shapes cannot receive
geometry keys. Buttons use the shared UI SDK, keyboard focus and tooltips.

Roto Auto key starts enabled independently of the global transform auto-key
setting. Creating or editing a shape keys all point positions, incoming/outgoing
tangents, feather offsets, feather distance and falloff together. Layer transforms
remain separate. With Auto key off, existing keys can be edited; edits between
keys require an explicit new key. A static unkeyed shape can still be edited.
Removing the last key retains the sampled pose as the authored base.

Keys remain native scalar tracks, preserving existing IDs and metadata. The
existing Composition animation adapter exposes them for timing/curve edits; no
second animation store or new timeline panel was added. Navigation uses the
existing frame-based transport, rounding imported subframe times to the nearest
frame. Shape topology changes retain the existing Roto model's behavior.

Validation used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0` and `--offline -j 2`:

- `cargo test -p fold-compositor --features ui --lib roto_ -- --include-ignored --test-threads=1`: five passed. Includes actual button clicks, previous/next traversal, add/remove/undo, complete and shape-scoped key data, default auto-key on creation and viewport drag/cancel/commit/undo. [Log](tests.log).
- `cargo check -p fold-app --features desktop`: passed. [Log](check.log).
- `cargo build -p fold-app --features desktop --bin fold-desktop`: passed; the desktop executable is rebuilt. [Log](build.log).
- Native Dear ImGui/WGPU [normal](normal.png) and [280px narrow](narrow.png) captures reviewed. The new row stays on one line and shows the filled current-key state. No horizontal overflow.

Physical artist acceptance remains open. This focused change does not close a
Motion rewrite phase or qualify large production Roto shapes.
