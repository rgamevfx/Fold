# Phase 19B — measured render efficiency

## Goal and boundaries

Improve throughput, latency and resource efficiency after phase 19A closes the
phase-19 performance checkpoint. Use that accepted implementation and native
evidence as the baseline; do not move unfinished corrective acceptance into this
phase. Preserve full-quality 1080p30, UI P95 below 16.7 ms, warm seeks below
100 ms, independent playback modes, normal audio and fair export progress.

Keep the existing Rust/wgpu architecture, feature-neutral render contracts, exact
time, immutable snapshots, explicit color/alpha semantics and shared resource
ownership. Preview, export and headless evaluation retain the same semantics.
Optimize demonstrated costs incrementally; this is not a renderer rewrite.

## Tasks

1. Profile the accepted two-video/title/audio workload plus representative
   compositor and procedural-motion graphs. Separate cold/warm and uncached/cached
   conditions. Attribute compile/validation, decode, queue wait, CPU preparation,
   GPU execution, synchronization, presentation and delivery costs to requests.
   Record a measurable objective for each candidate before implementation.
2. Reuse graph structure and execution plans where compilation/validation costs
   justify retention. Separate static topology from exact-time parameters and
   temporal dependencies. Key by actual content, provider/operator versions,
   output and relevant quality/color context; unrelated project edits must not
   invalidate reusable work, and changed dependencies must not reuse stale work.
3. Retain selected expensive intermediate results, decoded inputs and geometry
   where measured reuse offsets storage costs. Share equivalent content across
   consumers under aggregate budgets. Keep scene-linear results separate from
   display-transformed previews; export never depends on the presentation cache.
   Use selective eviction and account for referenced/in-flight allocations.
4. Reduce repeated allocation, upload and blocking synchronization on measured
   hot paths. Reuse resources only after ownership and GPU completion permit it.
   Keep bounded work in flight, reserve storage before dispatch, and preserve
   validation before publication, independent cancellation and pressure recovery.
   Build on 19A's preparation/graphics/delivery pipeline rather than duplicating it.
   Its metadata reuse, bounded lookahead, result handoff, redraw pacing and paused
   cache compression are baseline behavior. Profile remaining UI submission tails
   and compression granularity before introducing more background GPU work.
5. Evaluate pass fusion where bandwidth or submission overhead is material.
   Preserve operation order, sampling, effect padding, precision, working-space
   conversions, premultiplied alpha and nonfinite validation. Retain only variants
   that pass image equivalence and improve the named workload; do not create a
   general fusion compiler without demonstrated need.
6. Record the next bottlenecks and their owners. GPU-native encoder input and
   hardware-encoding qualification belong to phase 20, including the verified
   software fallback. Region/tile execution requires measured benefit and a
   separately scoped follow-up covering input regions, padding, temporal effects,
   seams and full-frame equivalence; it is not required for 19B completion.

## Acceptance and verification

- Publish reproducible before/after release measurements on the reference GTX
  1070, identifying fixtures, environment, quality, cache state and instrumentation.
  Include repeated runs and frame-time distributions, not only short averages.
  Report throughput, queue waits, allocation/upload/readback counts and bytes,
  cache reuse, and logical/process-tree/driver/scratch peaks. Explain noise and
  tradeoffs; do not claim every workload improves.
- Demonstrate a measured benefit for retained optimizations and no regression to
  phase-19 acceptance. Record rejected/deferred candidates with evidence rather
  than implementing every candidate regardless of value. If none yields benefit,
  record the outcome and leave the implementation goal open.
- Run focused correctness tests for changed behavior: exact-time and dependency
  invalidation, unrelated-edit reuse, CPU/GPU and preview/export equivalence,
  cancellation, held/in-flight lifetime, eviction and pressure recovery as relevant.
  Include repeated seek/edit and independent viewers alongside pinned export.
- Requalify the native sustained/mixed workload and review-range behavior. Cached
  replay, reduced quality and slow gap-free Every-frame playback remain separate
  evidence, not substitutes for uncached Real-time acceptance.
- At the phase checkpoint run workspace checks/tests, Clippy, package-boundary
  checks and desktop/headless packaging qualification. Report inherited failures
  and unverified native behavior explicitly. Phase 21 retains long-run A/V drift,
  teardown/recovery and broader creative-workflow hardening.

