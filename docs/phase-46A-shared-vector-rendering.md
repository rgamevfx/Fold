# Phase 46A: shared-geometry vector rendering

User-authorized follow-up to the saved `Fold-Triangle-Error.fold` reproduction. The renderer consumes existing immutable shared paths and instance transforms; authored Motion graphs, project archives, IDs, rational time, animation, undo, masks and group isolation are unchanged. Field evaluation, text shaping and local path tessellation remain CPU work. This implements the rendering portion of the phase-46 proposal, not GPU execution of arbitrary Rust or all Motion nodes.

## Reproduction

The saved project is 960×540, 24 fps, 240 frames, Motion document `7b0ef399-38ee-4404-90df-23c34509fa06`. SHA-256: `e1859a09c6848422114b63da4a5a30837ff9b63999d00536d27c656ba2d748cd`. It is a user file, not a checked-in fixture, and was not modified. Portable synthetic regressions cover the same repeated curved/stroked geometry failure.

At 2.5 seconds, the previous renderer flattened the nested duplicators into 1,560 drawings and rejected aggregate triangle 262,145. The stroke being visited had only 462 local triangles. Reducing the inner count from 52 to 1 in a temporary copy removed the failure. The original project's first eight consecutive frames rendered on the old renderer; frame 8 (one third of a second) also exceeded the old budget.

## Implementation

- `vector_instances` builds a bounded, cached local triangle hierarchy keyed by the existing tessellation result. Source identity includes path, fill rule, stroke shape and scale-dependent tessellation tolerance. Color and translation edits reuse geometry. Source lookup within a frame uses pointer identities protected by Arc/Weak ownership, never serialized addresses or unstable authored IDs. GPU atlas keys use weak handles so retained GPU buffers cannot pin evicted CPU hierarchies.
- The GPU retains source triangle/hierarchy buffers. Each frame uploads compact split-precision transforms, straight working-space colors, source ranges and preparation jobs. Animated instance data does not require another geometry upload when the source set is unchanged.
- A compute pass transforms triangles and hierarchy bounds into GPU working buffers. A second pass produces tile visibility bitsets in paint order. The coverage pass traverses candidate hierarchies and evaluates the original analytic triangle/pixel intersection. Bit iteration preserves drawing, fill/stroke and alpha order across 32-instance boundaries. Front-to-back premultiplied accumulation stops only when remaining transparency is exactly zero, avoiding hidden opaque copies without an opacity threshold or quality reduction.
- Working geometry is still proportional to executed instance triangles **on the GPU**. This change eliminates CPU triangle expansion and repeated geometry uploads; it does not make overlap, working storage or pixel coverage free. Whole-instance CPU bounds rejection is constant work per copy; per-tile visibility and triangle transforms execute on the GPU.
- Authored f64 transform coefficients are transferred as high/low f32 pairs and combined with compensated products/sums. World bounds beyond ±65,536 retain the existing f64 CPU transform/viewport-clipping preparation and GPU analytic coverage. The profiler counts these fallbacks explicitly. Both paths share one WGSL coverage implementation.
- CPU reference rendering preserves analytic semantics and blends bounded batches at paint boundaries. The original clipping calculation remains the boundary-pixel oracle; exact half-plane classification skips clipping only for fully inside/outside pixels. A deterministic 512,000-pixel comparison tests this fast path against unconditional clipping.
- Source buffers, instance buffers, transformed geometry, visibility storage and in-flight references use the shared GPU host's allocation reservations. Eviction never releases accounting for submitted work still in flight. Cache hits re-encode GPU preparation: an abandoned command encoder must not make a cached packet appear initialized. Low-budget failure evicts retained data and can recover on a subsequent smaller request.

## Budgets and limits

| Resource | Bound |
| --- | ---: |
| Unique source triangles in the instanced scene | 262,144 |
| Expanded instance triangles per vector operation | 4,194,304 |
| Conservative covered instance-pixels per operation | 268,435,456 |
| Visibility bitset allocation | 64 MiB |
| Retained GPU source atlas cache | 32 MiB, up to 8 entries |
| Retained frame packet cache | 32 MiB, up to 8 entries |
| CPU hierarchy cache | 32 MiB, up to 512 entries |
| Local tessellation cache (per preparation path) | 16 MiB, up to 512 entries |
| Retained extreme-coordinate fallback packets | Existing 32 MiB, up to 8 entries |

Drawing/segment/subdivision safeguards remain. Device buffer limits and the existing shared 1 GiB GPU host budget apply before allocation. The CPU oracle keeps bounded 262,144-triangle preparation batches; the extreme-coordinate GPU fallback retains its previous preparation limits. Error messages distinguish unique geometry, instance execution, covered pixels, visibility bytes and device buffer limits. The upper bounds are safety limits, not throughput promises at maximum occupancy.

## Verification and measurements

All **240 frames** of the unchanged project render at native 960×540 and agree with the CPU oracle within **one SDR display code**. Verification used three independent exact-time intervals (0–79, 80–159, 160–239); those concurrent comparison runs are not used for timing. The eight available original-renderer images also differ by at most one code. The 2.5-second frame was visually inspected, including text, rounded badges, outlines, overlap and animated placement. The project hash is unchanged.

