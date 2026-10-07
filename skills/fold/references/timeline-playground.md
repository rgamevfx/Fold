# Timeline demo authoring

Use for editable timeline fixtures with linked media and nested Composition/graph clips. The verified artifact is `demos/timeline-playground.fold`; its artist guide describes the actual layout.

1. Discover a session and guard each batch with its current revision/epoch. Build Motion → Composite → Sequence using `scripts/demo.py` only in a project where those additions are wanted.
2. Import synthetic video with explicit BT.709 primaries, transfer and matrix metadata. An H.264 file with unspecified `color_space` is rejected. `place_asset` creates linked video/audio automatically when both streams exist; inspect returned tracks and clips before arranging them.
3. Create a shared Composition from existing timeline clips through this verified public command:

```json
{"op":"command","id":"fold.app.create-composition","arguments":{"document":"SEQUENCE_UUID","clips":["CLIP_UUID"]}}
```

Inspect state afterward to discover the new `fold.composition.document`; rename it through `rename`. The selected timeline clip retains its identity and now refers to the shared Composition. A remaining clip can still reference the original compositor graph, covering both navigation types. `catalog type=fold.composition.document` currently reports an unknown authoring type; the command above is supported even though its argument schema is not advertised.

4. Use `Clip.Trim` with `linked:true` to retain source handles, then `Clip.Move` to position the trimmed segment. Use `Selection.Split` with linked selection for adjacent cuts; inspect again because it allocates new clip/link IDs. `MoveTo` can allocate a lane when a destination is occupied, so move in a collision-free order and inspect the final lane layout. Rename tracks through `RenameTrack`.
5. Render representative media-only and nested-source times, inspect linked pairs, save through `save`, and refresh state. Keep linked assets beside the demo and disclose manual interaction checks separately from rendered evidence.

Current public gaps: common Composition typed authoring/inspection and command argument schemas; `place_asset` has no explicit destination-track/new-track or insert/overwrite arguments. Discover current catalogs before assuming these limitations remain.
