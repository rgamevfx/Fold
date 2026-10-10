# Compositor Read / Write / Shuffle — 10 October 2026

Implemented Read v2, Shuffle v2 and separate Write v1. Output retains its internal document role. Superseded operator versions have no executable compatibility path; unsupported payloads remain opaque through project handling.

Read has compact source/range/frame-mode controls, independent before/after policies, missing-frame policy, input color interpretation, raw data, EXR alpha interpretation and optional proxy selection. Exact rational time and interpretation are shared between preview and delivery. Alpha changes participate in the GPU upload cache identity. Proxy selection is explicit and also affects export.

Shuffle uses primary B and optional A. Two input/output rows, colored routing wires and per-output 0/1 controls replace the previous inspector. Additional channels remain available in a collapsed section. Unconnected inputs expose the standard RGBA layout for authoring.

Write renders only its connected branch using `write/<node-id>` from a committed snapshot through the existing export worker. Destination, frame range/step, RGBA/RGB/alpha, half/float precision, EXR compression, output color space, premultiplication and folder creation are persisted. Each file is published atomically without replacing existing files. Output selection and project state remain unchanged. Jobs use the shared GPU host when present and the existing CPU evaluator otherwise.

## Verification

- `cargo check -p fold-app --features desktop --all-targets` passed.
- Compositor library and channel tests passed (13 library tests, 12 channel tests; native capture test normally ignored).
- `compositor_write` passed: distinct Write/Output branches, exact exported pixels, frame step, float precision, straight alpha, scalar-alpha output, overwrite refusal and unchanged snapshot revision.
- `exr_sequence_delivery` passed both ordinary sequence/delivery tests; its separately gated ACES test was not run in this pass.
- Explicit GPU execution test passed, including premultiplied/straight/ignored EXR alpha changes and cache reuse compared with CPU results.
- Native ImGui capture test passed at 420 and 280 px inspector widths. Reviewed Read, Write and Shuffle captures for alignment and clipping; no parameter/node tooltips were introduced.

[Read](read-normal.png) · [narrow Read](read-narrow.png) · [Write](write-normal.png) · [narrow Write](write-narrow.png) · [Shuffle](shuffle-normal.png) · [narrow Shuffle](shuffle-narrow.png)

- Release desktop build passed; the new session reports NVIDIA GTX 1070 / shared GPU.
- Regenerated the 24-node gallery and the three affected mixed-scene/background-spheres/Wrangle demos through public discovery → proposal → candidate inspection → Apply → durable save. The demo scripts also completed their frame observations and live Wrangle edit checks. No project payloads were edited directly.

## Limits

Write currently delivers flat OpenEXR, not video or arbitrary multilayer EXR. A persistent render queue is not implemented; Render uses the existing single background export job. Physical artist interaction/undo acceptance is not claimed by image captures. Roto is explicitly deferred. This follow-up does not close a Motion rewrite phase.
