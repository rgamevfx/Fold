# C7 — GPU execution and vector retirement

9 October 2026. Base: C6 `ecf016f` (C5 `2e540bf`). Initial working tree was clean.
Reused C0's inventory and C6's implementation/native evidence.
**C7 is complete by user acceptance, with a recorded qualification exception.**
After review of the inherited spatial extrusion-depth failure below, the user
requested completion and a commit. That specific failure remains deferred; it is
not a passing test. C8 maintenance work may proceed. M4 and physical artist
acceptance remain open.

## Implementation

- `gpu/request.rs` owns validation, dependency/fusion planning and leased source
  preparation. `gpu/encoding.rs` owns image operation encoding and its parameter
  ABI, scratch/immutable inputs and upload leases. `gpu/submission.rs` owns
  admission, submission, completion publication and source-cache publication.
  `gpu/frame.rs` owns immutable frames, explicit GPU leases, readiness, metadata
  and worker-only CPU readback. `gpu/statistics.rs` owns counters/aggregation.
  `gpu/mod.rs` retains renderer construction and entry points, reduced from 1,391
  to 302 lines. Existing focused video preparation and kernel modules remain.
- Prepared/encoded requests transfer actual owned resources between these stages.
  Host budgets, per-kind accounting, source retention, upload lifetimes, immutable
  snapshots, explicit CPU reference evaluation and image operators are retained.
  Admission still orders submission, not source preparation. A fresh cancellation
  and host-health check follows admission wait immediately before submission.
- [The exhaustive dispatch inventory](vector-inputs.md) maps every supported
  old-vector input class and its qualification. Precision-sensitive triangles use
  the shared f64 transform/viewport clip before entering the retained instance
  hierarchy. One GPU preparation/visibility/coverage pipeline handles all paints.
  Deleted the old Rust execution path and WGSL, its ownership/dispatch/clear
  branches, obsolete counters and consumers. Removed obsolete GPU paint-record
  bounds/layout while retaining independently required CPU binning/reference code.
- A new real-device-destruction test exposed a completion callback race: a queued
  successful map callback could read a buffer already destroyed with the device.
  Callbacks now record Pending → Mapped; readiness decodes status after callback
  delivery, checks the shared failure state and transitions to Complete. Host
  polling/submission and status decoding share a short synchronization gate.
  The reservation lives through decoding or abandonment; callbacks do not retain
  the Host. The native regression failed before this fix and passes afterward.
  This covers explicit device destruction, not physical driver-reset injection.
- C4 provider ownership, C5's sole Session coordinator/generation/cancellation/
  admission rules, and C6 stable-panel runtime ownership are unchanged. No UI,
  public authoring contract, schema, dependency, compatibility wrapper, fallback
  framework, C8 optimization or M6 work was added. Removing an internal Rust
  statistics field requires no public discovery/client/Fold skill change.

## Verification

Commands use offline/locked dependencies, two build jobs, development debug info
and incremental compilation disabled. Native test processes run serially outside
the sandbox on **NVIDIA GeForce GTX 1070, NVIDIA 580.173.02, Vulkan**. OCIO root:
`/home/rgame/Documents/fold-native-pipeline/release-package`. The sandbox exposes
llvmpipe; its initial attempted run is retained but is not hardware evidence.

- [Affected Render/App all-target check](check-final.log),
  [headless Render all-target check](headless-check.log),
  [final Render all-target Clippy](clippy-final.log),
  [boundaries](boundaries.log), and [provider ownership](ownership.log) pass.
  Clippy retains unrelated warnings in Media and Render `baked.rs`/`still.rs`;
  no warning-free claim is made.
- [Focused reference and behavior run](focused-final.log): **65 passed**, zero
  failures, 33 native/specialized tests ignored in that run. It covers Render
  unit tests, channels, compositor operations, graph validation, region rendering
  and sampling. This run preceded completion hardening; the final changed GPU
  completion path is verified by the final hardware replay below. No unchanged
  broad workspace suite was repeated: C7 is a native-check checkpoint, while the
  plan reserves broad maintenance checkpoints for C1/C4/C6/C9. No Motion phase
  completion/vertical-slice claim is made.
