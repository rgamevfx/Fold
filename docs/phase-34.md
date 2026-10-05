# Phase 34 — Motion authoring and Chroma Parade v2

User-authorized overhaul: retain Motion → Composite → Sequence. Ship powerful editable node assets, inspired by Houdini's encapsulated tools, with a small artist-facing control surface. Primitive nodes remain available inside constructions. Scene organization must not expose every object's implementation in one default network.

## Interaction contract

- Select a construction to edit its published controls; enter its graph deliberately. Network defaults to the selected object's construction, with explicit full-document access for legacy/advanced work.
- Source and path references are named, replaceable, and navigable from the Inspector. Generated copies belong to their generator; source editing is explicit.
- The Motion viewer owns an intent-based left shelf and contextual top controls. Other document viewers retain their existing presentation. Creation and editing use the transaction preview/commit/cancel lifecycle.
- Distribution and travel are independent. Changing speed must not change spacing; increasing count redistributes copies evenly. Color transfer is optional and its target explicit.
- Shipped assets use the same group interface and graph editing as custom assets. No duplicate hidden evaluator or flattened source content.
- Keep object identity visible while editing content, appearance, transforms and ordered operations. Preserve exact-time animation and existing documents.

## Implementation checkpoints

- [x] Along Path asset: source/path references → sampled path points → Copy to Points → optional color transfer; independent travel and secondary motion controls.
- [x] Useful Badge creation, explicit source editing, and compact Inspector reference/operation controls.
- [x] Scoped Network, meaningful group/node labels, parent navigation, and explicit document network access.
- [x] Motion viewer tool shelf, contextual controls, viewport selection/manipulation and cancellation.
- [x] Separate Chroma Parade v2 generator/project, keeping the original intact.
- [x] Focused semantic/UI tests and rendered normal/narrow review; record native interaction limits.

## Acceptance

Create a badge, create a path, create Along Path from those references, adjust count and travel, enable text color from the path, edit the source, inspect/customize the group, and review the unchanged Composite/Sequence integration. Verify count, speed, source text, target path, bypass, persistence, arbitrary-time evaluation and undo. Track unfinished broader tools explicitly; do not equate a successful image render with native UX acceptance.

## Delivered workflow

1. In Motion, use **Tools → Create → Badge / label**, or draw Text and Shapes with the viewer shelf. Badge is a scene group containing editable text and an automatically fitted rounded background.
2. Draw a curve with **Bézier Pen**: click for corners, drag for tangents, click the first point to close, then Finish/Enter. Escape cancels the entire construction. Points exposes anchors and tangent handles; Alt breaks tangent coupling.
3. Select the badge and choose **Tools → Arrange → Along path → your path**. This creates an ordinary editable group asset and named object-reference nodes; the source is hidden in the scene and remains editable.
4. Change Copies, Align to path, Position and Seconds per loop. Enable Travel independently of copy spacing. Static open paths include both endpoints; traveling copies wrap from end to start. Secondary motion is collapsed by default.
5. Enable Path colors, choose Text only or whole content, and edit the referenced path's shared gradient. Source/Path controls support a named picker, viewport Pick and Edit. Return returns to the generator after source editing.
6. Enter **Edit Graph** for Path Points → Copy to Points → color transfer. The default outer Network shows the selected construction and its references; Document network and parameter sockets remain explicit options.

The shelf also exposes Select, Move, Rotate, Scale, rectangle/ellipse creation, text creation/editing, grid snapping and point editing. Right-click the transform/shape families or use the contextual tool selector. Each completed drag is one transaction; Escape restores the original. Playback retains disabled chrome. Space-drag pans the shared viewer.

Tools are catalogued by Create, Arrange, Animate, Style and Connect. Oscillate and style operations are editable node groups. Modifier enabled state, order, removal, graph access and Make unique operate on the same construction. Convenience UI does not contain a separate motion evaluator.

## Code and project

