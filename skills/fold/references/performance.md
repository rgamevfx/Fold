# Performance and output equivalence

Use this workflow for slow playback, scaling an instance-heavy effect, comparing native nodes with Wrangles, or verifying an engine optimization. Artwork authoring stays on public MCP/API tools. Repository profiling is a separate engineering task; use the reproducible commands in [phase 46](../../../docs/phase-46-motion-performance.md) when that task is authorized.

## Establish a reproducible baseline

1. Read `state` and `viewers`. Record the project/document, revision and epoch, requested time, dimensions, color/display settings, instance count, and viewer stale/error status. Confirm the actual running binary/build and render backend through available diagnostics before attributing slowness to a debug build. A filename or installed release binary alone does not identify the running process.
2. Preserve the current project and user edits. Capture representative frames before a performance edit. For an engine rebuild, retain the saved baseline, then rediscover the session after relaunch. A missing socket calls for session discovery and process inspection; it does not prove a crash or authorize discarding edits.
3. Choose a measurable target: for example 33.3 ms display-ready at the requested resolution for 30 fps. Measure the intended element count, not just the small construction probe. Record cold source compilation separately from warm, distinct-time evaluations.
4. Pause the relevant viewer through `navigate` when appropriate and run measurements without competing builds, startup, playback or exports. Record concurrent activity if it cannot be isolated. Repeat contaminated runs and retain their explanation with the results.

Completion: a fixed workload and repeatable measurement reproduce the symptom. Public render round-trip time includes IPC, polling, encoding and thumbnail handling; it is not a measurement of evaluator time or physical playback FPS. If public tools cannot expose the required stages, report that limitation and hand off the saved reproducer for engineering profiling.

## Locate the cost before changing the artwork

| Evidence | Next check |
| --- | --- |
| Slow first source evaluation, warm frames fast | Compiler/program preparation; separate cold cost from steady-state cost. |
| CPU generation dominates | Instance counts, per-element setup, repeated channel sampling, allocation and geometry sampling. |
| Vector preparation/uploads dominate | Whether shared geometry is repeatedly flattened or rebuilt; changing instance data versus static source geometry. |
| GPU execution dominates | Resolution, coverage/overdraw, masks and image passes, using stage timings. |
| Readback or API wall time dominates | Compare GPU display readiness with readback/encoding; preserve the distinction in the report. |
| Warm cached frames are fast, new times slow | Measure uncached times; a frame cache does not establish evaluation throughput. |

Change one variable at a time in an isolated copy or benchmark: bypass a mask, substitute an empty Wrangle body, or compare an equivalent native operation. These ablations locate cost; altered images are not evidence of an equivalent optimization. Keep the baseline resolution, element count and controls for the final comparison. If quality or density must change, describe that as an artistic tradeoff explicitly.

Katie's evidence: 3,240 bars originally spent about 119 ms in CPU scene generation while GPU execution was about 9 ms. Numeric guest reuse, buffered value exchange, cached bindings/uniforms and shared scanline edge checks reduced generation to about 10 ms without changing the authored graph. Increasing VRAM or reducing bars would not have addressed the measured cause. The complete measurements and their limits belong in phase 46, not as universal performance promises in artwork recipes.

## Choose the execution model deliberately

Prefer a native operation when it directly expresses the effect. Use an inline Wrangle for custom logic, with native sources, instances, masks and published controls around it. Node count and code brevity do not predict throughput.

In the phase-46 runtime, local numeric Rust using supported channel/attribute calls can automatically reuse one guest within a node evaluation. Each element's numeric inputs and write flags reset. Other Rust retains fresh guest state per element; heap allocation, memory intrinsics, indirect calls and even local helper functions can leave the fast subset. Prefer straightforward local numeric expressions where suitable, and measure the actual source. The runtime selects the path; there is no authored switch to force unsafe reuse. Persistent simulation belongs in explicit Solver state.

Current Wrangles execute on the CPU. Native graph generation from Rust and GPU field/instance evaluation are proposals; discover shipped capabilities in the running catalog before using them. Native nodes also do not imply GPU evaluation. Keep static sources separate from animated drivers to make dependencies clear, but establish cache reuse through evidence rather than assuming that an unchanged mask is cached.

A backend change must preserve numerical behavior. Existing Rust uses f64; converting a `sin(...).fract()` palette hash to f32 can choose different colors. Preserve the original path or validate an explicitly specified numeric model before claiming CPU/GPU equivalence. Stable IDs and rational time must retain their identity and timing semantics.

## Verify identical requests and hand back

These request shapes were exercised in Katie through public MCP. Substitute observed session/document IDs; read the live catalog for the current schema. `fold_render` is asynchronous: poll `fold_jobs`, require a successful completed outcome, then consume the image and metadata.

```json
{"session":"SESSION_UUID","source":{"document":"MOTION_UUID","output":"video"},"time":{"numerator":1,"denominator":2},"dimensions":[1400,788]}
```

The above is a `fold_render` argument object. A completed Katie result reported `original_dimensions: [1400,788]`, `dimensions: [1024,576]`, and `resampling: "nearest diagnostic thumbnail"`. Compare before/after renders with the same original dimensions, exact time and color settings, checking their revisions against the intended baseline/candidate. Compare decoded pixels, not PNG encoding bytes. Rendering directly at 1024×576 is a different rasterization request and can differ at edges from that thumbnail.

For a four-second loop, render at 0, 1/2, 1 and 4 seconds. Require unchanged before/after images at those times for a semantics-preserving optimization, a matching 0/4 boundary, and visible change between different animation times. Validate text and published control edits separately, preserving concurrent user work using the main skill's revision-checked workflow.

Use `fold_viewers` to identify the target viewer, then `fold_navigate` with its observed ID:

```json
{"session":"SESSION_UUID","viewer":4,"source":{"document":"MOTION_UUID","output":"video"},"time":{"numerator":1,"denominator":2}}
```

Finish when the intended viewer has matching requested/displayed source and time, `stale: false`, and no error; report saved/dirty state accurately. `capture` checks displayed artwork, not physical frame cadence. Report warm mean, P95, resolution, sample count, backend, and whether readback is included. A passing mean is not a sustained-frame guarantee: Katie averaged 32.36 ms ready but had 37.24 ms P95, so neither uninterrupted 30 fps nor 60 fps was established.
