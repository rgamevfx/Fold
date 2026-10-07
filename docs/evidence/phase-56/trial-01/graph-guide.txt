# Graph authoring and Rust Wrangles

Use this reference when constructing or restructuring a network, publishing group controls, or writing a custom Rust Wrangle. Discover the running catalog first: older desktops will not expose these operations. Submit edits through the normal revision-checked `batch`; every example below is a package edit inside `{"op":"edit","document":DOCUMENT,"edits":[...]}`.

## Discover the actual interface

Motion catalog entries include settings choices, input sampling (`uniform`, `field`, `geometry`, or `consumer-dependent`), and default setting shapes. Shapes describe examples, not an exhaustive JSON Schema. Inspect a candidate configuration before wiring polymorphic nodes:

```json
{"document":"MOTION_UUID","query":{"mode":"signature","kind":"fold.motion.math","settings":{"operation":"Multiply","value_type":"Vector"}}}
```

`inspect` is a job. Read its completed `outcome.result.data`. A node query returns authored data, resolved sockets, bindings, animation and consumers:

```json
{"document":"MOTION_UUID","query":{"mode":"upstream","node":"NODE_UUID","group":"GROUP_UUID","depth":2,"limit":30}}
```

Omit `group` for the root graph. Use `nodes` for direct inspection and `downstream` for consumers. Neighborhoods are limited to 200 nodes and depth 32; reduce depth rather than requesting a whole large document. Compositing supports `nodes`, `upstream` and `downstream` with the same envelope. Motion `handles` accepts a `nodes` list and supplies scene-object roles and group-definition IDs.

To review changes after a known revision, call `inspect` with `document`, `since_revision`, and the current `project_epoch`. If that history root has expired, inspect the current neighborhood. Mutation receipts already report changed node handles, removed nodes, group interfaces, and newly changed object source/transform/appearance roles; use them before issuing another inspection.

## Create and connect together

Use `create_node` for configured Motion nodes. Inputs use the persisted, mutually exclusive `Value` or `Link` form; temporary references still use `{"$ref":"name"}`:

```json
{"op":"create_node","id":{"$ref":"scale"},"kind":"fold.motion.math",
 "settings":{"value_type":"Vector","operation":"Multiply"},
 "inputs":{"a":{"Link":{"node":{"$ref":"position"},"socket":"value"}},
           "b":{"Value":{"Vector":[2,2]}}}}
```

Fold specializes defaults after settings. It does not silently convert socket types. `create_node` creates a graph node, while `object` creates an artist-visible scene object and its required resources/appearance/placement. Use `object` for text so its font is established.

Compositing `create_node` accepts `parameters` and an `inputs` map from named sockets to source IDs. Its structure remains distinct from Motion. Read the compositor catalog rather than copying Motion's parameter envelope.

## Publish and restructure

- `publish`: `group`, internal `node`, `input`, stable `port`, display `name`, optional `section`. Creates the port and internal wiring in one edit. Existing numeric input animation moves onto current group instances; an unused definition with animated input requires an instance first. Connected inputs are rejected to avoid discarding their driver.
- `port`: update a published control's `name`, `section`, or `advanced` flag using `group` and stable `port`. Existing connections/animation retain their IDs.
- `insert`: select a consumer `node`/`input` and an existing `modifier`/`modifier_input`/`output`. Fold redirects only that connection and preserves its former input on the modifier. Compositing omits `output` because its operators have one output.
- `replace`: retain node identity while changing `kind`/`settings` in Motion, or `parameters` in Compositing. Incompatible sockets or animation are rejected; migrate them explicitly.
- `duplicate`: supply source `node` and new `id`. Motion requires `independent`: false shares referenced definitions; true copies nested definitions and independent published controls. The copied graph node retains upstream references and is not automatically a new scene layer.
- `group_nodes`: Motion processing-node selection `nodes`, outer instance `id`, new `definition` ID, and `name`. Fold creates the boundary ports and redirects external consumers. Keep scene-owned and group-interface nodes outside this selection.
- `layout`: Motion `nodes` plus optional `group` and `origin`. Compositing uses `layout_nodes`. Only listed nodes move; preserve the artist's existing layout unless the task requests otherwise.

Follow a failed structural operation's specific diagnostic rather than replacing entire arrays. All edits in the batch validate before publication. Motion also supports non-mutating `inspect` query `{"mode":"validate","edits":[...]}` for a document-local proposal; a successful result does not commit it or prove its appearance.

## Rust Wrangle

Create `fold.motion.wrangle`. The inspector contains the Rust source directly, **Create Controls**, **Apply**, and **Run Over**. Source typing is a draft; Apply compiles on a worker and commits only on success. Create Controls preserves existing values, animation, connections, and unused controls. Changing the type of an existing channel requires explicit migration.

