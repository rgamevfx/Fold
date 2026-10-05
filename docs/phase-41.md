# Phase 41 — Project menus, workspace presets and application preferences

File now owns New Project, Open, Open Recent, Save, Save As, Close Project,
Import and Quit. Edit owns Undo/Redo. Workspace owns named presets and layout
reset. Fold → Settings remains the preferences entry point. Delivery retains its
existing export controls; surface-specific exporting is a separate design task.

Project lifecycle:

- Native Open/Save As choosers and archive I/O run on background workers. Save
  uses the current association; Save As establishes a new association only after
  success. The most recent 20 successful project associations persist.
- The window title shows the project filename and an unsaved marker. Dirty state
  compares authored content with the snapshot actually saved, ignoring publication
  revisions, so undo back to saved content becomes clean.
- New/Open/Close/Quit and the native window-close button share Save/Discard/Cancel
  protection. Cancelled/failed saves and edits made during saving cannot complete
  the pending replacement or quit. Opening also rejects intervening edits.
- Undo/Redo menu items reflect history availability. Ctrl+Z/Ctrl+Shift+Z and project
  shortcuts do not intercept active text entry. Project-operation failures have a
  visible dialog; unavailable destructive actions wait for background jobs.

Workspace presets:

- Save As, Update Current, Restore Saved, Rename, Delete and Reset to Default are
  available under Workspace. Up to 32 named presets persist in user preferences.
- Presets store layout, panel instances, viewer preferences and link configuration.
  They exclude project paths, document references, selection, inspector locks,
  playheads and review ranges. Matching editors retain the current project's
  targets when applying a preset. Missing panel contributions remain placeholders.
- Per-project sidecars still restore project-bound sessions, including the active
  preset name. Closing/opening clears stale targets and runtime viewer/editor state.
  Reset restores the built-in panel arrangement without modifying project content.

Application settings:

- A new Application category sits beside UI. Automatic uses the existing
  conservative allowances: 1024 MiB shared GPU, 256 MiB viewer cache within that
  allowance, and 768 MiB CPU image memory. Automatic is a conservative default,
  not a measurement of available hardware memory.
- Manual GPU, viewer-cache and RAM-image budgets are validated and applied on
  restart. The viewer budget cannot exceed half the shared GPU budget. RAM splits
  one third to decoded images and two thirds to working images. Project/history,
  driver allocations, pipe/output buffers and other independently bounded storage
  are outside that RAM allowance; this is not an operating-system process cap.
- The detected presentation GPU is shown. The cache-folder field controls
  disposable media source copies, decoded audio, media probes/thumbnails and logs.
  Empty uses `$XDG_CACHE_HOME/fold`, or `$HOME/.cache/fold`. Startup checks write
  access; an unavailable folder produces a visible error and falls back to defaults.
  Files retain their existing lease-based cleanup. Helper sockets and atomic export
  staging remain in their appropriate runtime/output locations.
- Viewer frames remain GPU-resident. Settings reports their cache use and can clear
  unused frames, cancelling preparation/cached replay while preserving displayed
  and in-flight textures. There is no persistent disk viewer cache or disk-cache
  quota control in this slice.
- Apply saves application preferences; resource/path changes explicitly require a
  restart. Appearance remains live. Preferences and presets extend the existing
  versioned `fold/ui.json`, preserving unknown fields. Malformed/newer preferences
  are not overwritten, and worker errors surface in Settings.

Validation:

- Initial affected UI library checkpoint: 81 passed, three opt-in tests ignored.
  Subsequent focused checks cover 19 shell/preset tests, six Settings/store tests,
  and two project-menu guard tests. Preset tests cover save/load/rename/delete/reset,
  preservation of current targets, and removal of cross-project references.
- Two application lifecycle tests pass: save/edit races, undo-to-clean, failed saves,
  failed opens, new/close reset, and rejection of edits made during open.
- Portable-preset and machine-preference tests pass in the platform crate. The
  machine test exercises actual CPU budget exhaustion/release and cache-directory
  setup. The existing media lease-accounting test passes.
- Explicit real-backend rendering passes on Vulkan llvmpipe, including normal/narrow
  Settings, File/Edit/Workspace menus and the unsaved-project dialog. A capture-harness
  viewport mismatch was corrected before reviewing the final narrow captures.
  The Settings fixture uses the reference GPU name; these captures do not claim a
  live GTX 1070 interaction session.
- The explicit real-texture GPU lifetime test passes with added assertions that
  cache clearing retains held/submitted images and releases eligible textures.
- Release desktop build and final formatting/diff checks are recorded in dev-plan.
  Native file-chooser interaction and physical window-close click-through remain
  unverified. Workspace-wide checks and Clippy remain deferred to a slice checkpoint.

Reviewed captures: [Application settings](media/phase-41-settings-application.png),
[narrow settings](media/phase-41-settings-application-narrow.png),
[narrow File menu](media/phase-41-menu-file-narrow.png),
[Workspace menu](media/phase-41-menu-workspace.png),
[unsaved-project dialog](media/phase-41-menu-unsaved.png).

Deferred follow-ups: surface-specific export design, persistent disk viewer caching,
autosave/recovery snapshots and action-specific history labels. The existing history
stores mutations rather than artist-facing command names; this slice preserves
honest generic Undo/Redo labels.