- `crates/motion/src/authoring/assets.rs`: shipped constructions and actual source references.
- `crates/motion/src/nodes/path_points.rs`: distribution/travel; existing Copy to Points retains attributes.
- `crates/motion/src/ui/tools.rs`, `tool_shelf.rs`, `overlay.rs`: gesture state, chrome and direct editing.
- `crates/motion/src/ui/picking.rs`: bounded worker over immutable snapshots, with revision/time/scope validation. Text shaping and geometry evaluation stay off the UI thread. Picking has nesting/point budgets and handles stroke interiors and compound fill holes.
- `crates/app/examples/chroma_parade_v2.rs`: reproducible v2 fixture; `path_parade` retains its original construction and shares only project/composite/sequence assembly.

Generate a new project (existing paths are deliberately not overwritten):

```sh
cargo run -p fold-app --example chroma_parade_v2 -- /tmp/fold-chroma-parade-v2-final.fold
```

Generated project for this session: `/tmp/fold-chroma-parade-v2-final.fold`. The original `/tmp/fold-chroma-parade-final.fold` is unchanged. The generator validates rendering at nonsequential times through the shared output renderer.

## Verification and limits

- Motion: 50 tests pass, covering asset evaluation, arbitrary time, spacing/speed independence, color propagation, source-local placement, auto-fit text background, persistence, cycle rejection, scoped graphs, shape/pen undo/cancel and Bézier tangent coupling. The separate native-render test passes.
- App Motion integration: 3 tests pass. Shared UI: 75 tests pass (3 explicit native tests ignored by default). Desktop-feature check and release desktop build pass.
- Real Dear ImGui/WGPU captures at normal and narrow sizes, including the scoped Network, were reviewed: `/tmp/fold-motion-v2-ui/{normal,narrow,network}.ppm`. These render actual controls and evaluated selection outlines; they do not show the full desktop shell or its artwork texture.
- Release GPU-path profile at 960×540 over eight frames: warm mean 50.433 ms on **Vulkan llvmpipe software rendering**; graph compilation ~0.7–1.2 ms/frame, three render passes, zero CPU adapter nodes. This is neither hardware GPU performance nor a UI latency benchmark.
- CPU/GPU-path parity: three v2 frames have zero output code-value error. The saved final project was also rendered through `fold-cli` at 1.25 seconds and visually reviewed (`/tmp/fold-chroma-parade-v2-final.png`).
- Live OS-level interaction could not be exercised: Orca computer runtime returned `runtime_unavailable`, and opening it timed out. Native desktop acceptance and reference-hardware playback remain unchecked. Simulated ImGui inputs cover the implemented creation/undo gestures.
- Viewport transforms edit authored base values; richer driven/keyframed direct-manipulation policy remains future work. Picking is bounded flattened geometry, not final rendered-pixel/mask hit testing. Source editing uses scene scope and outlines rather than a separate isolated rendered stage.
- This phase implements the requested text/shape/Bézier shelf and the badge/path workflow. Additional asset families, per-copy overrides and advanced alignment/pivot tools are future extensions, not claimed here.
- Workspace-wide checks and Clippy remain deferred; affected crate checks are the gate for this iteration.

Remaining acceptance:

- [ ] Exercise the final release desktop on reference hardware, including text focus, source return, constrained objects, playback transitions and multi-viewer edits.
- [ ] Measure hardware playback and interactive latency with a larger production scene.

## Follow-up — stable construction membership

A user reported nodes disappearing after temporarily rewiring an output. A regression through `GraphContext::event` reproduced it: node IDs remained in the serialized document, while output-reachability filtering removed the disconnected branch from the view. No automatic deletion occurred in this reproduction.

Construction scope now includes authored source/appearance/transform/operations and retained node membership. A commit retains detached branches as scene-object presentation metadata in the same undo transaction; newly added disconnected nodes are also assigned to the active construction. Membership survives document reload and switching constructions. Explicit deletion removes nodes normally, and undo restores them. Nested groups still show their complete graphs. The transient `network_extras` workaround was removed.

Regression coverage exercises rewiring a custom branch, document reload, explicit deletion/undo, new disconnected nodes, and separation from unrelated source constructions. Existing hidden branches from older edits can be found with **Network → Full document network**; ownership of previously orphaned custom nodes is not guessed.
