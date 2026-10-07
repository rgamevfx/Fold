# Phase 52 — Timeline editing and cache visibility

Completed on 2026-10-07 following user review of the Timeline Playground: “it looks good,” with an explicit request to mark complete and commit. Detailed gesture-by-gesture results were not separately reported. Fold keeps its track-based editing model and existing theme. This phase adds direct editing capabilities without adding another toolbar row.

## Delivered

- One compact toolbar: Selection, Blade, Snapping, Linked selection, Fit and an options menu. Secondary edits, placement/trim modes, track presets, preview visibility and navigation controls live in menus. Context menus provide clip edits and track rename/add/remove.
- Video lanes stack upward; audio lanes stack downward. Headers use visibility/lock icons and audio M/S controls. The header divider and individual track heights are draggable. Track height presets remain available.
- Drag a clip above the top video lane or below the bottom audio lane to preview a new track. Track creation and movement commit together, with one undo. Escape, invalid ranges, incompatible destinations, project revision changes and locked tracks prevent publication.
- Project drops preview the same placement proposal used on commit. Placement creates missing compatible lanes and keeps source A/V linked; dropping on an audio lane deliberately places audio only. The preview includes any required companion track. Track creation stays inside the placement transaction.
- Shift-click adds to selection; Ctrl-click toggles membership. Drag empty timeline space for marquee selection. Moving a selection preserves relative timing and track offsets within the dragged media type. Linked companions retain timing. Alt+Left/Right nudges one frame; Alt+Up/Down moves between existing compatible tracks without changing time.
- Split, Lift, Ripple Delete, Insert, Overwrite, Ripple Trim and Roll use shared model-level validation. Both clip moves and Project placement support Insert/Overwrite. The default Move/Place behavior rejects collisions. Roll requires an adjacent clip and sufficient source handles. Ripple edits respect all affected track locks.
- Ghost destinations, retained source outlines, snap guides, trim time/duration readouts, invalid-drop messages, horizontal/vertical drag auto-scroll and Escape cancellation. Feedback overlays the canvas instead of changing its height.
- Frame-aware ruler with non-drop timecode, minor ticks, a visible playhead and fit/zoom/pan navigation. Exact rational time remains authoritative.
- Bounded asynchronous source posters and trimmed audio waveform overviews reuse the Project thumbnail worker through the shared UI SDK. No probing/decoding runs on the UI thread. Errors are visible on clips, with details on hover.
- Clip names use Project labels. Media, compositions and node graphs have small drawn type icons; links have a subtle indicator. Double-click/Open Source navigates a nested clip at the mapped playhead time, falling back to its source in-point when the playhead is outside the clip. Existing breadcrumb Back returns to the sequence.
- Two-pixel cache strips in the NLE ruler, shared Composition timing surface and viewer scrub strip. A small viewer status dot reports the current frame. Green means retained at the relevant viewer settings; amber means another resolution/crop. Content, document/output, display policy and channel identity must match. Evicted entries disappear from subsequent cache snapshots. The indicators describe presentation-cache residency, not render progress or export readiness.

## Editing rules and current limits

- Ripple Delete closes selected time ranges across the sequence, but rejects a range containing unselected material rather than silently cutting that material. Select the affected material or use Lift.
- Normal in/out trims, ripple in/out trims and roll boundaries retain source handles. Ripple in-trim changes the source in-point while anchoring the sequence cut and shifting following material by the duration change.
- Source posters are single-frame, ungraded media identification previews. Waveforms are bounded 128-column overviews of the trimmed source range, not sample-resolution editing displays. Nested documents use names/type icons rather than rendering new thumbnail jobs.
- Drag-to-create uses the outer video/audio boundaries; inserting a track between existing lanes is not part of this gesture.
- Cache coverage follows the linked viewer when unambiguous. Other available resolutions/crops are partial coverage; different source identities or channel selections never count as ready. A graph's evaluated output is represented by its viewer's strip/status, not per-node cache badges.

## Verification

- `cargo test -p fold-timeline --features ui --lib --tests -- --include-ignored --test-threads=1`: 19 passed, including native capture.
- `cargo test -p fold-ui --lib -- --test-threads=1`: 96 passed, three existing native/GPU tests ignored. Covers shared transport typing/seeking, scoped navigation, composition timing, cache policy and editor layouts.
- `cargo test -p fold-media --test thumbnails`: three passed, including trimmed silence versus tone waveform verification.
- `cargo test -p fold-app --features desktop --test project_placement`: five passed, including atomic track creation, undo/redo and insert/overwrite placement.
- Native offscreen timeline captures passed at 1024px and 320px on llvmpipe: `/tmp/fold-timeline-ui/normal.png` and `/tmp/fold-timeline-ui/narrow.png`. These exercise actual ImGui rendering, not OS mouse automation. Visual review includes enlarged header inspection; the capture test checks timecode pixels as well as panel bounds.
- Desktop check and release desktop build passed. Final diff whitespace review passed.
- Full workspace tests and Clippy remain deferred. No live artist project was modified or restarted for verification.

## Artist acceptance checklist

1. Open a sequence in the rebuilt desktop. Check normal/narrow widths, long clip names, track rename, header resizing and individual track heights.
2. Drag existing video above the highest video track and audio below the last audio track. Repeat with Project media and a composition. Check the proposed destination, A/V sync, Escape, and single-step undo/redo.
3. Marquee/Shift/Ctrl-select several clips; move, nudge and change tracks. Exercise snapping, auto-scroll, locked destinations and invalid overlaps.
4. Try Blade, split-at-playhead, Lift, Ripple Delete, Insert/Overwrite and both trim modes. Confirm affected clips match the preview and source handles are respected.
5. Double-click a composition and a node graph with the playhead inside the clip; verify mapped source time and Back navigation.
6. Play or prepare a review cache, then seek, edit, switch channels/resolution/crop, and clear unused cache. Check timeline/Composition/viewer strips and the status dot. Confirm stale content is not marked ready.
7. Check real media posters and waveforms after trimming, with playback and UI interaction remaining responsive.
