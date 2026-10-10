# Roto performance — 10 October 2026

User-directed follow-up to Roto v1. Baseline source: `c3af45e7035f4b3d0f38d31a80a45b2007151109`; changes are in the working tree. This does not close a Motion phase or establish artist acceptance.

## Changes

- Compositor lowers Roto to a feature-neutral scalar coverage program. Compatible consecutive Over shapes share one vector draw. Group opacity, inversion, ordered combine operations and shutter averaging remain explicit.
- Render uses the existing Motion/vector geometry and analytic coverage shaders. A scalar output variant writes R32Float. Mask storage uses four bytes per pixel instead of sixteen; public frame output remains RGBA.
- Vector and scalar operations use conservative bounds from both hard and feather contours. Scalar backgrounds preserve inverted masks outside the bounds. Only the final matte expands to the requested format. Region requests shift paths into their working coordinates.
- Completed, validated GPU mattes are cached by evaluated program and dimensions, independently of changing image inputs. Cache limits are eight entries and the smaller of 64 MiB or one eighth of the host budget. Host pressure clears retained entries. Submitted resources remain leased through completion; incomplete or failed results cannot be cache hits.
- One/two-source channel gathering uses one pass. Native paths use a bounded worker-local cache. Static controls and the current shutter sample avoid repeated evaluation. Reference indexing enumerates properties once per node instead of once per component, preserving existing reference addresses.
- Inspector and viewport share evaluated controls by node, document generation and exact time. Preview, cancellation and document changes invalidate the sample. No visual control redesign is included in this performance change.

## Reproduction and measurements

Debug build, NVIDIA GeForce GTX 1070, Vulkan, driver 580.173.02. `roto_playback_cost` drives actual compositor compilation and GPU evaluation at 1080 × 1080, with changing input color each frame. Eight frames per case; first two discarded. Completion is polled without image readback. The original implementation failed the regression assertion: eight passes where at most two are required for a static matte over a changing background. See [before.log](before.log) and [after.log](after.log).

| Case | Before ready, ms | After ready, ms | Before passes | After passes |
| --- | ---: | ---: | ---: | ---: |
| Bypass | 6.96 | 6.99 | 1 | 1 |
| Static hard | 12.22 | 7.42 | 8 | 2 |
| Static feather | 12.50 | 7.71 | 8 | 2 |
| Animated transform | 13.69 | 13.02 | 8 | 6 |
| Deforming point | Not measured | 22.62 | Not measured | 6 |

Ready times are medians, not GPU execution alone. Final-frame GPU timestamps after the change: bypass 0.115 ms; static masks 0.323 ms; animated transform 3.083 ms; deformation 3.335 ms. Before-change GPU timestamps were not captured. Warm static coverage reports zero rasterized coverage pixels, zero vector preparation and zero mask uploads. The static feather case is approximately 38% faster for render-to-ready in this fixture; animated rendering remains more expensive.

The fixture's median preparation/compile time fell from approximately 19–22 ms to 4–6 ms. This includes new Project construction, document serialization/validation, package registration and compilation on every frame. It must not be presented as isolated production compilation time. Native footage decoding, presentation and a sustained desktop playback session were not measured. No release-build performance claim is made.

## Validation

All Cargo commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`, `--offline -j 2`.

- `cargo test -p fold-compositor --features ui --test roto -- --include-ignored --nocapture --test-threads=1`: five passed on the native GPU. Covers feather parity, every combine mode, opacity/inversion, dimensions/configuration invalidation, warm reuse, region cropping, cancellation, exact animation and playback work counts. Existing feather parity maximum error: 0.000002027. New scalar tests require error below 0.002 and exact interior/outside values within 0.00001.
- `cargo test -p fold-compositor --features ui --lib --test references --test roto`: 20 passed, five native-only cases skipped in this invocation. Includes actual ImGui drag/cancel/commit/undo routing and public reference/animation preservation. See [compositor.log](compositor.log).
- `cargo test -p fold-render --features gpu --test channels --test regions`: ten passed; seven native-only cases skipped. See [render.log](render.log). Roto region GPU parity is covered in the native Roto run above.
- `cargo test -p fold-render --features gpu --lib gpu::vector::tests -- --ignored --test-threads=1`: eight passed on the native GPU, including resource/budget failure recovery and shared vector geometry reuse. See [vector.log](vector.log). The focused channel gather test also passed after correcting the operation-budget count for the reduced pass count; see [channel-final.log](channel-final.log).
- `cargo check -p fold-app --features desktop`: passed, including Motion, Compositor and shared UI/render consumers. See [app-check.log](app-check.log).

Full workspace suites, release profiling, dense production shapes and physical artist acceptance remain open. Existing unused YUV members produce warnings in builds without native video. The existing native parameter reference component limit remains unchanged; this work preserves its address format rather than changing the graph protocol.
