# Further rendering optimization: memory, GPU work and threading

10 October 2026. Research follow-up requested after the compositor audit. No
runtime defaults or renderer behavior changed in this follow-up. The recommendations
below are proposed experiments, not measured Fold speedups or Motion phase acceptance.
See the [primary-source research](gpu-threading-sources.md) for pinned wgpu 27,
FFmpeg 6.1.1, Vulkan and NVIDIA references.

Subsequent implementation is recorded in [implementation and evidence](improvements.md);
the baseline and proposals below describe the research checkpoint.

## Findings and measured limits

The 1 GiB setting is conservative for this 8 GiB GTX 1070. More importantly,
`Application::effective()` implements “automatic” by selecting fixed defaults,
not by inspecting available VRAM. The current video caches have independent hard
caps, so changing the top-level budget cannot make them retain more video.
Sources: [machine preferences](../../../../crates/platform/src/application.rs),
[GPU video cache](../../../../crates/render/src/gpu/native_video.rs),
[software-plane GPU cache](../../../../crates/render/src/gpu/yuv.rs).

The previous paired measurements provide this baseline:

| Optimized workload | Wall P50 | GPU P50 | Decode/preparation interval P50 | Graph compile P50 |
| --- | ---: | ---: | ---: | ---: |
| HEVC source | 10.81 ms | 1.82 ms | 2.36 ms | 0.79 ms |
| HEVC full effects | 20.58 ms | 10.44 ms | 2.65 ms | 0.89 ms |
| ProRes source | 23.76 ms | 3.10 ms | 6.24 ms | 0.92 ms |
| ProRes full effects | 31.43 ms | 10.29 ms | 5.86 ms | 0.89 ms |

Source: [raw summary](summary.json). Intervals are inclusive and medians are not
additive. In particular, subtracting GPU/decode medians from wall time does not
identify a specific CPU function. The profile waits for completion before the
next frame, so these are serial request latencies, not desktop playback throughput.
GPU clocks varied in the earlier experiments; retain that uncertainty. Peak
logical allocation stayed below 230 MiB, without evidence that the 1 GiB cap
limited these short graphs. That does not establish an adequate desktop cache
budget or multi-layer/4K capacity. The shared-source fixture duplicates one read;
it is not a benchmark of both distinct clips composited together.

## Use additional memory deliberately

| Resource | Current policy | Proposed experiment |
| --- | --- | --- |
| Shared GPU accounting | 1 GiB default | Compare 1/2/4 GiB; start machine-specific tuning at 4 GiB if live headroom permits |
| Display GPU cache | 256 MiB | Compare 256/512/1024 MiB, inside shared GPU accounting |
| Each GPU video cache | min(64 MiB, host budget / 8), at most 64 entries | Compare 64/128/256/512 MiB under an aggregate source allowance; reconsider entry cap alongside byte policy |
| Software decoded retention | 64 MiB per decoder, 128 MiB across decoders | Compare larger coordinated CPU retention; GPU cache enlargement alone cannot preserve discarded CPU frame identities |
| Image upload staging | min(128 MiB, host budget / 8) | Size to measured in-flight demand and reuse, not total VRAM |

These are limits, not reservations or additive per-worker entitlements. Avoid
each renderer independently claiming the proposed shared source allowance.
Count outstanding uploads, held frames, display retention and native imported
allocations. CPU pipe/decoded budgets are separate from VRAM and also constrain
parallel preparation. See [media budgets](../../../../crates/media/src/budget.rs).

At 1080p, raw native HEVC storage is about 5.93 MiB/frame and ProRes with alpha
15.82 MiB/frame. Current GPU retention fits roughly 10 and 4 frames respectively.
The ProRes GPU cache is keyed by decoded frame identity; coordinating CPU and GPU
retention, or designing a stable source/sample cache, matters for revisits.

The existing BC7 viewer cache offers a better use of memory for unchanged replay:
1920×1080 BC7 image payload is about 1.98 MiB/frame. A 360-frame clip occupies
about 712 MiB, so a 1 GiB viewer allowance could retain one complete loop once
prepared. This is payload arithmetic, not a measured occupancy guarantee: live
RGBA frames, other viewers, compression work and driver allocation add overhead.
BC7 is already a lossy presentation cache, not scene/export storage. The existing
GPU implementation means no new compression system is needed. Sources:
[viewer cache](../../../../crates/ui/src/preview.rs),
[compression](../../../../crates/render/src/gpu/cache.rs).

Implement real automatic budgeting using device-local heap budget/usage when
available through the GPU adapter boundary. Installed capacity alone is not
available capacity. Vulkan provides dynamic estimates through
[VK_EXT_memory_budget](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_memory_budget.html);
wgpu 27 has no portable high-level equivalent. Keep backend querying and
allocation policy out of Compositor/Timeline/Motion. A 4 GiB machine-specific
experiment is reasonable; a universal hardcoded 4 GiB default is not the design.

