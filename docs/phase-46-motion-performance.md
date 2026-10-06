# Phase 46: Wrangle execution and procedural generation performance

Status: CPU/runtime optimizations implemented and verified; optimized release desktop running. The native-graph/GPU stages below are a proposal, not shipped capabilities.

## Reproduction and diagnosis

The reported slowdown is reproducible in the optimized release build with the saved `demos/katie-chromatic-bars.fold`, document `0e7284c5-e13b-4244-b455-f4d71772c5ac`. It contains 3,240 animated bars, a sampled text mask and a background. No artwork or resolution reduction is necessary for the runtime changes.

The initial CPU-only scene benchmark averaged 119.108 ms across seven warm frames and failed a 33.3 ms budget. Removing masks gave 109.198 ms; an empty Wrangle body gave 45.191 ms. These short ablations identify Wrangle evaluation as the principal bottleneck, with static silhouette generation also contributing. A release 1,000-point translation benchmark measured 10.38–10.77 ms in Wrangle versus 0.24–0.29 ms natively.

The initial GTX 1070/Vulkan full-resolution probe (1400×788, eight consecutive frames, excluding frame zero) averaged 159.344 ms including readback. Per-frame records separate scene compilation, CPU vector preparation, GPU execution and display readiness. GPU execution was approximately 8–10 ms; there were zero CPU image adapters. The problem is primarily generation and runtime overhead, rather than CPU rasterization or insufficient VRAM.

Reproduce CPU generation:

```sh
FOLD_PROFILE_MAX_MS=33.3 cargo test -p fold-motion --features ui --release --test performance -- --ignored --nocapture
FOLD_WRANGLE_MAX_MS=3 cargo test -p fold-motion --features ui --release --test wrangle compare_native_and_wrangle_translation -- --ignored --nocapture
```

`FOLD_PROFILE_STAGE=no_mask` and `empty_wrangle` select benchmark-only ablations; they never edit the saved project. Timing gates are opt-in because developer hardware and concurrent workloads differ.

Reproduce the shared render pipeline:

```sh
cargo build -p fold-app --features desktop --example motion_profile --release
FOLD_COLOR_ROOT=/path/to/installed/color-package FOLD_PROFILE_SOURCE=motion FOLD_PROFILE_STEP=1 FOLD_PROFILE_RATE=30 target/release/examples/motion_profile demos/katie-chromatic-bars.fold 8 1400 788
```

Set `FOLD_PROFILE_DOCUMENT` to an explicit document UUID when a project contains multiple matching documents. `FOLD_PROFILE_READY_MAX_MS` gates display-ready mean; `FOLD_PROFILE_IMAGES` writes diagnostic PPMs to an existing directory using exclusive creation. The benchmark excludes viewer frame caching and physical presentation. Explicit readback is measured separately from GPU display readiness. Do not interpret render timings as certified desktop playback FPS.

## Implemented optimizations

- Generated Rust uses a bounded numeric memory exchange rather than a host function call for each component read/write. Exact integer IDs/time are still transferred as integers. No WASI or new host access is provided.
- A conservative Rust AST check identifies element-local numeric programs. Only local expressions, control flow, primitive numeric methods and generated channel/attribute calls qualify. Those programs reuse a guest within one node evaluation; every element receives reset numeric inputs, attribute-write flags and exact context. Guest state is never retained between requests.
- Other Rust, including heap allocation, memory intrinsics, indirect calls and local functions, keeps fresh guest state per element. A shared bounded Wasmtime pool reuses allocations while resetting memory/globals/tables. The pool permits 32 simultaneous guests, at most 16 MiB memory per guest, and retains 64 KiB per memory slot. Pool exhaustion produces an error rather than unbounded allocation.
- Put the exchange data before the linear stack so resetting a resident numeric page does not require clearing the entire stack. Retaining/zeroing 2 MiB per slot was measured and rejected as slower. Stack, fuel, compiler cancellation and memory limits remain enforced.
- Cache compiled binding metadata and source validation with the program, avoiding repeated Rust parsing/code generation on warm evaluation. Reuse numeric scratch storage and cache uniform channel samples within one evaluation; varying/delayed fields retain per-element sampling.
- Shape Grid prepares the edges crossing each row once. Per-cell coverage uses the same ordered cross-product winding test as the previous implementation, including holes, reflection, even-odd fill and exact boundaries. Cancellation and geometry budgets remain bounded.

## Final evidence

Release measurements on the same machine:

| Measurement | Original | Optimized |
| --- | ---: | ---: |
| 1,000-point Wrangle translation, warm CPU | 10.38–10.77 ms | 0.29 ms |
| Same native translation in final run | — | 0.31 ms |
| Katie scene evaluation, seven warm frames | 119.108 ms | 10.025 ms |
| Full-resolution package compilation, matched short GPU runs | 120.186 ms | 8.248 ms |
| Full-resolution display-ready, matched short GPU runs | 145.252 ms | 31.428 ms |
| Full-resolution including readback, matched short GPU runs | 159.344 ms | 43.996 ms |

The longer clean 120-frame run (119 warm frames) averages **32.357 ms display-ready**, **P95 37.240 ms**, maximum 43.326 ms; **45.395 ms including readback**. Average package compilation is 7.962 ms, CPU renderer evaluation 12.646 ms (including 3.681 ms vector preparation), GPU execution 9.077 ms. Peak accounted GPU allocation is 118,301,680 bytes, essentially unchanged. There are no CPU image adapters. The 33.3 ms mean gate passes, but 33/119 frames exceed 33.3 ms: this is not a guarantee of uninterrupted 30 fps, and 60 fps remains a future target. The simple translation result does not establish native-equivalent speed for arbitrary Rust.

