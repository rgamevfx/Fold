# Phase 13 — Project browser and reference placement

## Goal
Replace import-centric desktop scaffolding with a quiet file-explorer-style Project panel. Bins act as folders; a flat thumbnail grid shows the current bin and a list presents collapsible bins. Import links sources; placement separately authors references.

## Boundaries and implementation
- Preserve the phase-12 working-tree implementation and all provider-owned payloads.
- Shared SDK browser, compact navigation/search/view controls, contextual import/creation/organization and keyboard equivalents. Browser selection/expansion is UI state.
- Registered document creation and placement capabilities; generic shell does not enumerate creative packages.
- OS file drops target Project bins only. Internal typed drops target bins and compatible editors. Explicit destination/time/track/range; validation and one transaction per gesture.
- Worker-only file selection/ingest/thumbnail work with bounded concurrency and storage. Explicit existing SDR presentation policy until phase 16 production color work.
- Retain selection inspectors; retire path-entry import controls only after replacement verification.
- Panel instances and independent viewers remain phase 14/15. Unsupported provider/source combinations must fail visibly, not flatten or fabricate support.

## Acceptance and verification
- Import → organize → reuse in two documents → rename/move → undo/redo → save/reopen preserves identities, sources, references and evaluation content.
- Focused tests for locked/incompatible targets, source ranges, dependency cycles, referenced deletion, unavailable providers and transactional rollback.
- Desktop build, narrow/normal panel and keyboard inspection; native OS drop and internal placement acceptance.
- At the slice checkpoint run workspace tests serially, Clippy and package-boundary checks; distinguish inherited failures and unverified behavior.

## Implementation

### Browser and commands
- `app/src/project_panel.rs` is a shared-SDK contribution in a dedicated left-hand Project dock. It has a flat current-folder thumbnail grid and a virtualized, collapsible tree/list; bins draw as folders. A single row holds parent/ancestor navigation, view toggle and search. No permanent import/path-entry controls were added.
- Right-click exposes Import, New Bin, registered document creation, Open, Rename, Move, Relink, Delete and Place Into. Ctrl+I, Enter, F2, Delete, Alt+Up, tree expansion keys and Shift+F10 provide keyboard routes. Browser selection, folder, search and expansion remain presentation state, not project content or undo entries.
- Organization commands preserve item IDs and extension fields, run through project transactions and use the phase-12 guarded deletion service. Import/relink run on the existing bounded background worker; duplicate import reveals the existing item instead of adding or moving it. Native file selection uses the optional `rfd` XDG-portal backend off the UI thread.
- Timeline selection properties remain; the MEDIA/path-entry/append-import block is removed and its tab is named Selection. Ordinary desktop file arguments now import assets only. Explicit legacy `--timeline`/`--layers` fixture switches and headless APIs remain for regression evidence, not as dependencies of the Project workflow.

### Placement and ownership
- `platform/src/browser.rs` defines typed entry/source/placement requests; provider-owned create/place capabilities live in the package registry. The generic shell does not enumerate creative document implementations.
- Timeline placement reuses verified AssetIds and document video references, maps exact insertion/source-in/duration, validates the target track, and creates linked A/V in one transaction. Dropping video onto an audio lane selects audio only. Compositor placement creates an editable Read node without changing existing wiring. Motion outputs can be reused in both without flattening.
- SDK drag payloads share these commands with keyboard/context-menu placement. File drops only import into Project; drops on other panels report that files must first be imported. Bin moves, invalid ranges, locked tracks, source/dependency failures and recursive references fail without publishing partial edits. Dependency validation is shared with evaluation.
- Import publication retains its cancellation token through commit. A completed worker must not cancel its own proposal when disposed; a dedicated Session regression test covers this. Active preview edits cause ingest publication to be rejected rather than silently overwriting the gesture. Closing the host does not join a blocked native chooser.

### Browser previews
- One worker and one result slot produce 128×72 RGBA previews; the LRU holds at most 128 context-owned textures (4.5 MiB of image pixels, excluding backend overhead). Only visible rows request previews. Requests are not queued without bound, and stale source identities are not displayed under a new identity.
- Video/PPM thumbnails use the existing explicit SDR sRGB source-presentation policy. Video limited-BT.709 reconstruction precedes the sRGB transfer; this is not an ACES working image and is never used by export. WAVE shows a bounded first-30-second waveform. Document tiles use type indicators rather than pretending to have evaluated previews.
- Source inspection, fingerprinting, decoding and waveform generation run off the UI thread. The worker revalidates the narrow source profile, bounds codec allocation/output and rechecks identity after decoding. Cancellation is supervised by the existing codec process service. Missing/changed media and unavailable providers get visible warning indicators.

### Linux file-drop backend
The pinned winit 0.30.13 implements Linux file drops on X11, not its Wayland backend. The Linux desktop therefore uses X11/XWayland for this implementation. Its XDND events also omit cursor-position updates, so a small X11 input adapter queries the actual window-relative pointer during external drags and at drop; it never guesses the bin from a stale ordinary mouse event. OS file drops are batched (up to the phase-12 32-file limit), and the destination folder is highlighted. This is a deliberate backend compatibility route, not a claim of native-Wayland DND support.

