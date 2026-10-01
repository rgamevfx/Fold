# Graphite reference for motion development

Status: initial source inspection, not a dependency decision or completed algorithm audit.
Graphite is the primary Rust implementation reference for phase-9 vector, text,
transform, repetition, and node-definition work. Fold's product semantics remain authoritative.

## Pinned source

Repository: https://github.com/GraphiteEditor/Graphite

Inspected revision: `39158dec4b44df854d9f8befd657bf10a31f708d`.
The references below are pinned so later upstream changes do not change the evidence.

## Findings

- Graphite separates node implementations by responsibility (vector, repeat,
  text, transform, math, etc.) from supporting type/algorithm libraries and from
  graph compilation/execution. See the [source tree](https://github.com/GraphiteEditor/Graphite/tree/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph).
  Adopt this responsibility split in Rust modules first; do not reproduce its
  full crate structure before Fold needs independent compilation/ownership.
- Shape node functions carry category, units, defaults, and range annotations,
  and delegate curve construction to geometry algorithms. See
  [generator_nodes.rs](https://github.com/GraphiteEditor/Graphite/blob/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph/nodes/vector/src/generator_nodes.rs).
  Fold should colocate node metadata and implementation, while keeping reusable
  geometry algorithms independent of the graph and UI.
- Registry metadata includes descriptions, field types/defaults, units, hard/soft
  limits, and UI hints. See [registry.rs](https://github.com/GraphiteEditor/Graphite/blob/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph/libraries/core-types/src/registry.rs).
  Fold should use one explicit node definition to drive validation, creation,
  sockets, and ordinary property controls. Start without a custom procedural macro.
- Repeat implementations evaluate content with per-iteration index context and
  manipulate typed lists and transforms. See [repeat_nodes.rs](https://github.com/GraphiteEditor/Graphite/blob/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph/nodes/repeat/src/repeat_nodes.rs).
  This is implementation reference material, not evidence that Graphite's repeat
  semantics equal Fold's proposed distribution-plus-instancing semantics.
- Text-to-path code supports separate glyph items and propagates transforms and
  appearance to generated paths. See [to_path.rs](https://github.com/GraphiteEditor/Graphite/blob/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph/nodes/text/src/to_path.rs).
  Review the underlying shaping and fallback implementation before choosing Fold's
  libraries. This inspection does not establish cluster identity, reproducible
  font resolution, or animation guarantees for Fold.
- Graphite also has a substantial executor registration table. See
  [node_registry.rs](https://github.com/GraphiteEditor/Graphite/blob/39158dec4b44df854d9f8befd657bf10a31f708d/node-graph/interpreted-executor/src/node_registry.rs).
  Do not copy that scale of dynamic execution machinery as Fold's initial design.

## Application to Fold

Proposed, pending phase-plan agreement:

- Keep `fold-motion` the feature owner, with distinct document, graph, node,
  geometry, field, animation, evaluation, authoring, and UI modules.
- Give each substantial node a discoverable file, with its definition, typed
  authored settings, validation/lowering, and focused tests together.
- Keep the registry an explicit catalog, not a second implementation of node
  semantics. Adding an ordinary node must not require editing the shell, project
  core, or a central evaluator match over every node kind.
- Keep actual algorithms reusable underneath nodes; keep interaction code out
  of geometry/field evaluation.
- Shared render contracts describe immutable drawing work, never motion graphs.
- Record upstream file/revision and license obligations for any adapted code;
  verify file-specific notices and compatibility before copying. Reference use
  is not a decision to embed Graphite or its executor.

## Follow-up before implementation

Inspect Graphite's curve algorithms, shaping context, font fallback, transforms,
and rendering adapters in depth for the corresponding Fold tasks. Validate units,
coordinate conventions, degeneracies, alpha/color behavior, bounds, and cancellation
against Fold's requirements. Add independent Fold tests for adapted behavior.