An earlier 120-frame run overlapped desktop startup and live verification and averaged 51.502 ms display-ready. It was repeated with the viewer paused and no simultaneous jobs. Both runs, the baseline and matched short run are retained in [raw measurements](katie-motion-performance.json). Cold compilation is excluded from warm measurements (final simple Wrangle cold request: 95.16 ms; Katie CPU-only cold scene: 143.589 ms).

Validation: 42 focused release tests passed (29 Motion library, 10 Wrangle integration, 3 procedural-text). They cover pure-code classification, fallback guest heap/memory isolation, reset attribute write flags, infinite-loop fuel exhaustion, compiler errors/cancellation, exact time and delayed/animated channels, native-render parity, and scanline winding/boundary/cancellation behavior. Both opt-in CPU timing gates pass. Release desktop, CLI and profiling example build successfully; formatting and diff whitespace checks pass. Full workspace tests/Clippy remain deferred.

The optimized desktop reopened the unchanged revision-32 project through the normal application path. Public MCP renders at 0, 0.5, 1 and 4 seconds are pixel-for-pixel identical to the corresponding pre-optimization MCP renders; the four-second loop is unchanged. State reports `dirty: false`; viewer 4 displays Katie at 0.5 seconds with `stale: false` and no error. Native text, all published controls and both saved documents remain intact. No authored graph edits, reduced bar counts or baked frames were used to obtain the speedup.

An initial image comparison used a direct 1024×576 render against MCP's nearest-resampled thumbnail of 1400×788 and differed at edges. Repeating the comparison through identical full-size MCP requests matches exactly; the thumbnail and native-resolution raster are intentionally different sampling paths. Physical display timing, manual control interaction and concurrent viewer/export throughput remain unqualified.

## Proposed native-graph and GPU architecture

The current authored graph is not an executable GPU language. Motion fields contain opaque Rust closures (`fields::Field`), and `geometry::drawings_with` flattens instances into CPU drawing records. The shared vector renderer already computes coverage on the GPU, but it prepares transformed triangles, paints and tile bins on the CPU. Merely replacing a Wrangle with native nodes therefore does not move its arithmetic to the GPU.

Keep code inside the Wrangle inspector, with an explicit graph-generation execution mode. Rust graph-building code should run when its source or structural inputs change and return a bounded, typed native graph. Exposed channels become parameter references rather than baked numbers; time and element-dependent operations remain in the generated graph. Validate types, cycles, capabilities and budgets before publishing a new immutable compiled artifact. Retain the last successful artifact during an unsuccessful draft, preserve undoable source/channel edits, and never serialize runtime caches as authoritative data. Existing element Rust remains a fallback; arbitrary Rust is not promised automatic translation.

Implement this in vertical slices:

1. **Typed native field operations.** Add an inspectable operation representation for constants, parameters, exact-time inputs, IDs, arithmetic, comparisons, selects and explicit attribute reads/writes. Existing supported nodes and graph-building Wrangles lower to the same representation. Keep opaque closures and arbitrary Rust as explicit CPU fallbacks. Add constant folding, uniform hoisting and common-expression reuse; fuse supported per-element operations into one CPU loop before adding GPU execution.
2. **Dependency-aware preparation.** Separate source/topology, time-invariant geometry, per-frame uniform values and per-element data. Cache text shaping, sampled silhouettes and local tessellation by immutable upstream identity, font/resource fingerprint, sampling controls and geometry space. Invalidate only dependents when those inputs change. Animated text/mask inputs must invalidate at their sampled time; never infer staticness from equal consecutive frames. Bound caches and retain explicit leases.
3. **GPU instance batches.** Introduce a feature-neutral render operation for shared local geometry plus transform/paint/attribute buffers. Motion preserves instances when lowering. GPU compute evaluates supported fields into those buffers; vector preparation/binning and drawing consume them without CPU readback. Preserve paint order, masks, isolation, alpha and antialiasing semantics. The existing shared GPU host owns buffers, budgets, submissions and device-loss recovery.
4. **Qualification and tooling.** Report CPU fallback reasons and time by preparation/evaluation/upload/GPU stage through profiling/MCP diagnostics. Measure 3k, 10k and 100k instance workloads with count bounds raised only after memory/work qualification. Cover concurrent viewers/export, seek order, edits during work, cancellation, device loss and cache eviction. Use the same operation semantics for preview, export and CPU reference rendering.

A first useful GPU slice is independent 2D instance transforms/colors/scales over shared geometry—the Katie workload. Topology-changing Rust, arbitrary allocation, unsupported attributes and history-dependent code remain CPU work until separately specified. Start with measurable gates: Katie under 33.3 ms display-ready at 1400×788, then 16.7 ms; record cold compilation separately. These are targets, not promises of achieved FPS.

Numeric semantics must be explicit. Current Rust uses f64, while portable GPU shaders generally use f32. Katie's `sin(...).fract()` palette hash is especially sensitive to precision changes. Do not silently lower it to f32 and change colors. Define supported numeric/rounding/hash operations with CPU/GPU differential tests; retain the f64 CPU fallback for existing code where equivalence cannot be maintained. Preserve stable 64-bit IDs and rational time (for example as integer pairs or CPU-prepared uniforms), rather than converting them implicitly to float.

The practical result would be one artist-facing Wrangle, native graph generation for supported operations, fused CPU/GPU execution selected by capabilities, and bounded Rust execution for custom logic beyond that model. It does not require a second standalone code node or application relaunch for source edits.