## Prioritized implementation candidates

### Measured CPU opportunity: ProRes decoder thread allocation

A new isolated test on the original 360-frame ProRes alpha clip varied input
decoder threads while retaining native `yuva444p12le` output and one filter/output
thread. Three trials used different orderings. The output went to `/dev/null`,
so these results include process startup and raw output but exclude Fold's pipe
consumer, upload, effects and display. OS caches were not flushed, CPU clocks
were not controlled, and no competing Fold build/benchmark was launched.

| Decoder threads | Median elapsed, 360 frames | Derived average throughput |
| --- | ---: | ---: |
| 1 | 16.98 s | 21.2 fps |
| 2 | 9.32 s | 38.6 fps |
| 4 (current cap) | 5.43 s | 66.2 fps |
| 8 | 3.91 s | 92.2 fps |

Eight threads reduced elapsed time by **28.1%** relative to four (39.2% higher
throughput) in this isolated probe. This is evidence to test dynamic decoder
thread allocation, not an application speedup claim. Frame/slice mode remained
FFmpeg's default; mode selection and multi-clip contention were not measured.
The existing limit allows up to six regular media processes, so raising every
decoder to eight unconditionally would permit 48 decoder threads on this
12-logical-CPU machine. Use a shared CPU capacity policy with fewer threads per
decoder under concurrency, then measure seek latency as well as throughput.

Reproduction: [script](thread_probe.py), [all 12 results](thread-probe.json).
The preliminary run requested 16-bit planes rather than the native 12-bit format;
its timings were discarded and the complete matrix rerun with the correct format.

### 1. Reuse video allocations and imports

On every new ProRes frame the GPU ingress creates a new immutable storage buffer
with `create_buffer_init`. The CPU stream also allocates and zero-initializes a
new raw frame buffer before reading. `YuvFrame` then moves that allocation into
an Arc without a second full-frame copy; preserve this existing good behavior.
The generic float-image staging pool already supports completion-safe reuse,
but native video ingress does not use that path. Sources:
[YUV ingress](../../../../crates/render/src/gpu/yuv.rs),
[pipe reader](../../../../crates/media/src/video_stream.rs),
[decoded ownership](../../../../crates/media/src/decoded.rs),
[staging pool](../../../../crates/render/src/gpu/upload.rs).

For HEVC cache misses, Fold allocates exportable Vulkan storage, imports it into
CUDA, copies the decoded surface, synchronizes the stream, and tears down the
CUDA import. Investigate a bounded reusable allocation/import pool with stable
slots. Pool ownership must respect cached frames as well as submitted readers;
two or three slots cannot overwrite a retained frame just because its last GPU
submission completed. Sources:
[native allocation](../../../../crates/native-video/src/gpu.rs),
[helper copy](../../../../crates/native-video/src/helper.rs).

Measure allocation/import/pipe/upload time separately before choosing the pool
size. Keep the GPU-local copy that releases scarce decoder surfaces unless a
replacement ownership design proves better. Removing `cuStreamSynchronize`
without a replacement cross-API completion/visibility handshake is incorrect.
External semaphores are a later, separately qualified interop change.

### 2. Pipeline preparation and use bounded source parallelism

`prepare_video` traverses all required video sources sequentially. The preview
worker has one active request and waits while its result mailbox is occupied.
Software streams have a reader thread, but demand one frame at a time; FFmpeg can
already work ahead until pipe backpressure. Submission itself is asynchronous:
the graphics permit is released after submit, not after GPU completion. Sources:
[source preparation](../../../../crates/render/src/gpu/preparation.rs),
[preview worker](../../../../crates/app/src/preview_worker.rs),
[submission](../../../../crates/render/src/gpu/submission.rs).

Build a bounded source-preparation stage that overlaps frame N+1 with GPU work
for N, and prepares independent sources concurrently where codec ownership permits.
Start with 2–3 outstanding frames for steady playback/export; keep interactive
seek queues shallow and replace stale unsubmitted work immediately. Preserve
exact time, immutable snapshot identity, ordered publication, cancellation,
fairness to export/audio, and completion-owned resource leases. Do not share one
mutable decoder across arbitrary jobs or spin up a decoder per frame.

