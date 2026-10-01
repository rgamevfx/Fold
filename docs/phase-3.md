# Phase 3 — Thin desktop shell

## Goal

Implement the single-window Dear ImGui shell from product sections 14–17 through
`fold-ui`, keeping headless builds and creative models independent of UI.

## Tasks

- Integrate compatible dear-imgui-rs/winit/wgpu backends behind `desktop`.
- Provide an initial docked viewer, inspector, and timeline, with stable panel
  identities, keyboard navigation, and a reset-layout action.
- Handle resize, DPI, minimized surfaces, close, and backend errors. Keep all
  panel content explicitly placeholder-only; no project mutations or media work.
- Add focused layout/frame tests and verify the native shell and headless boundary.

## Acceptance criteria

- Desktop builds and presents one native window with the three docked placeholders.
- Docking can be rearranged and reset; resizing and closing work without errors.
- UI has no feature models and the headless app resolves no UI dependencies.
- Focused tests and affected-crate checks pass; limitations are recorded honestly.

## Scope

Use matching dear-imgui 0.18 backends, winit 0.30, and the backend's wgpu 27
compatibility feature. GPU ownership here is limited to UI presentation, not an
engine resource service. Engine textures are phase 4. Layout is session-only (no
project or working-directory ini writes); persistent versioned user layouts,
property editing, IME/accessibility qualification, and the full DPI matrix remain
later UI integration work.

## Verification

Implemented on Linux with Rust 1.95.0:

- `cargo check -p fold-ui -p fold-app --features desktop --all-targets` — passed.
- `cargo test -p fold-ui --locked` — passed. The focused test validates the dock
  tree and builds/reset-layout frames at 1280×800, 640×480, and 1920×1080 without
  a GPU. This tests frame construction, not native input or physical DPI scaling.
- `cargo build -p fold-app --bin fold-desktop --features desktop --locked` — passed.
- `timeout 8s target/debug/fold-desktop` — stayed running until timeout (124),
  with no stderr diagnostics on the available desktop display. This establishes
  startup smoke evidence, not visual or graceful-close acceptance.
- `python3 scripts/check_boundaries.py` — passed against the real all-feature and
  headless dependency graphs.
- Touched Rust files formatted with rustfmt. No existing locked dependency
  versions were upgraded; lockfile growth is the native UI backend graph.

Initial native inspection retry:

- The locked desktop build passed again; the launched `fold-desktop` process
  remained running with no stderr diagnostics.
- `orca-ide` is now available and its runtime was started successfully. However,
  `computer capabilities` reports no screenshot, window-focus, or move/resize
  support on this Linux session. `get-app-state` for the running desktop PID
  returned `app_not_found`; `list-apps` did not expose Fold. Native input and
  visual acceptance therefore remain blocked by the inspection provider, not
  established by the successful startup. The test process was terminated after
  inspection; this does not verify graceful window close.

A subsequent retry with the machine-specific desktop tools established partial
native acceptance:

- Screenshots confirmed one visible Fold window with viewer upper-left,
  inspector upper-right, and timeline below, all explicitly placeholder-only.
- Pointer input visibly focused Inspector; Ctrl+Tab moved focus to Timeline.
- Maximizing and restoring the window resized/reflowed the panels correctly.
- No application diagnostics were emitted during these checks.
- Input automation remained intermittent: several calls timed out after 15
  seconds. Screenshots were inspected after each timeout before further input.
  A press/move/release sequence focused Inspector but did not rearrange it, so
  automated docking/reset acceptance was not established. Minimize attempts did
  not change the visible window.
- The user subsequently confirmed that dragging and moving panels works manually.
  The failed automated drag is not evidence of an application defect: separate
  input sessions were used for press, movement, and release, without verifying
  that the held button persisted.

## Completion and deferred verification

Phase 3 is marked complete at the user's explicit request following the native
checks and manual docking confirmation above. This is user acceptance with
recorded verification exceptions, not a claim that every native check passed.
Native reset-layout interaction, minimize/restore, and normal close remain
unverified and should be exercised at the media slice checkpoint. Reset-layout
frame construction is covered by the focused test described above.

Persistent layouts, IME and accessibility qualification, physical 100/150/200%
DPI checks, engine GPU ownership/texture display, and property edit transactions
remain deferred. Workspace/full-suite/Clippy checks are deferred to the media
slice checkpoint per the development rules.
