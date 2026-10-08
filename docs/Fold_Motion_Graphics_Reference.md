# Fold motion graphics reference

> **RETIRED — 8 October 2026.** Historical design/implementation context only. The [Motion System Rewrite Plan](Motion_System_Rewrite_Plan.md) and its linked proposal supersede this document for motion and agent work. Examples and protocol descriptions below belong to the old system and are not rewrite contracts.

Source snapshot: 8 October 2026. Scope: the implemented Motion engine only. The compositor node system and render graph are excluded.

## Engine and data

Fold Motion is a procedural **2D** graphics engine. It creates vector shapes, shaped text, point distributions, shared copies, image geometry, animation, and bounded simulations. Its normal construction is **Content → Distribution → Driver → Influence → Response**.

The engine is written in **Rust, edition 2024**, in `fold-motion`. Shared animation code is in `fold-animation`. The optional desktop interface uses Dear ImGui through Fold's shared UI SDK. Graphs and settings are serialized data, not executable Rust source. Rust code is optional for custom operations and reusable assets.

| Term | Meaning |
| --- | --- |
| Content | An ordered, shared collection of elements. Each element has an ID, geometry, affine transform, appearance, and named attributes. |
| Geometry | Path, shaped glyph, point, instance, group, or textured image geometry. Paths contain move, line, quadratic Bézier, cubic Bézier, and close segments. |
| Instance | A reference to shared source content, with its own transform and attributes. Copies do not require duplicate source paths. |
| Field | A typed function sampled by a consumer. It can be uniform or vary by element. Context includes position, index, stable ID, count, and time offset. |
| Domain | The elements a response changes: Points, Instances, Glyphs, or Children. Children selects the current content level. A glyph is not necessarily one character. |
| Value types | Scalar (`f64`), Vector (`[f64; 2]`), Color (`[f64; 4]`), Bool, Text, and Id (`u64`). Other socket types are Content, Points, Image, and ImageField. Generic sockets resolve to a declared concrete type. |

IDs and order are different. Random values can remain attached to an ID when order changes. Index-based operations follow current order. Source edits can add or remove IDs; text edits can change glyph correspondence.

Coordinates increase right and down. Angles use degrees unless a function specifies radians. A transform applies `translation + pivot + rotation × scale × (point − pivot)`. Positive rotation is clockwise in these screen coordinates. Geometry and field math use mainly `f64`; GPU drawing uses lower-precision representations where required.

A logical canvas separates scene units from output pixels. Fit and Fill preserve proportions; Responsive preserves logical height and adjusts width for aspect ratio. Format provides scene units per full-output pixel. Preview resolution does not change layout. Older documents without a logical canvas retain their original coordinate behavior.

Colors use straight RGBA in the document's working space during authoring; drawing uses premultiplied alpha. Current new projects use scene-linear ACEScg. Legacy projects can use legacy linear sRGB interpretation. Finite HDR and negative RGB values are supported; display RGB values are not automatically working-space values. Opacity and paint alpha must remain in their supported 0–1 range.

## Execution

1. An edit changes a candidate document. Validation and a project transaction commit it with undo. Unknown extension data is retained.
2. A frame request supplies an immutable project snapshot, exact rational time, output format, and cancellation state.
3. The evaluator starts at the requested scene or graph output. It resolves dependencies on demand, dispatches nodes through the static registry, and caches node outputs within that evaluation. Cycles, missing demanded implementations, and invalid types produce errors.
4. Field nodes build typed functions. A geometry consumer samples them in an explicit domain. Uniform evaluation rejects an element-varying field; it does not invent element zero. Numeric operations require compatible types and finite results.
5. CPU code constructs geometry, samples ordinary fields, shapes text, and advances simulations. Shared geometry and per-instance transforms are retained where supported. Motion then supplies immutable drawing data to the shared renderer.
6. GPU paths draw vectors, instances, textures, and image responses. **Ordinary domain-varying fields and Rust Wrangles still execute on the CPU.** GPU image sampling does not expose pixels to CPU Math or Wrangle nodes. Preview, export, and headless requests use the same Motion evaluation semantics.

Pure animation can be evaluated at any requested time. Time is a rational fraction for requests, key positions, and temporal boundaries; numeric fields can convert it to floating-point seconds. Stagger converts delay to nanoseconds before applying its exact time offset. Per-card media offsets are quantized to microseconds.

