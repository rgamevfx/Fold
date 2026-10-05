# fold-motion

Motion owns the editable procedural construction. It does not own project state,
media workers, the GPU device, or a separate rendering path for the desktop.
Phase 9 is **in progress**; see `../../docs/phase-9.md` for acceptance and limitations.

## Contributor map

- `document.rs`: versioned payload, font environment, reusable groups, validation.
- `graph/`: stable node/socket identities, definitions, signatures and static catalog.
- `nodes/`: categorized node definitions and their interpretation. Small related
  operators share a module; substantial content nodes have individual files.
- `geometry/`: native paths, shaped glyphs, instances, transforms and style algorithms.
- `fields/`: typed values and pure, bounded element-domain field evaluation.
- `animation/`: exact authored key times and interpolation.
- `evaluation/`: demand dispatch and immutable render-plan compilation.
- `authoring/`: direct tools implemented as actual graph edits, group extraction,
  animation and published-control bindings. They never render.
- `package.rs`: static document/video/transaction contributions.
- `ui/`: editor-only state, graph presentation, property controls, curves and handles.
  `ui/graph.rs` implements the shared SDK's `GraphContext` adapter; motion and
  compositor use the same `GraphCanvas`, not separate native canvas implementations.
  Navigation, node search, selection and gesture fixes belong in
  `crates/ui/src/graph_canvas/`. Feature validation and transaction translation
  stay in the adapter. Node properties contribute to the **shared Node Inspector** through
  `PanelRegistry::register_node_inspector`; there is no Motion Inspector window.
- `fold-render::vector`: feature-neutral immutable native drawing contract and CPU
  raster adapter. It knows no motion nodes/documents.

The old `Solid` document/provider remains compatible with foundation projects.

## Adding an ordinary node

1. Implement a definition in the appropriate `nodes/` module (a new file for a
   substantial operation). Give the type and every socket stable identifiers.
2. Keep typed authored settings, their defaults and validation beside that
   definition. Do not serialize Rust type names or use display labels as IDs.
3. Declare socket types, defaults and units once. A specialized signature is
   available for typed math, explicit domains and reusable group interfaces.
4. Implement the node using reusable geometry/field/animation functions. Use
   `Evaluator::field` for domain-dependent values and `uniform` only for inputs
   which deliberately cannot consume element fields. Never silently evaluate an
   element field with a fabricated index zero.
5. Add the definition to its category catalog. The evaluator, project core, app
   shell and ordinary inspector do not need a new match arm.
6. Add focused tests through `Evaluator`, `compile`, or the package transaction
   interface. Cover invalid inputs, type/domain rules and explicit-time behavior.

Fundamental value kinds are a closed typed vocabulary; the node library is not a
single global operation enum. Runtime dynamic-library loading is not supported.

## Semantics and resources

- Time is an explicit rational request. Field math may use floating-point seconds;
  authored key times and interval comparisons stay rational.
- Content is ordered, native 2D geometry. Instances retain shared source references.
  Realization is explicit and bounded, including nested groups.
- Generated IDs and current element order are distinct. Text edits can change
  cluster correspondence; unchanged text keeps shaped glyph identity across time.
- Colors are scene-linear SDR sRGB. The current vector adapter has **8-bit linear
  color/coverage precision** and expands to premultiplied RGBA32F for compositing.
  This is not a claim of high-precision vector rasterization.
- The bundled Noto font is an explicitly versioned resource, with license and
  fingerprint under the workspace’s `resources/fonts/`. No ambient system-font fallback occurs.
- Unknown node settings and extension values survive roundtrips. Unavailable
  implementations fail explicitly if demanded; incomplete wiring remains editable.
- Authoring helpers mutate candidate feature models; committed edits must go through
  the registered command and project coordinator. Preview overlays never export.

## Checks and an inspectable fixture

```sh
cargo test -p fold-motion
cargo test -p fold-motion --features ui --lib
cargo test -p fold-app --test motion_workflow
cargo run -p fold-motion --example title -- /absolute/new-title.fold
cargo run -p fold-app --features desktop --bin fold-desktop -- --motion /absolute/new-title.fold
```

The example uses the same public graph-editing tools as the UI; it does not add a
special title evaluator. Its curves, shaped text, wave, falloff and published
control remain editable. The destination must not already exist.

## Implementation reference

Graphite is the primary Rust reference for vector/text and node implementation
work. Pinned source observations are in `../../docs/graphite-reference.md`.
Its architecture informed the split between nodes and geometry algorithms; Fold
has not embedded Graphite's application or executor. The current text adapter uses
Rustybuzz and Unicode bidi rather than copying Graphite's Parley adapter. Broader
script/layout/fallback coverage still needs explicit tests before support claims.