Release GTX 1070 / Vulkan / NVIDIA 580.173.02, three matched runs of frames 0–7 (seven warm samples each):

| Measurement | Original | Final |
| --- | ---: | ---: |
| Display-ready mean | 42.924–45.573 ms | 18.202–18.809 ms |
| CPU vector preparation mean | 16.776–19.446 ms | 1.673–1.920 ms |
| GPU graph execution mean | 17.585–20.529 ms | 8.611–9.389 ms |
| Vector uploads per warm frame | 6,466,772 bytes | 189,141 bytes |

This is approximately 2.4× faster display readiness and 34× smaller vector uploads on the matched portion. The original cannot complete the full animation, so no full-animation before/after speed ratio is claimed. The new vector preparation metric includes bind-group/dispatch encoding; the old metric ended before that encoding, making its scope narrower.

Three complete 240-frame runs (239 warm frames each) average **16.372–16.400 ms display-ready**, P95 **24.891–25.643 ms**, maximum **29.617 ms**. Explicit readback raises means to **22.991–23.074 ms**. All measured warm frames fit 33.3 ms, but 104–108 frames per run exceed 16.7 ms; uninterrupted 60 fps is not established. These are headless readiness measurements, not physical desktop scanout or concurrent viewer/export acceptance.

Across each full run: vector preparation averages 0.724–0.731 ms; GPU graph execution 8.292–8.317 ms; uploads average 140,409 bytes/frame, with only 459 bytes/frame of amortized source-geometry uploads. Maximum executed instance triangles are 359,134 while unique triangles peak at 2,095. There are zero CPU image adapters or extreme-coordinate preparation fallbacks. Accounted GPU peak is **53,232,864 bytes (50.77 MiB)** under the unchanged shared 1 GiB host budget; the original matched run peaked at 45.35 MiB. Driver allocations are outside that accounting.

The intermediate shared-geometry implementation already removed the error and uploads, but coverage still averaged about 22 ms GPU time. Exact opaque-coverage termination reduced that to about 8.3 ms. One intermediate full run is retained with the final measurements to make this distinction explicit. Raw records and summaries are in [shared-vector-rendering-results.json](shared-vector-rendering-results.json).

Validation: **29 focused tests** pass (20 vector/unit/native checks, one native nested-source/resource check, five regional-rendering checks, three application Motion workflow checks), plus **nine CPU-only test executions**. They cover alpha/order across tile words, fill rules, curved strokes, reflections/shear, fractional edges, geometry reuse/invalidation, large repeated geometry, empty/offscreen/singular inputs, cancellation, unsubmitted encoders, cache clearing, budget rejection/recovery, extreme-coordinate fallback, nested compositing and cropped viewers. Release desktop, CLI and profiler builds, touched-file formatting, diff review and package-boundary checks pass. Full workspace tests/Clippy, packaging, physical presentation, manual desktop gestures and device-loss injection remain deferred. The existing desktop process was not restarted.

The initial test-helper run omitted the start timestamp before resolving GPU timing queries and timed out on NVIDIA. Correcting the test helper resolved it; the small software Vulkan run alone did not reveal that harness error. Production evaluation already initializes those timestamps.

## Reproduce

Build the affected renderer, desktop/CLI and saved-project benchmark:

```sh
cargo test -p fold-render --features gpu --release --lib vector -- --include-ignored --test-threads=1
cargo test -p fold-render --features gpu --release --test gpu_adapters -- --ignored --test-threads=1
cargo build -p fold-app --features desktop --bin fold-desktop --bin fold-cli --example motion_profile --release
```

Select the physical GPU environment and confirm the adapter printed by the benchmark. A sandbox without GPU device access selects llvmpipe; it is suitable for some correctness checks, not hardware performance claims.

```sh
FOLD_PROFILE_SOURCE=motion FOLD_PROFILE_STEP=1 target/release/examples/motion_profile /path/to/Fold-Triangle-Error.fold 240 960 540
FOLD_PROFILE_SOURCE=motion FOLD_PROFILE_STEP=1 FOLD_PROFILE_VERIFY=1 target/release/examples/motion_profile /path/to/Fold-Triangle-Error.fold 240 960 540
```

The second command compares every frame against the CPU oracle. Its pauses change GPU clock/load behavior, so its timings are not the throughput measurement. Readback is reported separately from display readiness. No viewer cache, proxy, lower resolution, reduced copy count or baked graph is used. The optional profiler image output is diagnostic and outside the measured render interval.

## Remaining work

Typed native field IR, fusion, GPU field evaluation, richer dependency-aware Motion preparation and public per-stage/fallback diagnostics remain the separate phase-46 follow-ups in `dev-plan.md`. Further coverage/overdraw optimization is justified by GPU stage measurements. Physical presentation cadence, manual desktop interaction, multi-vendor equivalence and device-loss injection require broader acceptance; the headless benchmark does not certify them.