Solver is an explicit state region. For start `s`, step `h`, and request `t`, it evaluates `n = max(0, floor((t − s)/h))` updates. Initial state is sampled at `s`; update `j` runs at `s + jh`. There is no interpolation between solver steps. Valid checkpoints reduce replay work; eviction changes cost, not the intended result. Changes to evaluation identity prevent reuse of stale state.

## Math and animation

| Operation | Rule |
| --- | --- |
| Mix | `a + (b − a)w`, component by component. Weight is not automatically clamped. |
| Map Range | `u = (x − source_min)/(source_max − source_min)`; optionally clamp `u`, then optionally use `u²(3 − 2u)`; return `target_min + (target_max − target_min)u`. A zero source interval is invalid. |
| Falloff | Normalize and clamp a coordinate or radial distance between two limits. Optionally apply smoothstep. Normal output is `1 − u`; invert returns `u`. |
| Copy Progress / Palette | `i/max(N − 1, 1)`. Palette mixes its two colors with this weight. |
| Wave | `offset + amplitude × shape(coordinate × frequency + phase)`. Sine uses `sin(2πu)`; triangle and square produce values from −1 to 1. |
| Oscillator | Phase is `(time + offset) × time_scale × frequency + phase + spread × i/max(N − 1, 1)`. Frequency is `1/duration` or `BPM/60`. Each channel can add a phase offset. Output mixes minimum/maximum with `0.5 + (wave01 − 0.5) × strength`. |
| Stagger | Sample the upstream value at `t − i × delay`. Animated upstream parameters and group inputs use the shifted time. |
| Timing | Normalize the owning range to progress. Clamp holds endpoints; Repeat wraps modulo 1; PingPong travels forward and back. |
| Keys | Hold, linear, or eased interpolation. Basic Ease uses smoothstep. Per-component animation curves also support cubic Bézier handles. Values hold outside the key range. |
| Response | Selection chooses elements; weight controls the change. Replace blends toward the requested value. Add adds a weighted amount. Multiply blends a multiplier from 1. Each response permits only its supported modes. |
| Path travel | Sample distance along a flattened path and derive its tangent. Quadratic curves use 64 subdivisions; cubic curves use 96. This is approximate arc length, not an analytic curve-length solution. |

Particle Step uses an implicit spring with drag. For each axis, let `h` be delta time, `q` the target, `x` position, and `v` velocity:

```text
ω = 2π × frequency × (1 − variation + 2 × variation × seeded_random)
k = ω² × attraction
c = 2 × damping × ω
F = acceleration + turbulence × sin(2.3 × time + seeded_phase)
v_next = (v + h × (k × (q − x) + F)) / (1 + c × h + k × h²)
x_next = x + h × v_next
```

This is a point attraction system. It has no built-in collision, rigid-body, or fluid solver. Particle Targets assigns targets before simulation; it does not follow an animated target shape after Solver start.

## Complete built-in node list

All 88 registered node types are listed below. Each ID has the prefix `fold.motion.`. Names are current registry labels. These are Motion authoring nodes, not compositor nodes.

### Content, text, and layout

| Node | ID suffix | Function |
| --- | --- | --- |
| Rectangle | `rectangle` | Create a rectangle from width, height, and corner radius. Its origin is the top-left corner. |
| Ellipse | `ellipse` | Create a four-cubic Bézier approximation of an ellipse from width and height. |
| Path | `path` | Store editable path segments and named attribute stops along the path. |
| Text | `text` | Shape a uniform string with an embedded font, size, line spacing, and horizontal alignment. |
| Text Select | `text_select` | Select a newline-separated entry or Unicode scalar character by index; wrap or clamp. This is not grapheme selection. |
| Format Text | `format_text` | Format scalar fields with `{value}`, `{x}`, and `{y}`; use 0–6 decimal places. |
| Text on Points | `text_on_points` | Shape a text field at each selected point. Labels inherit point transforms and require an embedded font. |
| Fit Background | `fit_background` | Create a padded rounded rectangle around input geometry. Uses path/control-point bounds; returns the background only. |
| Align Content | `align_content` | Align the whole input's analytic outline bounds to a target and normalized anchor. Recompute after text or size changes. |
| Fit Content | `fit_content` | Uniformly fit analytic outline bounds inside a box, with anchor placement. |
| Format | `format` | Output logical canvas size, center, and scene units per output pixel. |
| Anchor | `anchor` | Calculate a canvas-relative position with percentage offsets. |
| Image Reference | `composition` | Place a referenced document video output. Source time is parent time minus start plus source start; outside source duration is transparent. |