- [Final hardware replay commands/status](qualified-results.json): **24 passed,
  one inherited failure**. Eight [vector tests](qualified-vector.log), three
  [lifecycle tests](qualified-lifetimes.log), three [image tests](qualified-image.log),
  one [adapter test](qualified-adapters.log), two [region tests](qualified-regions.log),
  and seven of eight [spatial tests](qualified-spatial.log) pass after the final
  completion change. All three lifecycle regressions verify return to wholly
  reusable pooled images and release of buffer reservations. They cover abandoned
  encoding, in-flight cancellation, device destruction and export admission behind
  a held preview permit; retained-vector tests also cover abandoned encoders,
  cache pressure and subsequent reuse.
- Earlier native execution additionally passed **19 distinct tests**: eight
  cache/precision/presentation/YUV tests in [hardware unit log](native-unit-hardware.log),
  two [camera-plane tests](native-camera_plane_gpu.log), two
  [color tests](native-gpu_color.log), one [fusion test](native-gpu_fusion.log),
  one [precision integration](native-precision_gpu.log), three
  [sampling tests](native-sampling.log), and two [text tests](native-text_gpu.log).
  These precede completion hardening; their unchanged kernels were not rerun.
  They are distinct from the final 24 passes, not repetitions counted as new tests.
- Extreme retained vectors agree with the explicit CPU reference at <1e-4 per
  channel across five affine cases, both fill rules and all nine cap/join pairs,
  plus curved/holed/invisible cases. Existing image tolerance is <3e-6; text/fill
  fixtures retain 2e-3. Spatial checks retain exact alpha, existing half-float/color
  tolerances, depth/binding and immutable-frame checks. No existing tolerance was
  relaxed. Preparation-limit and below-dispatch exploratory cases are explicitly
  recorded in the input inventory rather than passed off as supported old-path
  qualification.

Iteration logs include compile fixes and red native regressions. The old optional
packet return required removing a redundant test `unwrap`; the new extreme matrix
initially counted offscreen strokes and tried scales outside the existing
subdivision budget. Those fixture errors were corrected. The stale adapter test
expected two CPU source uploads even on warm requests. The
[C6 executable reproduces that assertion](c6-adapters-control.log); the test now
checks two cold adapters, two warm retained sources and unchanged pixels. This
corrects the test's accounting expectation, not rendering or pixel tolerance.

## Open spatial gate and preserved incidents

`retained_text_mesh_and_extrusion_match_vector_interiors_and_depth` passes its
planar text interior/alpha checks, then fails the extrusion ray-depth oracle at
pixel `(71,50)`: **4.015864849090576 vs 4.01612410445728**, exceeding the existing
`2e-4` tolerance. [The retained C6 checkpoint executable](c6-spatial-control.log)
fails at the identical pixel with identical values; its provenance is the C6
`final-workspace-tests.log` executable path. Hashes are recorded in
[review evidence](review.json). The spatial implementation, ray oracle and
threshold are unchanged. The failure predates C7; its cause and resolution remain
open. The full native fixture suite did not pass. The user accepted this specific
exception for C7 maintenance completion; it does not close the spatial defect or
any Motion acceptance gate.

C6's causally proven empty-layout restore SIGSEGV remains fixed. This work does
not close or conflate the **original unknown-trigger freeze**, **C0's separate
unbound shutdown SIGSEGV**, or **physical artist acceptance**. The new device-loss
regression has its own causal evidence and is not evidence about those incidents.
All remaining M3–M5/M5a–M5c gates stay open; M6 remains gated. See the
[Motion continuation](../../../rewrite/evidence/c7-rendering.md).

## Final review

Reviewed the ownership split, moved operation branches, clipping semantics,
resource release, consumers, deletions and documentation. No duplicate execution
kernel, compatibility shim or unrelated implementation was introduced. Shared
image/CPU/reference paths remain. Formatting, whitespace and deleted-symbol
searches pass; manifests and lockfile are unchanged. The user authorized committing this work after accepting the recorded exception.
