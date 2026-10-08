# Timeline demo authoring

> **Pre-rewrite reference — 8 October 2026.** These examples describe the old build. For new motion/MCP/agent work, use the [Motion System Rewrite Plan](../../../docs/Motion_System_Rewrite_Plan.md); this guide supplies no compatibility requirement. Requalify examples against the replacement public contract before publishing them as supported.

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

## Existing MoGraph demo → Composition (Rust Orbit verification)

`demos/fold-rust-orbit.fold` now exercises native layer ownership and keyed group inputs. A public `create` operation with `kind: "fold.composition.document"` is supported even though typed Composition authoring discovery is incomplete. In the same guarded batch, `command` with `id: "fold.composition.edit"` accepts `{document, composition: {info, layers}}`. A layer carries its inspected scene object's shared state, source `{"Output":{"document":"GRAPH_UUID","output":"video","fold.layer.object":"OBJECT_UUID"}}`, source `info`, and rational `origin`/`source_start`. Binding objects through this command also marks their graph as layer-owned, enabling Composition → Network selection routing. Preserve any inspected extension data; do not rebuild existing graphs.

For published group inputs whose values currently come from defaults, first materialize literal instance inputs using `node` or `set`. A `node` patch can retain native scalar curves in `fold.motion.animation` (for example `radius.0` and `speed.0`); obtain the exact key schema from a tool-created example/current catalog. Rust Orbit uses constant-value Bézier keys at 0 and 4 seconds to expose editable channels without changing the existing output. Verify both the native UI and the renders: a successful graph edit alone does not prove Composition discovery or Curve Editor interaction.

The saved Orbit regression verifies public save, reopen, Composition insertion and curve dragging, rendered response, and undo/redo. Typed Composition inspection/edit schemas and a high-level scene adoption operation remain public API gaps; track them in dev-plan rather than editing project archives.