Text uses Rustybuzz and Unicode bidi with embedded fonts. There is no implicit system-font fallback. Horizontal text alignment uses advance width. Align Content and Fit Content use outlines and exclude stroke, masks, and effects; they preserve child identity. Empty geometry passes through.

### Distribution and geometry

| Node | ID suffix | Function |
| --- | --- | --- |
| Linear Points | `linear_points` | Place `N` points at `i × spacing`. |
| Line Points | `line_points` | Place evenly spaced points between two endpoints. |
| Grid Points | `grid_points` | Create a row-major grid from counts, spacing, and origin. |
| Radial Points | `radial_points` | Place points around a center and radius; set start angle, sweep, and orientation. A full circle avoids a duplicate endpoint. |
| Scatter Points | `scatter_points` | Create seeded points inside a rectangle or on its perimeter, with independent-axis jitter. |
| Path Points | `path_points` | Distribute points along a path; add travel, tangent orientation, normal wave, scale pulse, and sampled path attributes. |
| Path Motion | `path_motion` | Copy content along a path with per-copy time delay, speed, wave, pulse, orientation, and attribute transfer. |
| Shape Points | `shape_points` | Sample filled paths/text on a grid and return flat points. Apply transforms and respect holes. IDs derive from grid coordinates. |
| Shape Grid | `shape_grid` | Convert filled geometry into square or circular cell subpaths. Optional world alignment shares one lattice. Does not return points or instances. |
| Copy to Points | `instance_on_points` | Place shared source content at each point; preserve point transforms and attributes on the copies. |
| Realize Instances | `realize_instances` | Replace shared instances with explicit nested geometry, retaining transforms and appearance. |
| Convex Sweep | `convex_sweep` | Form the convex hull of each profile and its translated copy. Requires uniform profile style; holes and concavity are lost. |

### Values, context, and influence

| Node | ID suffix | Function |
| --- | --- | --- |
| Value | `value` | Supply or pass through a value of the selected type. |
| Combine XY | `combine_xy` | Combine two scalar fields into a 2D vector. |
| Separate XY | `separate_xy` | Split a vector field into X and Y. |
| Direction | `direction` | Return `length × [cos(angle), sin(angle)]`, with angle in degrees. |
| Math | `math` | Add, subtract, multiply, divide, Euclidean modulo, min, max, absolute, negate, round, floor, ceil, or truncate. Numeric vector/color operations are component-wise. |
| Compare | `compare` | Compare scalar fields: `<`, `≤`, `=`, `>`, `≥`, or `≠`; return Bool. |
| Map Range | `map_range` | Remap a scalar interval, with optional clamp and smoothstep. |
| Mix | `mix` | Blend matching numeric types with a scalar weight. |
| Random Value | `random_value` | Generate a repeatable scalar range value from node seed, explicit seed, and stable element ID. |
| Position | `position` | Read the consuming element's position in its sampling context. It is not an automatic world-space conversion. |
| Index | `index` | Read zero-based order in the consumer's domain. |
| Element ID | `element_id` | Read stable element identity. |
| Domain Size | `domain_size` | Read the consumer's element count. |
| Copy Progress | `copy_progress` | Return normalized element order from 0 to 1. |
| Palette | `palette` | Blend two colors across element order. |
| Color Ramp | `color_ramp` | Map a scalar to ordered color stops; use linear, smooth, or stepped RGBA interpolation in working space. Hold endpoints outside the stop range. |
| Range Falloff | `range_falloff` | Produce influence from a scalar interval. |
| Radial Falloff | `radial_falloff` | Produce influence from distance to a center. |
| Expression | `expression` | Evaluate a typed expression tree with named inputs, time, index, count, position, and vector construction. Supports sin/cos, floor, abs, negate, and binary arithmetic/min/max. |

### Animation and responses

