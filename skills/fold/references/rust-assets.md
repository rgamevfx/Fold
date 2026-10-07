# Live Rust assets

For procedural asset creation and source revisions, use Rust asset SDK v1. Fetch `catalog guide="rust-sdk"` for the complete SDK. A Rust source body returns an `Asset`; Fold compiles it on a worker to a retained native group. Ordinary controls and native fields evaluate without recompiling source. Custom runtime Wrangles inside the group compile separately when needed.

## Create and review

1. Select the session, read state and begin a candidate at the current revision/epoch. All operations below fit existing `candidate action="edit"` batches. Await each job with `jobs {job, wait_ms:25000}` and use its returned generation. A deadline returns `pending` without cancelling work; await again if needed.
2. Create a `fold.composition.document` with a normal `create` operation. Discover its edits with `catalog type="fold.composition.document"`. Before adding the asset layer, set its format if project defaults differ from the brief: `{"op":"edit","document":"COMPOSITION_UUID","edits":[{"op":"format","width":960,"height":540,"rate":[30,1],"frames":180}]}`. The layer source inherits this format.
3. In one batch, allocate keys `graph`, `definition`, `instance`, `layer` in `ids` and submit this operation. `composition` is the existing Composition ID; `source` contains the Rust body:

```json
{"op":"command","id":"fold.app.create-rust-asset","arguments":{"composition":"COMPOSITION_UUID","graph":{"$ref":"graph"},"definition":{"$ref":"definition"},"instance":{"$ref":"instance"},"layer":{"$ref":"layer"},"source":"RUST_SOURCE"}}
```

4. The source graph is layer content, and the asset is a selectable native Composition layer. Observe the **Composition** `video` output at multiple exact times. Inspect the source graph's `groups`, `graph` and `rust_assets` for the generated construction and saved source.
5. Commit one reviewed candidate, save, then verify the saved result. Do not claim a compile success proves visual quality or working controls.

## Source body

```rust
let mut a = Asset::new("Moving title");
let label = a.text_control("text", "Text", "Hello Fold");
let size = a.scalar("size", "Size", 100., "px");
let amplitude = a.scalar("amplitude", "Amplitude", 30., "px");
let paint = a.color("color", "Color", [0.3, 0.04, 0.7, 1.]);
let title = a.text("title", label, size);
let title = a.fill("paint", &title, paint);
let title = a.align("center", &title, [480., 270.]);
let offset = (time() * 6.283185307179586).sin() * amplitude;
let title = a.translate("motion", &title, vec2(0., offset));
a.output(&title);
a
```

Coordinates use the source document's design pixels. `align` follows analytic ink bounds, excluding stroke/effects. Text uses an embedded font; generic Text-node settings can explicitly choose an existing embedded font identity. Colors use the project's working interpretation; compare under the actual project color policy.

`Signal` is symbolic retained math. Rust `+ - * /`, unary `-`, `.sin()`, `.cos()`, `.floor()`, `.abs()`, `.modulo()`, `.min()` and `.max()` create native expressions; `.clone()` reuses a signal. Operands must have matching numeric types. `vec2(x,y)` combines scalar signals. `time()` is local seconds derived from exact evaluation time; `index()`/`count()` are sampled in the consuming domain. Ordinary Rust `for`, local functions and collections execute only at construction time. Runtime conditionals must be native nodes or an explicit Wrangle, not Rust branching on a Signal.

Creative methods: `text`, `circle`, `rectangle`, `fill`, `align`, `translate`, `grid`, `shape_points`, `copies`, `merge`, `wrangle`. Every operation has a stable key; use `key("prefix", i)` inside loops. Duplicate keys are errors. `merge(back,front)` paints front over back. Copy geometry is retained and shared, not flattened. A grid returns Points; `shape_points` samples filled text/paths into Points; `copies` returns Instances containing the source. Choose Wrangle domains deliberately: Points before copying, Instances after copying, Children for a source's direct children. Whole-object motion uses `translate`.

For custom algorithms:

```rust
let points = a.grid("grid", 12., 8., [24., 24.], [0., 0.]);
let points = a.wrangle("wave", &points, "Points",
    "position[1] += chf(\"amplitude\") * (time.seconds + index as f64 * 0.2).sin();",
    &[("amplitude", amplitude)]);
```

