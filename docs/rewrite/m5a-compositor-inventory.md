# M5a compositor replacement inventory

Recorded from the current operator catalog before document-model replacement. No operator is deferred or retired by this inventory. The shared `fold-graph::types::Type` is used for socket contracts. The 9 October continuation replaces animation with shared typed parameter/track/key records and image inputs with shared edges in schema 7. Static operation/parameter persistence and scoped source/authoring remain unreplaced; see [continuation evidence](evidence/m5a-compositor-native-records-result.md).

| Operation | Version | Fixed inputs | Output | M5a disposition |
| --- | --- | --- | --- | --- |
| Read | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Solid | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Transform | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Crop | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Blur | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Grade | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Mask | 1 | None / dynamic | Mask | Replace authoring contract; retain and requalify processing |
| ApplyMask | 1 | Image, Mask | Image | Replace authoring contract; retain and requalify processing |
| Merge | 1 | Image, Image | Image | Replace authoring contract; retain and requalify processing |
| Output | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Shuffle | 1 | Image, Image | Image | Replace authoring contract; retain and requalify processing |
| RemoveChannels | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Grade | 2 | Image | Image | Replace authoring contract; retain and requalify processing |
| Merge | 2 | Image, Image | Image | Replace authoring contract; retain and requalify processing |
| Transform | 2 | Image | Image | Replace authoring contract; retain and requalify processing |
| Blur | 2 | Image | Image | Replace authoring contract; retain and requalify processing |
| Exposure | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Invert | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Clamp | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Premult | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Unpremult | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Rectangle | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Ellipse | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Input | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Effect | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |
| Dissolve | 1 | Image, Image | Image | Replace authoring contract; retain and requalify processing |
| Relative Transform | 1 | Image | Image | Replace authoring contract; retain and requalify processing |
| Shader | 1 | None / dynamic | Image | Replace authoring contract; retain and requalify processing |

The catalog contains 28 operation/version entries. Shader and Effect inputs are dynamic, supplied by their declared image interfaces.

## Replacement obligations

- Schema 1–6 readers are removed; unsupported documents remain opaque. Schema 7 uses native animation and image connection records, but `Composite`/`Node` static operation persistence still requires replacement. Do not mistake this partial storage cutover for full acceptance.
- Replace the `Parameters`-tag authoring protocol with native versioned operation records, stable parameter and connection identities, typed bindings, and scoped source/structured edits. Inventory every `Edit` variant in `automation.rs`, including shader-interface migration, publish/effect contracts, replacement, duplication and graph layout. No protocol alias or adapter to the retired authoring model ships at phase completion.
- Preserve named channels, EXR/source color metadata, explicit premultiplication, masks, effect mix/mask controls, fractional source times, half-open ranges, declared document/asset dependencies, destination-format-aware transforms, promoted controls and instance overrides.
- Preserve registry/UI creation, graph edits, inspector controls, provider animation, reusable effects, shader publication, nested reads and Composition processing/binding transactions. Unknown native operations must remain opaque and non-executable, with useful diagnostics.
- Use the existing image-processing implementations in `compile.rs`, `compile_image.rs`, `channels.rs`, `transform.rs`, `shapes.rs` and the shared renderer where independently valid. Do not route new documents through a second authoritative legacy graph.

## Existing regression fixtures to carry across the cutover

- `crates/compositor/tests/{animation,reusable,agent_graph,graph_authoring,channels}.rs` cover current authoring/evaluation behavior; passing them today does not qualify the new document model.
- `crates/app/tests/common_composition.rs` covers mixed layers, explicit inputs, processing, animation, timing, nesting and undo.
- `crates/app/tests/motion_video.rs` covers Motion publication and compositor reads through exact named outputs.
- Add new shared-source round-trip, stable typed parameter/connection identity, unknown-version preservation, protection/conflict and artist-continuation fixtures as the replacement is implemented.