For agents, `wrangle` updates a node's `source`, synchronizes channels, and compiles before the batch can commit. `create_node` performs the same preparation when creating a Wrangle with custom source. `create_controls` synchronizes an existing node from its committed source. Use ordinary `set`, `connect` and native animation edits for its resulting channels.

Example Rust function body:

```rust
let radius = chf("radius");
let phase = tau * (index as f64 / count as f64 + time.seconds * chf("speed"));
position = [radius * phase.cos(), radius * phase.sin()];
set_attrf("phase", phase);
```

Channels: `chf` scalar, `chv` 2D vector, `chc` RGBA color, `chb` boolean. Each takes a literal stable name. Set useful initial values after creation; newly discovered scalar/vector/bool controls default to zero/false and color to white. Use a group to publish artist labels, sections and a reusable tool interface.

Writable variables are `position: [f64;2]`, `color: [f64;4]`, and `opacity: f64`. Read-only context includes `index`, `count`, stable `id: u64`, `time.seconds`, exact `time.numerator`/`time.denominator`, `delta_time` (Solver step, zero outside Solver), and `tau`. Position is the element's translation in input coordinates; colors retain the project's working-space interpretation.

Numeric attributes use `attrf/attrv/attrc/attrb("name")` and `set_attrf/set_attrv/set_attrc/set_attrb("name", value)`. Missing reads use zero/false or white; reads alone do not create attributes. Existing mismatched attribute types fail. Unmodified geometry, identity and attributes are retained. Run Over selects top-level Points, Instances, Glyphs, or Children; it does not implicitly realize instances or traverse descendants. `geometry_kind` is `Points` or `Content`; the inspector chooses the matching kind with Run Over.

The guest is actual Rust 2024 compiled to Wasm, not a Rust-like expression language. Use local expressions, control flow and functions. Macros, attributes, modules, imports, static declarations and external dependencies are excluded. Runtime access is limited to Fold's numeric bindings; there are no filesystem/network/project-edit imports. State for simulation belongs in Solver attributes, not guest globals.

Compilation requires `rustc`, the `wasm32-unknown-unknown` target, and Linux `prlimit`; `FOLD_RUSTC` can select the compiler. Installing the target is an environment setup step. A missing toolchain is an error, not a reason to replace the desktop or bypass compilation.

Source and ABI version are saved in the project. Compiled programs are bounded, disposable process-local caches. Source size, parsing complexity, compiler time/memory, guest memory and instruction fuel are bounded. Consult `catalog.wrangle` for limits. Compilation errors expose structured messages and source spans; runtime errors identify the element. Preserve the last committed source after a failed proposal; never report that failed source as active.

## Verification

Render representative times through public `render`, then edit one published channel and render again. Check correspondence across attached labels/markers, group reuse, animation and out-of-order seeking. Check an invalid source proposal leaves the revision unchanged. Save/reopen and verify the source, controls and rendered output. Code brevity is not a throughput guarantee: measure cold compilation separately from warm evaluation.

Verified example: [Rust Orbit](../scripts/wrangle.py) builds and publishes a Rust tool using receipts and targeted operations, tests failed/hot edits and undo, and saves a project copy. Run it only in a session where adding its Motion document fits the task. The resulting [editable demo](../../../demos/fold-rust-orbit.md) explains the artist controls.

For selecting native operations versus custom code, verifying runtime cost at the intended element count, or checking a proposed GPU path, use [Performance and output equivalence](performance.md#choose-the-execution-model-deliberately). It is the authoritative workflow for numeric guest reuse, isolated fallback execution and current versus proposed capabilities.

## Multilayer EXR color and routing

For a numbered EXR sequence, import one representative frame and place that asset into a composition. Inspect the resulting Read: sequence discovery supplies the actual channel names, frame range and rate mapping. Multipart files may include both the part name and channel prefix; use the inspected full names rather than guessing from Blender's view-layer labels. Placing an EXR does not automatically connect it to Output.

The compositor catalog advertises `input_color` to set a Read's per-use OCIO interpretation:

```json
{"op":"input_color","node":"READ_UUID","space":"Linear Rec.709 (sRGB)"}
```

Set `space:null` to restore the asset/default interpretation. Set the Read's `source.Exr.color_layers` to the inspected RGB layer names that need this conversion. Route their red/green/blue/alpha channels into `rgba` with the catalog's Shuffle mappings. Preserve `source_start` when moving Read `start` to zero to avoid a leading empty frame; retain the source's actual FPS. Validate color through a rendered frame, not merely a successful edit. A missing OCIO space fails at render time.