## Status

Complete on the reference GTX 1070. Retained opacity/over fusion has repeated
measured benefit; two consecutive native desktop runs pass the unchanged phase-19
criteria. Checkpoint results, inherited warnings and measurement limits follow.


## Retained optimization

The GPU physical executor fuses a reachable, single-use `Opacity` into its
foreground `Over` consumer. Logical IR, feature compilers and the CPU reference
are unchanged. Branches using the opacity result retain the original passes.
Planning happens after validation on every request, so exact-time values, input
identities, output selection, quality and color changes cannot reuse a stale
plan. Unrelated project edits require no new invalidation policy.

The source image remains leased until the fused consumer, even with intervening
work. The existing scratch pool, aggregate reservation, cancellation and submitted
lease ownership remain authoritative. Both the scaled foreground and final over
result receive nonfinite/alpha validation. No color conversion, sampling, blur,
mask or working-space boundary is crossed. Intermediate precision stays float32.

Each eligible pair eliminates one compute pass, bind group and 80-byte operator
uniform allocation, plus one RGBA32F full-frame write/read pair (66,355,200 logical
bytes at 1080p). This is a bandwidth estimate, not a hardware-counter measurement.
The texture pool already reuses scratch: the optimization does **not** claim one
fewer physical texture allocation for every eliminated pass. Host reservation
peaks decline by 160 bytes per pair because parameter admission reserves two
uniforms per reachable node. `fused_opacity_passes` reports actual use.

## Measurement protocol and results

Before implementation, the objective was fewer full-frame passes and lower GPU
time on the compositor opacity/over workload, preserving image equivalence and
phase-19 acceptance. Profiles use the accepted 19A source tree, with only the
profiling fixture added, then the same fixture with fusion. The unchanged baseline
executable is retained at `/tmp/fold-19b-baseline-profile`.

Reference: NVIDIA GTX 1070, driver 580.173.02, Vulkan, release build, 1920×1080
RGBA32F, pinned OCIO ACEScg. Each of three runs evaluates 180 advancing frames
and 180 repeated frame-zero requests for each workload. Report the first 30
frames separately from the remaining 150. Host/renderer construction and source
inspection precede the timer; “cold” means the initial evaluation window, not
a flushed OS page cache or cold process startup. Repeated requests still execute the
graph: they exercise decoded-input/geometry reuse, not presentation-cache replay.
The procedural-motion fixture animates 64 vector rectangles; the compositor
fixture has five eligible pairs; the two-video fixture has one. The full native
desktop fixture separately covers the actual compiled two-video/title/audio plan.

| Advancing workload | Before GPU mean, ms (3 runs) | After GPU mean, ms (3 runs) | Passes before → after |
|---|---|---|---|
| Compositor | 4.702 / 4.734 / 4.689 | 3.066 / 3.076 / 3.058 | 13 → 8 |
| Procedural motion | 1.239 / 1.248 / 1.251 | 0.956 / 0.936 / 0.937 | 4 → 3 |
| Two video sources | 2.894 / 3.161 / 2.891 | 2.874 / 2.783 / 2.736 | 8 → 7 |

Compositor wall means fall from 10.28–10.55 to 8.70–8.77 ms; motion from
7.03–7.44 to 6.73–7.08 ms. Advancing video wall means are 15.70–17.00 before
and 16.40–16.73 ms after: **no end-to-end video speedup is established**. Source
preparation dominates and varies between runs. Compositor GPU improvement is
approximately 35%; motion approximately 24%. These are named-workload results,
not a universal speedup or a new hardware-support claim. Sampled microprofile GPU
process memory remains 784 MiB and scratch disk 61.6 MiB; there is no material
physical-memory reduction claim.

[Per-run evidence](phase-19B-profiles.json) includes cold/warm mean, median, P95
and maximum timings, transfers, pass counts, reuse counters, logical peaks and
sampled process-tree/driver/scratch peaks. Raw logs remain under
`/tmp/fold-19b-{before,after}-{1,2,3}` with their hashes in that evidence. Some
baseline collection overlapped a focused correctness build; GPU distributions
and repeatability are stronger evidence than small host-time changes. Timestamp
intervals cover the recorded GPU graph, not codec execution. Host intervals are
inclusive, and process sampling can miss brief peaks.

