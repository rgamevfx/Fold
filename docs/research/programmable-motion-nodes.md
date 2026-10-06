# Programmable Motion nodes

Decision update: phase 45 implements the user-selected **single inline Rust Wrangle**, compiling actual Rust to WebAssembly. The DSL and separate Field Code suggestions below are historical alternatives, superseded by that decision. Implementation progress is tracked in [dev-plan.md](../../dev-plan.md).

Research date: 2026-10-05. Proposed design, not an implemented capability or a change to accepted phase priorities.

Fold should support a built-in programmable Motion node: an agent supplies source, typed inputs/outputs, and control metadata; Fold validates and compiles the snippet while running. The source and interface become ordinary project data. Artists can animate its controls and package it inside an editable group. Implement the host once in Rust; subsequent snippet edits require neither an application rebuild nor a relaunch.

Start with a small typed expression language and a bounded CPU interpreter. Keep general Rust-to-Wasm modules as a possible later extension for algorithms that exceed the expression model. A code node can compress custom math and attribute processing substantially, but it cannot invent engine capabilities that its host does not expose.

## Existing implementation and the missing seam

| Existing Fold capability | Implication |
| --- | --- |
| [Static node registry](../../crates/motion/src/graph/registry.rs) and [Definition callbacks](../../crates/motion/src/graph/definition.rs) | Register one built-in Code node. Each instance can carry different source; runtime registration of native Rust functions is unnecessary. |
| [Instance-dependent signatures](../../crates/motion/src/graph/signature.rs) | Inputs and outputs can be derived from a validated declaration on each Code node. |
| [Typed fields](../../crates/motion/src/fields/mod.rs) | Scalar, Vector, Color, Bool, Text and Id values already exist. Samples carry position, index, stable ID, count and an exact time offset. Fields are currently Rust closures, not a compiler IR. |
| [Immutable geometry](../../crates/motion/src/geometry/mod.rs) and [attribute access](../../crates/motion/src/nodes/attributes.rs) | Wrangles can read input elements and produce new elements using existing values and identity rules. Current geometry is `Arc<Vec<Element>>` with per-element attribute maps, not the columnar GeometrySet described in the proposal. |
| [Reusable groups and ports](../../crates/motion/src/nodes/interface.rs) | Published inputs already have stable IDs, names, types, defaults, units, sections and advanced status. Extend this metadata only where needed for ranges and control presentation. |
| [Evaluation](../../crates/motion/src/evaluation/mod.rs), [temporal sampling](../../crates/motion/src/evaluation/temporal.rs) and [prepared roots](../../crates/motion/src/prepared.rs) | Execute code through Motion's existing demand evaluation and immutable render compilation. Preserve delayed sampling through groups and existing Solver restrictions. |
| [Package authoring](../../crates/motion/src/automation.rs) and [atomic batches](../../crates/app/src/automation/batch.rs) | Agents can already add nodes, patch settings, wire inputs and author groups. Extend discovery and diagnostics rather than introducing an unrestricted eval endpoint. |

