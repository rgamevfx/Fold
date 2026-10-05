# Phase 33 — Application UI settings

Add a small floating Settings window with a category rail and UI as its first
category. Open through Fold → Settings. Expose live application text size, node
title/label/secondary sizes, graph detail visibility, and named shared theme
colors. Restore defaults and close must remain obvious at narrow window sizes.

The host owns the settings; packages continue using shared typography and theme
components. Persist versioned preferences outside project/workspace archives at
the XDG configuration location. File loading/saving runs off the redraw thread,
with validation and visible errors. No project transaction or undo entry results.

Increase graph text defaults, tighten excess spacing, and retain labels at distant
zoom by default. This does not implement a new collapsed-node representation.

Verify persistence/validation, live role and theme propagation to existing and new
graph panels, and normal/narrow Settings rendering. Review an eight-node graph;
record remaining legibility limits rather than claiming font size alone solves
overview information design. Run affected UI/desktop checks and focused tests.

Implemented:

- Floating Settings window, left UI category, Fold → Settings and Ctrl+, access,
  focus on opening, Escape/Close dismissal and Restore defaults.
- Application text defaults to 15 px. Node titles/port labels/secondary text
  default to 22/22/15 px. Node spacing defaults independently to 18 px.
  Padding, sockets, rounding and minimum width follow spacing; labels use the
  available room before their row/column grows to prevent overlap. Detail hiding
  is opt-in. Existing saved text preferences are retained.
- Seven shared color roles: text, secondary text, background, surfaces, headers,
  borders/grid, accent/selection. Swatches open the standard ImGui color picker
  with numeric RGB/HSV/hex input. Semantic socket/type and warning colors remain
  fixed; these controls do not recolor authored artwork.
- Existing and newly opened panels share the same validated appearance values.
  Versioned preferences live in `$XDG_CONFIG_HOME/fold/ui.json`, falling back to
  `$HOME/.config/fold/ui.json`. Saves debounce, coalesce on a worker and use atomic
  replacement. Unknown fields survive; malformed/newer files are preserved and
  errors appear in Settings. Shutdown flushes the final edit.

Validation:

- `cargo test -p fold-ui --lib -- --test-threads=1`: 74 passed, three native tests
  ignored. Includes five new settings/persistence tests and graph interactions.
- Explicit `graph_typography_renders` real-backend regression passes on Vulkan
  llvmpipe: normal/narrow graph, zoom, 150% UI scale, 2× framebuffer DPI, eight
  nodes, normal/narrow Settings. An initial fixture failure (the last node had no
  output socket) was corrected before the successful run.
- Follow-up: six graph tests (including font/geometry decoupling), five settings
  tests and the nine-case real-backend render regression pass. Reviewed updated
  normal/narrow Settings and eight-node captures.
- Release desktop build passes. Launched Chroma Parade in Motion with the shared
  NVIDIA GTX 1070 GPU backend.
- Touched-file formatting and diff review pass. Workspace checks and Clippy are
  deferred. Native desktop control interaction and hardware-DPI visual acceptance
  remain unverified; captures below use the real backend with software Vulkan.

Reviewed captures: [Settings](media/phase-33-settings.png),
[narrow Settings](media/phase-33-settings-narrow.png),
[eight-node graph](media/phase-33-eight-nodes.png).

The eight-node fixture uses a 1000 px wide panel and a four-column layout.
Extreme zoom or smaller panels still reduce physical letter size. This change
removes automatic label disappearance by default; it does not introduce semantic
zoom, minimum screen-space text size, or a compact overview node representation.
