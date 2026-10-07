# Timeline Playground

Open `timeline-playground.fold`, then open **START HERE - Timeline Playground** in Project. Use **F** over the timeline to fit it. Save As a working copy if you want to keep the original fixture.

The 12-second, 24 fps sequence contains:

- **Footage:** three adjacent linked video/audio cuts at 0–4, 4–7 and 7–10 seconds, with one second of unused source at each outer end.
- **Nested titles:** a shared Composition at 1–3 seconds and a compositor node graph at 3–6 seconds. Both reference editable Motion text.
- **Linked tone:** quiet source audio linked to each footage cut.
- **Edit pulses:** separate pulse audio at 8–12 seconds, with a visible waveform.

## Try these

1. Drag footage above the highest video lane, then audio below the lowest audio lane. Check the new-track preview, linked sync, Escape cancellation and Undo.
2. Multi-select cuts, move/nudge them, split, lift and undo. Select a placement or trim mode from the toolbar overflow to try Insert, Overwrite, Ripple Trim and Roll. The adjacent footage cuts have source handles for roll testing.
3. Double-click **Composition - Opening Title**, then return and double-click **Graph - Title Output**. Verify their different clip icons and mapped source times. Edit the text/size in **Motion - Editable Title** to see source propagation.
4. Play and scrub the sequence and nested sources. Watch the thin cache ranges and viewer indicator; change an input and verify stale ranges disappear.
5. Drag either media asset from Project back into the timeline. Resize track heights/header width; check thumbnails, waveforms, mute, solo and lock.

Undo between experiments. Ripple Delete rejects a range if unselected material spans it: either select all intersecting clips or test the footage-only 6–7 second region after splitting at 6 seconds.

## Files and verification

Keep `timeline-playground-media/` with the project; the media is linked, not embedded. It contains generated BT.709 color-motion footage with a quiet tone and a separate pulse WAV. The test-pattern counter displays **source** time, intentionally one second ahead of sequence time after trimming.

Created through Fold's public agent transactions and saved at revision 12. Sequence structure and linked IDs were inspected; frames at 0, 2, 4 and 8 seconds rendered successfully. Frames at 2 and 4 seconds were visually reviewed with both nested title types visible. The user accepted the demo on 2026-10-07; detailed dragging, navigation and cache test results were not separately reported. The main sequence is 1280×720; test media and title graphs are 640×360.