Wrangles use the existing Rust element API described in `catalog guide="graph"`; custom attributes survive Points → Instances. Runtime source has its own bounds. Source strings are literal Rust strings, not macros.

`a.add(Node::new(key,kind).settings(json).value(socket,signal).link(socket,&output))` is the low-level escape hatch. Discover native node signatures first. `.socket("points")` or `.socket("value")` selects an alternate output; `add` defaults to `content`. Settings are JSON objects validated by the owning node. `a.input(id,label,"Content")` or `"Points"` exposes a required connection; it has no default and must be wired before rendering. Scalar/text/color/vector declarations expose controls with defaults and units.

## Revise without losing artist edits

To change source, edit the existing source document with:

```json
{"op":"rust_asset","definition":"DEFINITION_UUID","source":"REVISED_RUST_SOURCE"}
```

Omit `instance` on revision. Optional `instance` creates a new bare group node; common Composition creation uses the application command above. Existing instance values, connections, animation, placement and identity are preserved. Changing a source default changes defaults for **new** instances; it does not overwrite existing artist values. To deliberately adjust an existing control, use normal `set` or animation operations on the instance.

Changing parameter labels retains IDs. Removing/changing a parameter's type requires an explicit migration (below). Source owns generated internals. Graph layout edits are retained; semantic edits inside the generated group cause a conflict on recompilation. Make an independent ordinary group to retain such edits before continuing source ownership. Definition updates affect every instance in the source document. Do not duplicate artwork when a source update fails.

The native inspector offers **Edit Rust source**, background **Compile and apply**, cancellation and error feedback. Asset controls use the existing inspector and Composition animation tracks. Native graph output persists; reopening does not rerun author-time Rust. Runtime Wrangles still need the installed compiler on cache miss.

## Limits

Requires the same installed Rust/wasm32 target and `prlimit` as Rust Wrangles. Author source is bounded to 64 KiB, 1024 opening delimiters; no user macros, attributes, imports/modules or static state. SDK helpers and local functions are available. Guest execution has 16 MiB memory, 10 million fuel and 1 MiB returned-description limits; compilation is cancellable and limited to 30 seconds. At most 64 inputs and 1024 declared nodes; native graph budgets also apply. No filesystem, network or process imports are linked into the guest. Custom runtime code uses existing Wrangle budgets. Arbitrary source/native-graph merging and coordinated migration of dependent source-owned assets are not supported. Portable bundles reject external media, scene references and document dependencies; expose these as explicit asset inputs.

## Verified artwork workflow

The [NOV cell conveyor](../../../demos/cell-type-rust.md) was authored by a fresh requested GPT-Sol-6.1 agent through public MCP. Its 49-line source generates an editable native asset with nine controls. The recorded first trial failed its loop seam; a separately recorded continuation repaired it. This is one example with measured limits, not a general quality or performance guarantee.

- Publish text/size/spacing/palette controls first. Connect ordinary Published Control and Keyframes nodes to the instance when an artist needs an external driver; source revision preserves those connections. Edit those drivers rather than replacing a linked input with a literal.
- For shaded copies, preserve the paints of the shared cell source. A per-instance color override can replace descendant paint and flatten layered shading; selecting among reusable shaded sources retained it in this example.
- A cyclic translation must wrap beyond the complete visible geometry, including radius/shadow. Check the moments immediately before and after entry/exit as well as matching phases. Repeated endpoint images can conceal an appearing fragment.
- `control_response` now fingerprints the full bounded native construction even when element details are truncated. It proves an evaluated change, not visible pixels. Images/effects remain inconclusive; render the original and a private candidate at the same exact time to judge the intended visual response.
- Await compilation/observation with `jobs {job, wait_ms:25000}`. Waiting does not block the application UI. The first trial's reused 50 ms polling helper consumed 127 of 193 public calls; the helper now uses bounded waiting.


## Timing, layout and artist controls