| Node | ID suffix | Function |
| --- | --- | --- |
| Time | `time` | Return evaluation time in seconds, including a sampled time offset. |
| Keyframes | `keyframes` | Sample scalar, vector, or color animation at exact authored key times. |
| Wave | `wave` | Produce sine, triangle, or square output from an input coordinate. |
| Oscillator | `oscillator` | Produce sine, triangle, square, sawtooth, or custom-curve animation; support duration/BPM, ranges, strength, and copy/channel phase. |
| Stagger | `stagger` | Delay an upstream value by element index. |
| Timing | `timing` | Return range progress, curve value, elapsed seconds, and duration. Does not retime media. |
| Curve | `curve` | Map normalized progress through editable linear or smooth segments; clamp endpoints. |
| Timing Segments | `timing_segments` | Return intro, hold, and outro progress. Compress intro/outro proportionally when total duration is short. |
| Set Position | `set_position` | Add to or replace selected element positions. |
| Set Rotation | `set_rotation` | Add to or replace selected element angles; retain position. |
| Set Scale | `set_scale` | Multiply or replace selected element scale. Replacement rejects a singular transform. |
| Set Color / Opacity | `set_color_opacity` | Replace, add, or multiply fill RGBA on selected elements. |
| Set Opacity | `set_opacity` | Replace, add, or multiply element opacity. |
| Copy Colors | `copy_colors` | Apply a two-color gradient across top-level copies; optionally preserve glyph colors. |
| Apply Copy Color | `apply_copy_color` | Apply a color field per top-level copy; optionally affect shapes only. |

### Appearance, scene, and reuse

| Node | ID suffix | Function |
| --- | --- | --- |
| Fill | `fill` | Apply uniform fill color and nonzero/even-odd fill rule. |
| Stroke | `stroke` | Apply uniform stroke color, width, cap, and join. |
| Transform | `transform` | Wrap whole content in a translation, rotation, scale, and pivot transform. |
| Group | `group` | Combine back and front content in drawing order. |
| Output | `output` | Expose final content; an unconnected output is transparent. |
| Empty Content | `empty` | Return an empty content collection. |
| Object Reference | `object_reference` | Reuse a scene object's output, with local, world, or owner-relative space options. Hidden source objects can still be referenced. |
| Scene Group | `scene_children` | Collect active children of a scene object in drawing order. |
| Published Control | `published_control` | Read a document-level typed override or its default. Supports scalar, vector, color, Bool, and Text. |
| Group Input | `group_input` | Read a named typed input of a reusable group. |
| Group Output | `group_output` | Expose a typed result from a reusable group. |
| Node Group | `group_instance` | Evaluate a reusable group definition with instance input values or links. |

Scene relationships also provide transform parenting; Follow Transform, Copy Position, Copy Rotation, Copy Scale, Aim, and Follow Path constraints; and local/world masks with intersect, add, subtract, inversion, opacity, and feather. These are scene properties, not extra node types. Common layer timing, visibility, transforms, and animation control when and where objects appear.

### Attributes, simulation, and code

| Node | ID suffix | Function |
| --- | --- | --- |
| Read Attribute | `read_attribute` | Read a named scalar/vector/color/Bool attribute by stable ID from supplied geometry; use a fallback if absent. |
| Store Attribute | `store_attribute` | Sample and store a named typed attribute on each top-level element. |
| Transfer Attribute to Color | `transfer_color` | Apply an inherited named color attribute to glyphs or shapes. |
| Solver | `solver` | Run an editable update group at a fixed rational step, with Content or Points state and cached checkpoints. |
| Previous State | `solver_state` | Inside Solver only: expose prior state, delta time, and step number. |
| Geometry History | `history` | Sample upstream content at past exact intervals; combine oldest first with `fade^age` opacity and history attributes. |
| Particle Targets | `particle_targets` | Assign each point a stable seeded target inside transformed filled paths/text, respecting holes. |
| Particle Step | `particle_step` | Advance flat points with spring attraction, damping, speed variation, acceleration, and seeded sinusoidal turbulence. |
| Rust Wrangle | `wrangle` | Run bounded Rust code on matching top-level elements. Read time/context/channels/attributes; write position, fill color, opacity, and attributes. |

### Images used by Motion

| Node | ID suffix | Function |
| --- | --- | --- |
| Media Texture | `image` | Supply an image/video/document-output collection with source-time offsets and Hold, Loop, Black, or Error range behavior. |
| Image Card | `image_card` | Place one selected source on a sized textured card with fit settings. Source index wraps. |
| Image Cards on Points | `image_cards` | Place cards on points with per-point source selection and media time offset. |
| Textured Grid | `image_grid` | Create a textured mesh; use one image across it or repeat per cell. An offset field deforms vertices. |
| Sample Image | `sample_image` | Produce a deferred GPU image field from working RGB, luminance, or alpha; use bilinear clamped UV sampling and gain/bias. |
| Image Response | `image_response` | Use the GPU image field for displacement, scale, rotation, color, and opacity. Vector shapes use shared raster atlases. Combine response amounts in one node. |
| Image to ASCII | `image_ascii` | Select glyphs on the GPU from a dark-to-light character atlas using image luminance. Requires an embedded font. |
| Image Bake | `image_bake` | Build a background texture cache with mipmaps. Auto retains HDR with Float16; BC7 is lossy SDR. Rebuild stale caches or clear them to use the source. |

