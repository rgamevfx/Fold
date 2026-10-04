# Phase 22 — Animation authoring

User-requested animation slice, independent of the remaining delivery milestones.

## Goal and boundaries

Add a compact Animation panel with Dope Sheet and Curve Editor views, backed by exact-time scalar channels. Keep feature models in their packages, share curve semantics and UI components, and route all edits through existing preview/commit/cancel transactions. No new roto geometry model is implied: the existing compositor Mask is rectangular.

## Tasks

- Shared animation contracts: exact times, stable key identities, independent channels, Hold/Linear/Bézier interpolation, automatic/linked/broken tangents, validation and deterministic sampling.
- Compositor parameter animation, schema migration, snapshot compilation and persistence.
- Shared compact parameter keying: label versus component targeting, context menus, Alt-click, animation indicators and explicit Auto Key.
- Animation surface using the linked editor/viewer context; compact animated-only hierarchy, filtering, selection, retiming, copy/paste, snapping and curve editing.
- Reuse shared time navigation; keep feature adapters and transactions separate.
- Focused model, persistence, rendering, transaction and UI checks; review normal/narrow layouts.

## Acceptance and verification

Key a Transform component, scrub and render its evaluated value, edit its keys in both views, undo/redo, save/reopen, and preserve unrelated extension data. Verify component independence, exact/subframe times, insertion at an existing key, interpolation, collision rejection, cancellation, and viewer targeting. Toolbar chrome is at most two rows including the link selector. Record any native verification limits honestly.

## Results

Implemented the Animation surface as a capability of each editor instance. The shell dispatches it through the same explicit link group and selected viewer as the inspector. Multiple eligible viewers require an explicit viewer selection; neither surface silently inserts keys at an editor fallback time.

### Ownership and compatibility

- `fold-animation` contains exact-time scalar channels, stable key IDs, interpolation, weighted Bézier handles and automatic/linked/broken tangents. It depends only on foundation and serde. Feature packages retain their own property bindings, document payloads, validation and transactions.
- The UI SDK owns the Animation canvas, key menus, property keying and shared time-navigation geometry. The NLE reuses the extracted geometry without acquiring an animation-model dependency or changing clip semantics.
- Compositor schema 3 reads schemas 1–2. Transform, Solid, rectangular Mask/Crop, Blur and Grade expose animatable components. Snapshot compilation samples their curves before generating the existing render operations, so preview/export/headless share evaluation. Unanimated parameters retain their original values exactly.
- Motion schema 2 reads schema 1. Numeric literal inputs have namespaced component channels; connected graph drivers cannot be silently replaced. Existing Keyframes nodes appear in Animation, retain their existing Hold/Linear/Ease evaluation until edited, and preserve Ease through equivalent Bézier handles. Pre-roll key times remain supported. Removing a driver's final animated components converts it to a constant Value node with the same node/output identity.
- Inspector and Animation edits use the existing preview/commit/cancel path. The inspector only finishes gestures that it owns. Undo/rebind generations invalidate stale Animation gestures. Removing the last property key holds its evaluated value.
- Unknown node/document extension data survives round trips. Animation-only data never enters project/render infrastructure. Provider build identities change with the new evaluation semantics.

### Artist workflow

Open **Panels → Animation**, or select its dock tab. The panel has one link-group row and one icon-toolbar row. At narrow widths secondary actions remain in the tools menu. The Dope Sheet shows animated nodes, properties and component rows; scalar properties avoid a redundant Value row. Nodes/properties collapse into summary keys. Search and selected-node filtering are in the menu.

Right-click a parameter label to add/update/remove keys for the whole property, or right-click a component value for only that channel. Alt-click provides the same insertion fast path. Existing keys update in place. Auto Key is explicit in Animation's tools menu; between-key values require insertion or Auto Key before editing. Color properties retain the shared color picker in their parameter menu. Four-component fields wrap within the inspector rather than shrinking into unreadable cells.

Animation interactions:

- Drag/box-select keys; Ctrl-click extends selection. Summary diamonds select their coincident channel keys. Delete removes selected keys; Ctrl+A selects matching channels' keys. Ctrl+C/Ctrl+V copy and paste within the same document/channel bindings, anchored to the playhead, with collisions rejected atomically.
- Switch Dope Sheet / Curve Editor without discarding selection or horizontal navigation. Choose visible curves in the channel list; Ctrl-click adds channels. Up/Down chooses a channel; Enter selects its keys. F fits the view, I inserts at the playhead, and Shift+F10 opens key tools.
- Right-click keys for Hold/Linear/Bézier interpolation, automatic/linked/broken tangents, exact rational key time, numeric value and numeric tangent handles. Drag Bézier handles directly. Escape cancels the gesture; release commits one edit.
- Ctrl-wheel zooms time around the pointer; Shift-wheel pans time; ordinary wheel scrolls rows; middle-drag pans. Alt-wheel zooms values in Curve Editor. Frame snapping is explicit; disabling it permits subframe drag placement without quantizing existing authored keys.

The current clipboard deliberately targets the original document/channel bindings; it rejects unavailable targets and existing key-time collisions. This does not add a spline-roto model, motion settings/geometry interpolation, spatial path editing, quaternion interpolation, or a new expression/property-stack engine. Existing procedural graph connections retain their meaning.

### Verification

Focused verification completed:

- Animation model: 5 tests passed.
- Compositor: 10 tests passed, including evaluated rendering, persistence, migration, extension preservation and undo.
- Motion: 22 tests passed, including legacy driver conversion, component independence and unknown extension preservation.
- Timeline: 12 tests passed after extracting shared time navigation.
- UI: 52 tests passed; 2 existing GPU tests remain ignored. Coverage includes narrow layouts, input gestures, clipboard collisions, linked viewer contexts and fractional-frame-rate subframe placement.
- Application compositor/motion/panel workflows: 13 tests passed.
- Desktop check and final desktop build passed offline. Dependency boundaries and 9 boundary-policy tests passed.
- Affected-library Clippy completed successfully with existing warnings in media/compositor/motion; no new animation/UI warnings. Full workspace checks and GPU stress testing remain deferred.

Native review uses the small project written by:

```sh
cargo run -p fold-compositor --example animation -- /tmp/fold-animation-review.fold
cargo run -p fold-app --features desktop --bin fold-desktop -- --compositor /tmp/fold-animation-review.fold
```

The example refuses to overwrite an existing project. Native review confirmed the Dope Sheet, Curve Editor, channel selection, compact inspector and right-click whole-property insertion on the GTX 1070. Screenshots were captured at 1280×684 and 760×684. The initial narrow review led to improved label width and toolbar overflow; both refinements were confirmed in the final native build. The native views are not substituted mockups. Alt-click, key dragging/cancellation, layout constraints and independent viewer targeting are also exercised through real ImGui input tests. Orca runtime startup was unavailable, so native inspection used the X11 review window directly.