`progress(start,duration)` returns clamped 0–1 local-time progress; `.smooth()` applies cubic ease-in/out. `phase(period)` wraps local time into 0–1. `stagger(start,duration,delay)` delays that progress per sampled `index()`. These are retained signals: changing connected controls requires no compilation. Nonpositive duration/period uses a 1 μs minimum. `a.translate_each(key,&copies,vec2(stagger(0.,1.,0.05)*100.,0.))` samples each instance; `translate` moves the whole object and does not accept varying fields. Scalar `.mix(to,progress)` interpolates, and `.clamp(low,high)` bounds evaluation.

`a.keyframes(key,&[(0,1,0.),(1,3,100.)],"Ease")` creates native scalar keyframes at exact rational seconds `(numerator,denominator,value)`. Connect the returned output using `Node::link`. Interpolation is `Hold`, `Linear` or `Ease`. Source-authored internal keys belong to source; artist animation belongs on instance controls or connected external keyframe nodes.

`a.fit(key,&content,origin,size)` uniformly contains current analytic bounds in the rectangle, centered. It follows text/geometry edits at evaluation time. `a.align_anchor(key,&content,target,anchor)` aligns a normalized `[0,0]`–`[1,1]` bounds anchor. Both exclude stroke/effect extents. Fit requires positive finite target dimensions and handles empty or zero-extent geometry.

`a.scalar_range("radius","Radius",24.,"px",[1.,100.])` adds inspector drag limits; `a.describe("radius","Geometry",false,"Size of each circle")` adds section, advanced visibility and description. These are soft UI limits, preserving existing values and animation. Use signal `.clamp()` when the algorithm needs hard evaluation limits. Ordinary group interfaces expose the same metadata through native editing and the `port` edit (`range`, `clear_range`, `description`). Edit Rust-owned metadata in source to avoid an ownership conflict.

## Explicit interface changes

Include `migrations` in the same `rust_asset` edit as revised source:

```json
{"op":"rust_asset","definition":"DEFINITION_UUID","source":"REVISED_SOURCE","migrations":[{"action":"rename","from":"speed","to":"velocity"}]}
```

Rename requires the same type and preserves values, links, visibility and native animation identities across root and ordinary nested instances. Removal uses `{"action":"remove","input":"radius"}` and deliberately discards that input and its keys. Type replacement uses `{"action":"replace","from":"velocity","to":"offset","value":{"Vector":[0,0]}}` and explicitly resets bindings/animation. Invalid or ambiguous migrations fail atomically. Destinations must not collide with another existing input; use separate revisions. A dependent source-owned group causes a conflict: coordinated multi-source migration is not supported. The native source editor's **Interface change** section provides the same actions, using the new source default for replacement.

## Portable library files

Discover `catalog method="asset"`. Export a reusable definition from a committed Motion document:

```json
{"action":"export","document":"MOTION_DOCUMENT_UUID","definition":"DEFINITION_UUID","path":"/absolute/title.foldasset","request_id":"export-title-1","revision":12,"project_epoch":1}
```

Import into an existing Composition using fresh graph/instance/layer UUIDs:

```json
{"action":"import","composition":"COMPOSITION_UUID","graph":"NEW_GRAPH_UUID","instance":"NEW_INSTANCE_UUID","layer":"NEW_LAYER_UUID","path":"/absolute/title.foldasset","request_id":"import-title-1","revision":12,"project_epoch":1}
```

Both return jobs; await with `jobs {job,wait_ms:25000}`. Import is one undoable project transaction. Export never overwrites an existing file. Bundles contain native dependent groups, controls/defaults, source/identity history and fonts, with unknown extension metadata preserved. Import does not execute source. Stable-ID conflicts reject rather than replace existing definitions. Files are bounded to 64 MiB. External project/media references must be exposed as inputs before packaging; instance-specific artist values and animation stay in their original project. Native group context menus provide **Load procedural asset** and **Save procedural asset**; loading into an existing instance retains compatible inputs and removes incompatible inputs/keys.

For large native constructions, query `{"mode":"verify","check":"control_response","control":"INSTANCE_UUID","input":"radius",...}` with the usual `values`, `target`, and exact `times`. Returned `fingerprint.complete` covers all evaluated elements within 100,000-element, 64-level and 2,000,000-path-segment budgets even when `elements` is truncated. Diagnostic hashes are not persistent identities or pixel-visibility evidence. Matching fingerprints can establish repeated native geometry; mismatching truncated samples cannot establish tolerance-based periodicity.