Reproduce with `cargo test -p fold-render --features native-video --release
--offline --test efficiency --no-run`, then run the printed executable with
`--ignored --nocapture --test-threads=1`, outside device isolation. Set
`FOLD_TEST_COLOR_ROOT` to the pinned package, `FOLD_VIDEO_HELPER` to its helper,
`FOLD_VIDEO_DECODER=cuda-native`, and `FOLD_NATIVE_FIXTURE` /
`FOLD_NATIVE_SECOND_FIXTURE` to the phase-19 sustained background/overlay MP4s.
Default `FOLD_PROFILE_FRAMES` is 180. Wrap with `scripts/measure_process.py --gpu`
for process/resource peaks. `scripts/summarize_efficiency.py` summarizes each
raw log independently; it does not combine runs or discard outliers.

## Deferred candidates and remaining costs

- Reusable graph plans: preparation is approximately 0.10–0.13 ms here, including
  validation and input processors. Retention/keying complexity is not justified
  by these profiles; all request values remain freshly validated.
- More input/geometry caching: repeated video preparation is about 0.002 ms and
  repeated vector preparation about 0.012–0.014 ms already. Advancing geometry
  costs about 0.37–0.42 ms and changes with time. No additional result cache was
  added without a measured reuse/storage tradeoff.
- General pass fusion and intermediate caching: only the measured opacity/over
  pattern is implemented. Preserve independent validation, shared branches,
  color boundaries and sampling. No general fusion compiler is needed.
- Allocation/synchronization pooling: existing texture reuse remains. Eliminated
  passes avoid their small buffer/group allocations. Host recording and status
  setup still contribute several milliseconds; further attribution is required
  before pooling mapped validation/query resources or changing completion waits.
- Background BC7 work remains deferred during playback as in 19A. This phase
  does not increase background GPU submission or change UI redraw pacing.
- Phase 20 owns GPU-native encoder input and bounded render/readback/encode
  overlap. Current delivery still reads back RGBA8 and feeds the software encoder;
  preserving that reference fallback and fair shared-host service is required.
- Region/tile execution is deferred: these profiles process full-frame images,
  and eliminating a redundant full-frame pass already helps without introducing
  region, padding, seam or temporal-dependency contracts.


## Checkpoint verification

- Full workspace/all-feature tests: **236 passed**, 34 opt-in tests ignored.
  The focused release render run, including its native opt-ins except the
  separate profiler, passed **37 checks**. Updated native UI texture/compression
  lifetime and native CLI equivalence tests passed **3 additional checks**.
- Workspace/all-feature `cargo check`, headless application check, package-boundary
  checks and **16 Python tests** pass. Workspace/all-target/all-feature Clippy
  completes with inherited warnings; none point to the new renderer, fusion,
  profile or correctness-test code. Strict renderer-only Clippy with `--no-deps`
  passes. Without that scope restriction, strict Clippy stops on the pre-existing
  `crates/media/src/video_stream.rs:19` type-complexity warning.
- Retain setup failures: initial all-feature compilation lacked `FOLD_VIDEO_ROOT`.
  A subsequent global `LD_LIBRARY_PATH` override made system FFmpeg load private
  libraries and fail three audio-workflow tests with an undefined symbol. The
  final full run sets only the pinned build root and removes the library override;
  all tests pass. An oversubscribed linking run was stopped and resumed with
  `-j 2`, without reducing test scope.
- Reproducible checkpoint command: set `FOLD_VIDEO_ROOT` to the pinned runtime
  built by `scripts/build_video_runtime.py`, unset `LD_LIBRARY_PATH`, then run
  `cargo test -j 2 --workspace --all-features --offline -- --test-threads=1`.
  Native test executables additionally use the documented color/helper/fixture
  paths and run outside GPU device isolation. Raw logs are
  `/tmp/fold-19b-{workspace-clean-env,render-tests,extra-native,clippy,render-clippy-nodeps}.log`.


## Native playback and delivery requalification