## Verification
- Focused placement fixtures cover asset-only Session import, duplicate/no-op import history, worker publication, linked A/V placement, reuse in timeline and compositor, motion references, guarded deletion, rename/move content-key stability, undo/redo and save/reopen.
- Shared-SDK ImGui tests cover tree preorder, expansion/filter state, 600/240/180-pixel widths without horizontal overflow or project mutation, actual typed mouse-drag delivery into a bin and one-step undo, keyboard folder opening, and explicit OS-drop endpoint/bin hit testing. These are simulated UI events, not native file-explorer acceptance.
- Thumbnail fixtures cover fixed-size PPM/WAVE previews, sRGB sample preservation, cancellation, metadata mismatch, missing files and changed fingerprints. The placement fixture also decodes a real H.264 thumbnail.
- Native desktop launch and a populated thumbnail grid were inspected on the GTX 1070/Vulkan adapter (driver 580.173.02). Video thumbnails, a PPM swatch, waveform, folder icon, document items and the compact toolbar were visible. Local evidence is outside the repository at `~/.local/state/agent-desktop/phase13/`, including `Screenshot_2026-10-01_22-21-57.png`.
- Checkpoint: `cargo test --workspace --all-features -j 2 --no-fail-fast -- --test-threads=1` passed **141 tests**, with **1 existing native-audio test ignored**. After final UI/diagnostic cleanup, app library + ingest + placement tests passed again (13 tests); a final headless placement/Session run passed all 3 tests. These reruns are not additional unique tests.
- `cargo clippy --workspace --all-targets --all-features -j 2` completed. Introduced warnings were corrected; the affected-crate rerun leaves 10 inherited Motion warnings and 1 pre-existing phase-12 compositor relink `collapsible_if` warning. No new Clippy warnings remain.
- `python3 scripts/check_boundaries.py` passed, including headless UI isolation. Touched Rust files were formatted with `rustfmt --edition 2024 --config skip_children=true`; `git diff --check` passed. Desktop commands used the existing local ALSA `PKG_CONFIG_PATH` documented in phase 12.
- Initial checks caught and fixed a missing new-action arm in a timeline test, the completed-worker cancellation bug, and an undo-test assertion that incorrectly expected the project revision not to advance. The passing results above are after those corrections.

## Dock-drag crash follow-up

A user reported an immediate crash when grabbing a panel for redocking. A deterministic mouse-event test through the real ImGui shell reproduced a native SIGABRT in `ErrorCheckUsingSetCursorPosToExtendParentBoundaries`. The same dock gesture passed with the new canvas Project-drop overlay disabled, isolating the regression to `ui/src/project_drop.rs`, not the shell's dock layout or graphics backend.

The overlay restored a canvas's next-item cursor (which can lie beyond its last submitted item's bounds due to trailing spacing) without submitting an item there. ImGui's own docking payload also executes that path. The fix submits a zero-sized layout item at the restored position, then preserves the caller's cursor; it does not suppress assertions or reinterpret docking payloads as Project entries.

`ui/src/docking_tests.rs` covers the actual dock-tab press/drag/release with and without the overlay, verifies a docking payload really exists and is not delivered as a Project item, and verifies a normal Project drag delivers exactly once without moving the subsequent layout cursor. The dock-drag test aborted before the fix and passes afterward. All 23 `fold-ui` tests passed serially, and `target/debug/fold-desktop` was rebuilt. These tests drive real ImGui mouse events, not native OS input; the separate native file-drop acceptance limitation below is unchanged. Strict `fold-ui` Clippy passed with all features. The default-feature strict check instead flags the existing `desktop.rs` redraw-error `return` as unnecessary when native-probe code is compiled out; that unrelated conditional-build warning was left unchanged.

## Remaining acceptance / limits — phase is not checked off
- Native right-click/keyboard/drop workflow acceptance is **unverified**. The prescribed `wdotool --backend libei` route intermittently timed out or returned `no relative-pointer device from EIS`. Captures were inspected after failures; no unsupported input backend was substituted. A screenshot of the populated app is not proof of file-dialog operation, OS drag-and-drop, or native cross-panel undo/persistence.
- Motion currently has no raster/media input capability. Drops into it are rejected honestly; its outputs are reusable in compatible timelines/compositions. Implementing a media-input model in Motion is not represented by a fake node or flattened intermediate. This limitation is not an approved completion of the user's requested Motion destination workflow.
- The existing narrow placement profile is retained: H.264 video in timeline/compositor and WAVE audio in timeline. PPM can be imported and previewed, but those creative providers do not yet support still-image asset placement. Only the supported primary media streams and document `video` output are exposed; no arbitrary stream/output remapping is claimed.
- Browser interaction is initially single-item selection/drag. Missing-source checks occur during preview generation, not through a continuous filesystem watcher. Verified metadata is required for placement; legacy assets without it fail clearly, consistent with phase-12 conservative relink policy.
- Workspace persistence, multi-editor instances/independent viewer clocks and production ACES processing remain phases 14–16. No responsiveness benchmark, real-time performance, broad codec/platform support, or native-Wayland DND acceptance is claimed.
