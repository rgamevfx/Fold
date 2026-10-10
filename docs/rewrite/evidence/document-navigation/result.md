# Document creation and graph navigation — 10 October 2026

The audit reproduced missing Project creation entries and bare footage layers through the actual application commands. `document_workflows::project_new_exposes_all_creative_documents` initially failed on the Timeline entry; `project_placement::common_composition_accepts_still_assets_as_full_duration_layers` initially failed because dropped footage had no editable graph.

## Findings and changes

- Project's New menu is provider-driven. Compositor and native Motion had Creation implementations but did not register BrowserKind. Both now register; Timeline uses the artist-facing name Timeline. Scene remains available.
- Timeline's clip context menu did not invoke the existing cross-feature composition command. New composition now submits that command with the clicked/selected clips and current revision.
- Footage placement and timeline conversion stored bare asset references. Application assembly now creates a native Read → neutral Color Grade → Output graph and attaches it to each footage layer in the same transaction. Providers remain independent. Existing document sources are retained directly.
- Composition graph selection depended on the internal-graph visibility extension. It now queries an explicit desktop Network capability, forwarded through panel contexts. Standalone graphs therefore link to Network without becoming hidden project items. Internal graphs retain their existing project visibility behavior.
- Timeline-created compositions now receive a named Project item in the timeline's bin. Layers retain their source item names. Exact timing, source offsets, opacity, effects and linked audio behavior remain authoritative in the existing owning documents.
- Timeline tooltip hit tests now respect popup/window hover ownership, preventing clip tooltips from covering context-menu actions at narrow widths.
- Existing bare footage layers remain preserved. Their Layer > Add image processing graph action still creates a graph explicitly; attempting to open a bare layer now points to that action.

## Verification

- `cargo check -p fold-app --features desktop` and `cargo check -p fold-app --features desktop --tests` passed.
- The final focused pass passed all 16 tests across `document_workflows`, `project_placement`, `compositor_workflow` and `compositor_media`, including image/video placement, immutable rendering, audio preservation, persistence, rejected transactions and undo/redo.
- The video/export test also passes with nonzero rational occurrence and source offsets.
- Composition UI: 11 tests passed, including standalone graph selection/open through a panel context, retained parent navigation, source clocks, cancellation and undo. Timeline UI: 9 tests passed, including a real ImGui right-click and activation of New composition.
- Native ImGui/WGPU Timeline captures run through Vulkan llvmpipe. The new menu was reviewed at 1024 px and 320 px panel widths, with readable action labels and no tooltip covering the popup. Physical desktop/GPU interaction is not implied by this offscreen capture.
- Formatting and diff whitespace checks passed. The UI-only build emits existing unused YUV field/method warnings. Workspace-wide tests and Clippy were not run: this is a focused repair, not a rewrite vertical-slice checkpoint.

[Normal Timeline menu](menu-normal.png) · [Narrow Timeline menu](menu-narrow.png)

## Limits

Timeline conversion still requires video clips on one unlocked track and rejects unsupported selections atomically. Linked audio stays on the original timeline. No automatic migration of existing documents was introduced. Project menu creation is verified through its actual provider catalog/application commands and desktop typechecking; a live Project-menu click walkthrough was not performed. Physical artist acceptance and the existing Motion phase gates remain open; this repair does not close M3–M5/M5a–M5c or start M6.