## Programmability and reusable assets

**Expression** stores a typed abstract syntax tree. It executes native field functions and has no arbitrary source execution or implicit scalar/vector conversion.

**Rust Wrangle** compiles Rust to `wasm32-unknown-unknown`; Wasmtime executes it on the CPU. The guest has no host imports. Compilation requires `rustc`, the WebAssembly target, and `prlimit`. Cargo dependencies, build scripts, and source includes are unavailable. Runtime fuel and memory are bounded. Source and controls are saved; compiled artifacts are disposable. A Wrangle changes existing elements; it does not provide arbitrary geometry creation or shared hidden simulation state. Use attributes plus Solver for persistent state.

**Live Rust assets** execute at author time. The bounded Rust SDK returns a construction that becomes retained native groups, nodes, links, and expressions. Those native nodes evaluate each frame; the builder does not rerun each frame. Assets can include runtime Wrangles when needed. Stable interface keys retain compatible artist values, connections, and animation across source updates. Incompatible changes require explicit migration; conflicting internal edits are reported. Bundles include native groups, source, and fonts; external media and document inputs remain explicit dependencies.

Reusable groups expose typed ports, defaults, units, sections, descriptions, and optional control ranges. Inspector ranges guide editing; they do not clamp evaluation automatically. Direct tools and public MCP/CLI authoring create the same editable native construction and commit through transactions. The built-in registry is static; runtime dynamic-library node loading is not supported.

## Bounds and current limits

| Area | Implemented bound or limit |
| --- | --- |
| Work | Default field budget: 2,000,000 work units per evaluator; cancellation is checked. |
| Distribution | Most point generators: 16,384 points. Path Points and Path Motion: 1,024. Shape Points: 65,536 candidate cells and 16,384 occupied points. |
| Nesting | Reusable group evaluation: 32 levels. Many geometry traversals: 64. Realization: 16,384 visited elements. |
| Simulation | Step at least 1 ms; request at most 100,000 steps. Default checkpoint cache: 64 MiB, at most 128 entries. Nested temporal Solvers are unsupported. Other state/work limits also apply. |
| History / attributes | At most 128 history samples; interval at least 1 ms. At most 64 stored attributes per element. |
| Curves | Keyframe track: at most 18,000 keys. Normalized Curve: 2–64 points. Color Ramp: 1–1,024 stops. |
| Expression | At most 512 terms and depth 32. |
| Wrangle | Source at most 32 KiB; 64 channels; 16 MiB guest memory; at most 100,000 fuel per element and 10,000,000 per node evaluation. |
| Rust asset | Source at most 64 KiB; construction at most 1,024 nodes and 64 inputs; output at most 1 MiB. |
| Image geometry | Textured Grid: at most 128 × 128 cells. ASCII: at most 16,384 cells and 64 ramp characters. Image Bake longest edge: 16–4,096 pixels, multiple of 4. |

There is no built-in 3D geometry/lighting system, text-to-path conversion node, path offset node, or path trim node in this registry. Path travel requires one path with one continuous contour; shaped Text is not accepted as a path target. Shape sampling ignores stroke paint. Broader text-script/fallback coverage is not certified. Image-driven bounds require rendered observation. Arbitrary GPU field execution and a general fused field compiler are not implemented. A scene's frame rate depends on its geometry, fields, simulation replay, media, and hardware; node support does not guarantee real-time playback.

## Source basis

Checked against the current implementation, not planned features:

- [Registry](../crates/motion/src/graph/registry.rs), [node implementations](../crates/motion/src/nodes), and [evaluation](../crates/motion/src/evaluation).
- [Geometry](../crates/motion/src/geometry), [fields](../crates/motion/src/fields), [animation](../crates/animation/src), and [simulation](../crates/motion/src/simulation).
- [Rust Wrangle](../crates/motion/src/wrangle), [Rust assets](../crates/motion/src/program), and [scene relationships](../crates/motion/src/scene/relationships.rs).
- [Product specification, sections 11–13](Fold_Application_Product_Architecture.md), [implementation ownership](architecture.md), and [status and known gaps](../dev-plan.md).
