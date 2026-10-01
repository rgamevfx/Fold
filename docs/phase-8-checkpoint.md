# Editing slice checkpoint — sequence, audio, nesting, and undo

## Goal

Verify the editing slice before starting motion: a short editable sequence with
sequence audio and a nested compositor, with atomic cross-document undo. Follow
product architecture sections 4, 9, 10, 17, and 18.

## Tasks

1. Run the existing actual-media, nested-workflow, and session integration tests;
   retain a fresh inspection project outside the repository.
2. Run workspace tests/checks and package-boundary checks at this slice checkpoint.
3. Open the retained sequence in the native desktop. Verify nested-source
   navigation and return to synchronized root playback where desktop input permits.
4. Record evidence separately from unperformed native checks. Request user
   acceptance if physical input/audio cannot be verified; do not mark complete early.

## Acceptance criteria

- Creation replaces video with a live document reference and preserves sequence
  audio samples, timing, and gain, without flattening.
- Nested edits affect sequence rendering; exact local-time navigation and return
  to root playback work.
- Global undo restores a graph edit, then atomically restores the original linked
  A/V clips and removes the created composition; redo restores the batch.
- Save/reopen preserves nesting; actual MP4 export includes audio.
- Workspace checks and tests pass, or unrelated failures are explicitly recorded.
- Native short-sequence playback/audio and cross-document undo are verified or
  explicitly accepted by the user with remaining limits documented.

## Status

**Complete by user acceptance.** After reviewing the checkpoint, the user reported
that everything looks good and explicitly requested completion. Automated
verification passes and native playback was observed. This acceptance closes the
editing-slice gate and permits phase 9; it does not assert that every manual recipe
step below was individually performed or establish new measured audio/performance
results.

## Verification

- `cargo test --workspace --all-features`: passed; opt-in physical audio-device
  test remains ignored. This run includes the corrected navigation fixture.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- Package boundaries/headless isolation and all nine boundary-script tests passed.
- Workspace formatting and diff whitespace checks passed.
- After strengthening the actual-media undo/redo assertions, all 13 tests across
  `compositor_media`, `compositor_session`, and `compositor_workflow` passed again.
  Document and asset maps match before creation, after creation, and after graph
  editing through undo/redo. Global revisions intentionally advance during undo.

### Finding and correction

The exact-navigation fixture was flaky: with two unrelated composites, random
UUID ordering sometimes selected the intended source as the implicit project
output. Navigation to that existing ancestor correctly truncated breadcrumbs,
so Back could not restore the expected parent. Four isolated runs reproduced one
failure. Establishing the parent while it is the sole output fixes the test setup
without changing production navigation semantics. The final focused run passes.

### Native evidence

Fresh artifacts are retained outside the repository at
`/home/rgame/Documents/Fold-editing-checkpoint/`: `phase-8.fold`, the three-second
320×180/24-fps H.264 source with 48-kHz sine audio, and `phase-8-export.mp4`.
The export test verifies 48 video frames and an audio stream.

Opened the existing phase-8 desktop binary (no application logic changed) with:

```sh
./target/debug/fold-desktop --project /home/rgame/Documents/Fold-editing-checkpoint/phase-8.fold
```

Screenshots in `/home/rgame/.local/state/agent-desktop/screenshots/fold-editing-checkpoint/`:

- `Screenshot_2026-10-01_07-07-08.png`: reopened sequence has a document-reference
  video clip and separate preserved source-audio clip; graded/blurred image visible.
- `Screenshot_2026-10-01_07-07-33.png`: Play activated through the tested libei route;
  playhead advanced to frame 15, viewer displayed frame 14, transport reported
  **Audio clock** and **underruns 0**. This is a single short-playback observation,
  not a measured synchronization tolerance, long-duration test, or listening test.

## Native follow-up recipe

1. In the open project, seek back and Play; confirm the sine tone is audible and
   the nested image advances. Confirm the short sequence stops normally.
2. Select the composite video clip; Open Source from its inspector. Change Grade,
   return to Timeline, and confirm the updated nested result and parent playhead.
   Undo/Redo the graph edit from the timeline workspace.
3. To test composition creation's global undo, use a fresh project, import the
   retained `Compositor source.mp4`, and select its linked A/V clips. Create
   Composition, verify audio remains, edit its graph, then Undo twice: first the
   graph edit, then creation (original linked A/V restored). Redo twice.
4. Confirm save/reopen and export remain usable. Note that undo history is
   session-local: the reopened inspection project cannot undo the fixture's
   pre-save creation operation.

The agent's checkpoint run did not independently verify native navigation,
graph-edit undo, composition-creation undo, or audible quality. User acceptance is
separate evidence, not a claim that the agent executed those checks. Long-duration
drift and performance measurements remain deferred.