Profile physical playback first so the change targets real idle intervals.
Parallel command recording is possible with independent wgpu encoders, but is
lower priority for a 5–15-pass graph. Parallelize coarse source/compile work before
adding one task/command buffer per operator. NVIDIA supports this direction and
cautions against tiny submissions and dedicated async-compute overlap on
pre-Ampere hardware such as this Pascal GPU.
[Official Vulkan guidance](https://developer.nvidia.com/blog/vulkan-dos-donts/).

### 3. Reduce repeated graph and per-pass setup

Compositor compilation reads the native document and reconstructs prepared
records on each request. Retain immutable prepared structures by document content,
provider/schema and dependency identity; sample only time-dependent parameters
per frame. Invalidation must include nested source/instance changes, exact local
time, output/color policy and dependency identity. This fits the existing feature
provider boundary; do not move authored feature state into render infrastructure.
Source: [compile path](../../../../crates/compositor/src/compile.rs).

Generic passes also allocate new uniform buffers and bind groups. Test a
completion-safe uniform arena with dynamic offsets and reuse compatible binding
layouts/groups. Maintain independent ranges for in-flight submissions: wgpu queue
writes occur before the next submit, so overwriting a shared range can corrupt
earlier encoded work. The measured compile interval is only about 0.8–0.9 ms on
these small fixtures; this work alone will not explain or remove the full latency.
Sources: [pass encoding](../../../../crates/render/src/gpu/encoding.rs),
[wgpu 27 queue](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu/src/api/queue.rs).

### 4. Fuse GPU work that preserves evaluation semantics

YUV reconstruction, OCIO input/display conversion, image effects, blur and BC7
presentation compression already run on the GPU. The full fixture still uses 15
passes and about 10.3–10.4 ms GPU time. Existing fusion only handles single-use
opacity into Over. Target compatible pointwise chains first: grade, exposure,
opacity, masks and compositing; investigate integrated color validation and output
packing separately. Sources: [fusion](../../../../crates/render/src/gpu/fusion.rs),
[color](../../../../crates/render/src/gpu/color.rs).

Eliminating a materialized 1080p RGBA32F intermediate removes 31.64 MiB of writes
and 31.64 MiB of subsequent reads logically; hardware cache behavior determines
actual DRAM savings. Preserve authored order, intermediate validation, alpha,
HDR and CPU-reference agreement. Don't fuse through transforms or blurs as if
sampling commuted with color/math. Blur already uses separable shared-memory
stripes; measure per-pass costs before claiming it needs that optimization.
Source: [blur shader](../../../../crates/render/src/gpu/filtered_blur.wgsl).

RGBA16F intermediate storage could halve image payload, but is not a transparent
default change: signed HDR range and accumulated precision need explicit policy
and tests. Existing `rgba16` is a storage-boundary conversion, not evidence that
all kernels are qualified at half precision. Defer this behind exact fusion and
resource reuse. Source: [precision boundary](../../../../crates/render/src/gpu/precision.rs).

### 5. Move export conversion and packing to GPU

Alpha export currently reads the scene back as float RGBA, applies output color
on CPU, unpremultiplies and packs RGBA16 on CPU. Add an alpha-preserving GPU output
path that validates and packs the encoder's expected format before readback. For
the existing RGBA16 contract that halves pixel readback from 16 to 8 bytes/pixel
and removes the CPU pixel loop, without promising an end-to-end percentage.
GPU packing directly to native encoder planes could go further only after exact
matrix/range/chroma/alpha qualification. Sources:
[export evaluation](../../../../crates/app/src/output.rs),
[CPU packing](../../../../crates/render/src/lib.rs).

H.264 hardware encoding with GPU-resident input is a separate encoder/backend
integration, not a wgpu toggle. ProRes hardware decode is not available through
NVDEC on this machine; its software path needs CPU/transport improvements.
These export changes do not improve preview, which already has zero pixel readback.
[NVIDIA decoder capability table](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvdec-application-note/index.html).

## Validation and decision order

First capture a real playback/export timeline and extend existing counters for
decode, pipe read, native allocation/import, upload encoding, per-pass GPU time,
cache hits/evictions and outstanding frame count. Reuse existing timers where
present instead of creating competing metrics. Record physical GPU usage and
clocks alongside Fold's logical accounting; no clock settings were changed here.

Then evaluate resource reuse and bounded overlap independently, followed by their
combination. Sweep budgets separately so performance gains can be attributed.
Use both original clips, a composition containing both at once, forward full
duration, repeated loops, reverse and random seeks; include foreground playback
with background retention/export. Report P50/P95/P99 seek latency, sustained fps,
missed deadlines, cancellation latency, CPU usage and memory peaks. The target is
lower end-to-end time and fewer missed frames, not just higher GPU utilization.

For each runtime change retain CPU/GPU color and alpha oracles, invalid-value
checks, device loss/cancellation/pressure tests, source identity and held-resource
tests. Encoded export must be decoded and checked as well as timed. Distinguish
serial headless tests from physical artist playback acceptance. These proposed
steps do not close M6 or any outstanding rewrite gate.
