# Timeline, Compositor, and Motion UI cleanup

## Goal and constraints

Apply `.agents/skills/fold-ui/SKILL.md` and product architecture §§10, 13–15:
quiet one–two-row panel headers, contextual tools, artist-facing names, and
editable depth. Preserve commands, preview/edit sessions, undo, graph access,
source context, and essential errors. No authoring model or rendering changes.

## Tasks

- [x] Shared SDK toolbar icons, hover/focus tooltips, and compact popup menus.
- [x] Timeline: one-row toolbar with Edit, Tracks, and View menus. Wide panels also
      expose Split, Lift, Snap, and Linked; narrow panels retain them in menus.
      Fit remains an icon; gesture instructions live in the help tooltip.
- [x] Compositor: compact Composite menu and viewer action, without a full-width
      UUID selector. Document names use available metadata or numbered fallbacks.
- [x] Motion: Motion / Add / Modify menus and viewer action. Repeat, position wave
      scope, radial falloff, reusable grouping, and sequence insertion remain available.
- [x] Shared graph row: Add, Frame All, and overflow actions/help; contextual group
      navigation remains available. Tab/F shortcuts and graph gestures are unchanged.
- [x] Inspectors: remove object IDs and routine status/helper rows; move Animate
      and Publish into property menus. Show driver and font names, readable property
      labels, and artist-facing scope choices. Text alignment offers named presets
      while retaining custom continuous values.
- [x] Move the bundled font notice to **Motion > Font licenses...**, a separate
      scrollable dialog, rather than a text-node setting. License resource retained.
- [x] Remove nested-source UUID/debug-time labels from timeline presentation.
- [x] Focused affected-crate compilation and interaction/regression tests.
- [x] Inspect normal-size native headers for all three panels; test narrow layouts
      in real ImGui frames without altering the project.
- [ ] Complete native narrow-panel/menu/inspector interaction acceptance; see limits below.

## Verification

Completed 1 October 2026. Existing skill/AGENTS changes predate this task.

Passing focused tests:

- `cargo test -p fold-ui sdk:: -- --test-threads=1` (shared graph gestures,
  property identity/edit lifecycle, SDK registrations); newly added
  `sdk::toolbar::tests::compact_menu_opens_and_activates_once` also passes.
- `cargo test -p fold-compositor --features ui ui:: -- --test-threads=1`
- `cargo test -p fold-motion --features ui ui::tests:: -- --test-threads=1`
- `cargo test -p fold-timeline --features ui ui:: -- --test-threads=1`

The new/extended panel tests cover 850 px and 320 px widths, no horizontal
scroll overflow, and no project changes from drawing. Compositor additionally
checks graph availability below two header rows. Motion tests verify a custom
alignment preview whose control disappears commits once, retains its exact
value, and can be undone. Existing tests cover graph selection/movement,
preview/cancel/commit, timeline move/trim/seek, and atomic feature edits.

Desktop build passes: `cargo build -p fold-app --bin fold-desktop --features desktop`.
The initial build lacked ALSA development files; an externally extracted package
and pkg-config environment resolved it, as in the earlier reference-machine
setup. No machine-specific paths/dependencies were added to project configuration.
Touched Rust files are formatted; `git diff --check` passes. Full workspace tests
and Clippy remain deferred per the project's iteration rules.

Native screenshots confirmed a one-row Timeline header and two-row Compositor
and Motion headers. Evidence is outside the repository under
`~/.local/state/agent-desktop/screenshots/fold-ui-cleanup/`; test project copies
and desktop logs are under `~/.local/state/fold-ui-cleanup/`.

**Remaining verification:** the libei mouse-move command timed out. A recapture
showed no node selection, so native menu clicks, tooltips, license-dialog scrolling,
inspector interaction, and narrow-size visual inspection are not claimed as
verified. The automated ImGui/transaction tests above passed independently.
Inspection app instances were closed; original test projects were not modified.