Two consecutive full-quality runs pass `scripts/check_shared_probe.py`, without
changes to the 19A protocol or thresholds. Each uses isolated workspace state,
the existing two-video/title/stereo-audio fixture, a 1 GiB engine GPU budget,
256 MiB presentation cache, native decode, ordinary audio, background ingest and
a 150-frame export. Explicit review preparation/replay and warm seeks complete.
[Native evidence](phase-19B-native.json) retains phase resources, queue/cache
counters, request-stage totals, frame distributions, transfers, artifact
hashes and process/driver/scratch measurements.

| Metric | Accepted 19A | 19B run 1 | 19B run 2 |
|---|---|---|---|
| Sustained admissions / seconds | ~30 fps, both passes | 901 / 30.015 | 901 / 30.012 |
| Mixed Real-time admissions / seconds | ~30 fps, both passes | 368 / 12.271 | 374 / 12.474 |
| Mixed UI P95, ms | 15.11–15.91 | 15.077 | 16.123 |
| Mixed Real-time admission gaps | 0 / 1 | 0 | 1 |
| Mixed Every-frame admissions / gaps | consecutive | 152 / 0 | 189 / 0 |
| Maximum warm seek, ms | <25 | 17.841 | 26.525 |
| Observed audio underruns | 0 | 0 | 0 |
| 150-frame export, seconds | 12.30–12.84 | 12.271 | 12.474 |
| Mixed logical GPU peak, MiB | 727.1 | 727.1 | 727.1 |
| Sampled process-tree RSS, bytes | 1,593,094,144–1,599,447,040 | 1,888,751,616 | 1,601,957,888 |
| Sampled GPU process bytes | 2,139,095,040 | 2,141,192,192 | 2,139,095,040 |
| Sampled scratch bytes | 102,657,619–108,382,880 | 108,382,880 | 108,380,341 |

These preserve acceptance (UI P95 <16.7 ms, warm seeks <100 ms); individual
latencies fluctuate and are not all improvements. The first new process RSS
peak is higher than 19A, while the second returns near baseline. Its cause was
not isolated; do not attribute a physical-memory saving to fusion. Logical
budgets, queue peak two, zero pressure retries in the desktop probe, held-frame
protection and explicit allocation-pressure/recovery tests remain satisfied.
Process/driver figures overlap and must not be added to logical allocations.

The actual compiled workload uses two fusions per evaluated frame, reducing
recorded preview/export compute passes from 13 to 11. Each 150-frame export
retains 1,244,160,000 bytes of RGBA8 readback and 933,120,000 bytes of native
GPU-local source copies; preview has no pixel readback or full-frame CPU adapter.
The next delivery bottleneck is still the phase-20 encoder boundary, not a
second scheduler. Queue waits and decode/preparation costs remain explicit in
the evidence rather than being relabelled GPU execution.

Reproduce desktop qualification by building `fold-app` with `native-probe` in
release, setting `FOLD_SHARED_PROBE` to a fresh directory's `report.json`,
`XDG_STATE_HOME` to that directory's `state`, and the same pinned color/helper
paths plus `FOLD_RENDER_BACKEND=gpu` / `FOLD_VIDEO_DECODER=cuda-native`. Launch
`fold-desktop --project <copy-of-phase-19-fixture.fold>` through
`scripts/measure_process.py --gpu`, then check the report with
`scripts/check_shared_probe.py`. Raw runs are `/tmp/fold-19b-native-{1,2}`;
the measured executable is `/tmp/fold-19b-probe-desktop`.

The relocated production package `/tmp/fold-19b-package` contains rebuilt desktop,
CLI and helper binaries plus the hash-verified pinned runtime; all **45 file
hashes** verify. Without resource/helper path overrides, full-quality 1920×1080
CPU and native GPU renders at exact time 1/1 differ by at most **one SDR code**.
The production desktop remains running through an eight-second launch check;
the test then terminates it. Native probe shutdown succeeds, but the launch check
is not graceful-shutdown certification. This is same-host packaging evidence,
not a clean-machine or wider hardware-support claim. Physical scanout/DPI,
ten-minute A/V drift and broader teardown/recovery remain unverified here and
retain their phase-21 ownership. No UI changes were needed.