The [architecture proposal, section 11](../Fold_Application_Product_Architecture.md#11-procedural-motion-graphics-evaluation), explicitly anticipates a typed expression IR shared by field math, bindings and snippets. That is direction, not completed implementation. Its runtime package discussion is separate: the first code node need not enable dynamic native plugins, which remain deferred in [dev-plan.md](../../dev-plan.md).

Motion and Compositing share graph presentation, but have different execution models. The [compositor graph](../../crates/compositor/src/graph.rs) is not interchangeable with Motion fields. Begin in Motion; a future pixel-code node needs its own image/channel, bounds, color, precision and CPU/GPU contracts.

## What to borrow from Houdini

An Attribute Wrangle runs code over a chosen geometry domain, binds attributes, and can sample parameters using `ch()`. Houdini describes it as the textual counterpart of an Attribute VOP network. The useful pattern is code plus an explicit domain, attributes and normal node parameters—not arbitrary access to the application's internals. [SideFX Attribute Wrangle](https://www.sidefx.com/docs/houdini/nodes/sop/attribwrangle.html)

For Fold, distinguish two uses:

- **Field code:** returns a typed value at a consuming element sample. Use it for formulas, timing, falloffs, color and procedural offsets; connect it to existing geometry operations.
- **Element code:** runs over an explicit domain, reads an immutable element and produces permitted attribute/transform changes. This is the closest equivalent to a wrangle. Start with points, then add instance/glyph scopes using the same core runtime.

Persistent state belongs in Fold's existing Solver. A code node inside its update graph can calculate the next state from Previous State and the supplied step context. An ordinary code node must not accumulate hidden state as the playhead moves.

## Language and runtime choice

| Approach | Fit for Fold | Cost or limitation |
| --- | --- | --- |
| Small typed DSL → typed program → CPU interpreter | Recommended production starting point. Explicit types, dependencies and domains align with existing fields. | We own a parser, type checker, diagnostics and versioned semantics. Keep the language deliberately small. |
| Rhai embedded in Rust | Fastest experiment with editable source and cached compiled ASTs. | Dynamic scripting semantics and per-element overhead need measurement. Fold still supplies domain access, controls, budgets and deterministic functions. It does not automatically become Fold IR or GPU code. |
| Rust → Wasm → Wasmtime | Possible advanced custom-algorithm tier. Reload modules without restarting Fold. | Rust snippets still require compilation and a managed toolchain; define a versioned data contract instead of sharing Rust references. Bulk transfers and build latency need measurement. |
| WGSL compute | Potential later target for a compatible numeric subset. | Requires GPU resource integration and deliberate semantics/precision parity. Not the first general Motion language. |

Rhai explicitly supports compiling source once to an AST and evaluating it repeatedly. That is a practical prototype route, not native machine-code compilation. [Rhai compilation](https://rhai.rs/book/engine/compile.html)

Wasmtime supports fuel and epoch interruption, but neither makes arbitrary host functions safe or bounded. These mechanisms concern module execution; the host must still constrain memory, imports and native calls. [Wasmtime interruption](https://docs.wasmtime.dev/examples-interrupting-wasm.html)

Rust's `wasm32-unknown-unknown` target provides a concrete route for compiling guest modules; Wasmtime consumes Wasm, not Rust source. A future compiler worker should pin the SDK/toolchain and exclude arbitrary dependency build scripts. [Rust Wasm target](https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html) Runtime GPU source is also possible through wgpu shader-module creation, but needs a separately implemented execution adapter. [wgpu ShaderModule](https://docs.rs/wgpu/latest/wgpu/struct.ShaderModule.html)

The production DSL should initially support typed locals, arithmetic, comparisons, conditionals, vectors/colors, seeded randomness, a small math library, declared inputs and explicit outputs. Exclude recursion and general loops initially. Add bounded operations only when a real workflow needs them. Reuse existing numeric functions; migrate shared visual math to the same typed operations incrementally rather than rewriting the node library first.

## Source and controls

This is illustrative syntax, not valid current Fold code. The node would run over points; position is a 2D translation in the declared input coordinate space, frequency is cycles per second, and phase is cycles per element:

```text
input amplitude: scalar = 40.0;
input frequency: scalar = 1.0;
input phase: scalar = 0.08;

let angle = tau * (time.seconds * frequency + element.index * phase);
out.position = element.position + vec2(0.0, amplitude * sin(angle));
```

An accompanying declaration supplies stable parameter IDs, labels, units, defaults and optional soft ranges. For example, `amplitude` appears as **Amplitude**, with a soft range of 0–200 in composition units. The artist sees normal controls with existing animation and publication actions. The code editor is a secondary inspector action, using shared UI components and visible inline errors, following the [Fold UI skill](../../.agents/skills/fold-ui/SKILL.md).

Source declarations and metadata must compile into one validated interface. Do not maintain separate inconsistent socket and control lists. Renaming a display label must not change the stable ID. Removing or changing a parameter's type must diagnose affected wires and animation tracks and require an explicit migration; never silently reconnect them by position.

Wrap the Code node and any supporting native nodes in an existing group, expose its controls, and name it as the custom tool. A named group containing code is sufficient for the first reusable asset. Avoid building a new dynamic plugin registry just to make it appear as one graph node.

## Channel references are dependency declarations

Start with declared inputs and normal links. If convenient `ch("amplitude")` syntax is added, resolve its literal argument against stable input IDs when compiling. A reference to another node's channel should become a serialized, validated dependency, with a readable label as presentation. Arbitrary computed paths and ambient project lookup should be excluded initially.

Channel reads must sample at the requested local time, including the current element's delay. Reuse the existing exact-time machinery; do not capture one slider value when compiling or read the current UI playhead during export. Keep exact rational time at the host interface and convert to floating-point seconds explicitly for formulas.

The program records required inputs, domain, time dependence and permitted writes. Those facts support cycle detection and cache invalidation. Channel renames, source edits, animation changes and Solver updates must retain ordinary graph semantics.

## Compilation and execution contract

Keep the first implementation within `fold-motion`, with an internal module that owns parsing, validation, compiled programs and bounded execution. Its interface takes source plus typed declarations and returns an immutable compiled program or structured diagnostics; execution takes that program, explicit inputs, sample/element context and a shared work budget. Extract a feature-neutral crate only when a second real consumer needs it. Project and render infrastructure must remain free of Motion models.

1. Edit source in a transient editor buffer, or submit source/interface changes through the existing agent batch path.
2. Parse and compile off the UI thread with source-size, syntax-depth and program-size limits. Keep structural document validation cheap; do not compile inside every signature lookup.
3. Cache compiled programs by source, interface and language/runtime semantic version, with bounded retained memory. Time and animated parameter values are runtime inputs, not program-cache keys.
4. Activate a successful source/interface revision atomically through normal transactions. Recheck project revision after asynchronous preparation so intervening artist edits are not overwritten.
5. Evaluate the immutable program on workers through the same Motion evaluator used for preview, export and headless operation. Existing snapshots retain their previous program while work is in flight.

Keep draft code separate from the committed executable source. A compile error leaves the previous committed version active and visibly reports the draft error. A runtime failure of committed code reports failure; never silently export an older successful result. Source and semantic version are authoritative saved data; compiled artifacts are disposable caches. Preserve unsupported source payloads on load and report unavailable execution when demanded.

Budget instructions inside the interpreter, not just entry into the Code node. The current Field budget alone cannot stop a script that loops inside one closure. Bound memory, outputs, host-function work and compilation as well as execution; poll cancellation. Start with no filesystem, network, process, project mutation or raw GPU access. Each element reads the original input snapshot, preventing accidental dependence on iteration order. Validate finite values and domain/type compatibility. Seed randomness from explicit seed and stable element/node identity, not process state.

A safe interpreter is still an implementation that needs adversarial testing; these restrictions are design requirements, not proof of isolation. CPU/GPU floating-point agreement and cross-platform bitwise reproducibility must not be promised without defining and testing them.

## Agent workflow and first acceptance slice

Expose language version, supported types/functions/domains and example programs through the package catalog. Extend instance inspection to include resolved sockets and control metadata; the current catalog primarily reports static definition defaults. Return diagnostics with node, source range, expected/actual type and actionable message. Add non-mutating compile validation and bounded evaluation probes, reusing the existing protocol's jobs and revisions where appropriate.

The agent loop becomes inspect → write code and interface → validate → commit one batch → render representative times → inspect result → refine. It still uses normal revision checks and undo. Do not expose generic host eval or a script API that can mutate the project during rendering.

First deliver a scalar/vector Field Code node with source persistence, typed inputs, normal animation, worker compilation, structured diagnostics and MCP authoring. Prove it with a procedural offset/color study that currently needs several math nodes. Then add a point-domain element wrangle and package a reusable group around it. Broader topology operations, neighbor queries, image code and Wasm belong to subsequent demonstrated needs.

Acceptance should cover source/control edits without restart; undo/redo and save/reopen; invalid-source recovery; rename-stable bindings; cycle rejection; exact delayed sampling through groups; budget/cancellation failures; out-of-order frame evaluation; and preview/export/headless agreement. Benchmark equivalent native graphs and snippets at representative element counts before making performance claims. A shorter graph and fewer MCP calls are hypotheses to measure, not evidence of faster rendering.

Continue reference workflows as acceptance tests. Wrangles should eliminate repeated application implementation for custom formulas and element operations; they do not replace hardening of geometry, fonts, resource ownership, interaction or rendering.
