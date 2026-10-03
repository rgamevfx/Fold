# Phase 18A — native media and GPU graphics execution (complete)

The reference implementation replaces the temporary CPU-RGB/CPU-mask interchange
for ACES video and graphics. Phase 18 remains a historical checkpoint; phase 19
still owns shared scheduling, multi-consumer fairness and sustained desktop/audio
playback certification. This document does not claim universal GPU/codec support,
physical scanout measurements, or literal zero-copy decoding.

## Execution and ownership

The explicit Linux `cuda-native` backend now executes:

```
H.264 packets → NVDEC CUDA/NV12 frame
  → bounded GPU-local copy into a dedicated Vulkan export allocation
  → supervised completion acknowledgment and external queue ownership acquire
  → immutable wgpu storage buffer → GPU YUV reconstruction → OCIO
  → float composition → explicit display/export transform
```

No video pixels pass through host memory on this route. The GPU-local copy
releases scarce decoder surfaces instead of keeping them in the presentation
cache. Decoder readiness uses a CUDA stream completion acknowledgment on a
supervised worker, **not external asynchronous GPU semaphores**. Media processing
and waits remain off the UI thread.

- `fold-native-video` isolates CUDA/FFmpeg/Vulkan unsafe implementation. The main
  application links neither FFmpeg nor CUDA. A separately supervised helper owns
  codec contexts; media owns verified source pins, process permits and exact CFR
  request scheduling; render owns GPU allocations and immutable input caches.
- CUDA/Vulkan device UUIDs must match. Actual NV12 format, dimensions, strides,
  progressive/crop/color metadata and real AVFrame PTS are validated. No silent
  software/download fallback occurs when `cuda-native` is explicitly selected.
- Two LRU range helpers preserve both two-video use and independent ranges of one
  source. `[0,60,1,61]` requires two launches/two initial seeks, followed by two
  forward reuses. Backward/distant requests seek from keyframes with exact integer
  PTS qualification. The existing software/download adapters remain available.
- One-shot allocation-bound write tickets/completion receipts prevent accidental
  publication of an unrelated or unacknowledged allocation. The acquire encoder
  tracks the buffer; completion callbacks retain both the buffer and its budget
  reservation. Cache clear, immediate caller drop and explicit buffer destruction
  cannot free an allocation used by submitted work.
- Allocation requirements are reserved before Vulkan allocation. Native inputs
  retain at most `min(64 MiB, host budget / 8)` and 64 entries. Submitted consumers
  independently protect their resources. Fresh copies are never written over
  an in-flight immutable frame. A 30-second demand deadline, cancellation and
  helper failure kill/reap before the target allocation can be released.
- Existing consumers keep immutable source copies through link replacement or
  unlink; new consumers validate their sources. Existing aggregate process,
  source-copy and CPU decode bounds remain in force.

The compatible software and `cuda` download adapters now carry shared native CPU
YUV leases rather than float RGB pipes. They upload native samples, cache immutable
GPU inputs, and reconstruct on GPU. Source-resolution input reuse is independent
of Full/Half/Quarter output dimensions. At small preview resolutions native-source
uploads can exceed the old pre-resized RGB payload; no universal payload advantage
is claimed for that adapter.

## GPU graphics and bounded reuse

The ACES path no longer rasterizes tiny-skia coverage masks on CPU. It prepares
bounded path/glyph geometry and computes triangle/pixel intersection coverage on
GPU, accumulating ordered premultiplied float paint. RGB stays signed/HDR float;
there is no RGBA8 paint-image intermediary.

- Pinned MIT/Apache-2.0 `lyon_tessellation` 1.0.20 handles fill tessellation. Shared
  stroke construction preserves caps, joins and transforms. Curves are flattened
  to a declared 1/256-output-pixel geometry tolerance, not a claim of exact curves.
- Viewport clipping and 16×16 triangle/paint bins bound shader work. CPU geometry
  cache is 16 MiB/512 entries; immutable uploaded packets are 32 MiB/8 entries.
  Color and translation changes reuse eligible local geometry. Repeated packets
  upload zero bytes. External YUV layouts reuse at most sixteen 32-byte uniforms.
- Geometry/tile work has explicit ceilings: 262,144 triangles/paint-tile groups,
  two million triangle-tile references, plus existing drawing/segment bounds.
  Excessive subdivision is rejected before tessellation. Over-budget geometry
  errors explicitly; arbitrary-size batch streaming is not advertised.
- Resource pressure discards expendable input/packet caches, not displayed or
  in-flight leases. Transient uploads are reserved through submission. Driver
  and codec internal allocations are not represented by Fold's logical budget.
- Legacy SDR keeps its existing rasterizer. The ACES CPU correctness path uses
  the same prepared geometry with independent float64 analytic area integration.
  This is an explicit coverage-semantic change from the former 8-bit mask, not
  a claim of bit-identical tiny-skia output at every edge. Winding holes,
  intersections, fractional areas, curves, strokes, clipping, transforms, text,
  signed colors, alpha and reuse are exercised. GPU/reference float tolerance
  is 1e-4 (observed f32/f64 discrepancy about 3.34e-5), not SDR paint quantization.

## Sampling contract

CPU and GPU YUV reconstruction use exact integer center-nearest mapping. Even
outputs resize YUV first; odd outputs reconstruct in native coordinates before
nearest RGB sampling. Independent FFmpeg qualification covers source size and
2:1 even reduction; odd and other ratios use the explicit CPU adapter, not old
swscale center-tie behavior. Tests include odd output, enlargement, 1×1, planar
YUV and padded/interleaved NV12. Signal error is below 1e-5.

The source certification remains narrow: progressive, zero-origin CFR H.264/MOV,
8-bit limited BT.709 4:2:0, square pixels and left chroma. Native NVDEC additionally
requires even 48–4096 width and 16–4096 height, within the existing 4,194,304-pixel
and 18,000-frame limits. Unsupported devices, layouts, runtime versions and
CPU-only use of `cuda-native` fail explicitly. Software remains the portable
default; this phase does not introduce an implicit backend-selection policy.

## Distributable runtime

`scripts/build_video_runtime.py` builds pinned FFmpeg 6.1.1 and nv-codec-headers
12.1.14.0 from verified source archives. GPL, nonfree, version-3, network and
unneeded components are disabled. This is a minimal dynamically linked LGPL-2.1
runtime, **not** the installed GPL-enabled FFmpeg build. The helper checks runtime
version, ABI, configuration and licensing; forcing system GPL libraries is tested
and rejected.

`scripts/package_fold.py` packages the helper, replaceable shared libraries,
original source archives, exact build recipe, configuration manifest and notices.
NVIDIA driver libraries remain system-provided. `resources/licenses/video-runtime.md`
and `lyon-MIT.txt` record the added distribution obligations. This is an engineering
license/compliance review, not a legal opinion. The separate existing FFmpeg
executable remains an OS prerequisite for software, audio and encoding adapters.
Do not globally set `LD_LIBRARY_PATH` to the minimal runtime: it would override
libraries used by the system FFmpeg executable. The helper uses a private RUNPATH.

## Reference-device evidence

Artifacts: `/home/rgame/Documents/fold-native-pipeline`. Reference: GTX 1070 8 GiB,
NVIDIA 580.173.02, Vulkan/wgpu 27, i7-8750H, about 32 GiB RAM, pinned OCIO 2.4.2.
These are whole-path comparisons across implementation changes, not isolated
software-versus-hardware codec rankings.

| Full-resolution two-video/title workload | Phase 18 | Initial CPU-YUV slice | Native GPU route |
|---|---:|---:|---:|
| First frame, ms | 523.3 | 295.9 | 407.0 |
| Subsequent new frames, ms | 154.6–177.3 | 29.5–34.1 | 19.9–22.9 |
| Repeated resident frames, ms | — | 23.4–27.0 | 16.3–18.0 |
| Video/geometry upload payload per new frame | 82.94 MB | ~6.29 MB | ~84.4 KB geometry; zero video pixels |
| Repeated input/geometry upload payload | — | ~6.29 MB | zero |

All normal evaluations report zero image readback, two resident video nodes and
no full-frame CPU adapters. The 48-byte validation/timestamp readback is separate.
Explicit CPU-oracle/export readback is not concealed in these counters. Full-scene
CPU/GPU pre-encode comparison has maximum error **one SDR code**; explicit-output
export succeeds. Small parameter uploads are not pixel-payload measurements.

### Longer uncached qualification

Two 450-frame 1080p30 sources plus a procedural title/stereo audio were evaluated
twice, then exported for 450 frames. The input cache cannot retain this range;
this is **900 evaluations, not 900 presentation-cache hits**.

- First pass: first 675.6 ms; subsequent mean 25.23 ms, P95 27.36 ms, P99 28.13 ms,
  maximum 42.46 ms. Second pass begins with a 528.3 ms backward seek/restart demand;
  subsequent mean 25.35 ms, P95 27.61 ms, P99 29.43 ms, maximum 37.29 ms.
- GPU evaluation is typically about 5.4 ms. Raw input and animated geometry caches
  remain bounded; the evaluation host's logical GPU peak is 256,765,192 bytes
  within its 512 MiB budget. Export uses another host; this is not an aggregate
  whole-process logical peak.
- Process-tree sampled RSS peak: 1,458,487,296 bytes. Sampled source scratch:
  87,801,043 bytes. Complete probe including oracle/export: 33.17 seconds.
- Total driver GPU memory: baseline 563 MiB, peak 1,664 MiB, delta 1,101 MiB.
  These are whole-device samples, not exclusive process accounting; other clients
  may contribute. RSS sums can double-count shared mappings; sampling can miss
  peaks. Codec/driver allocations are deliberately distinct from logical memory.

Statistics distinguish graph preparation, evaluation CPU time, output preparation,
queue submission, native service latency, helper codec-call time, helper
copy/readiness time, GPU-local destination-copy bytes, vector preparation and
uploaded payload. Helper intervals include demux/preroll/API waits and copy
import/map/synchronization/release respectively; they are **not physical NVDEC or
copy-engine timestamps**. Parent service time also includes source verification,
startup, IPC and ownership acquisition. Cache hits do not increment copy/codec
counters. Inclusive/overlapping intervals must not be summed as a critical path.

The long run demonstrates bounded uncached execution on this workload, not
sustained interactive playback, multi-consumer fairness, audio/scanout timing or
absence of latency spikes. Those remain phase 19 acceptance.

## Qualification and remaining platform limits

Closing qualification:

- Serialized workspace checkpoint: **204 passed, 29 ignored**. Hardware cases
  were exercised separately. The initial parallel workspace invocation hit the
  existing Dear ImGui `ContextThreadConflict`; serial execution passed.
- After final delivery/telemetry changes: **52 media/render + 12 application +
  41 UI tests passed** (105 closing focused tests, overlapping the workspace run).
  This includes native CLI backend selection, image/export output, geometry,
  decoder supervision, range reuse, receipt validation, pressure and ownership.
- Still-image CLI originally ignored GPU backend selection. It now shares
  `app::output::OutputRenderer` with video export. The regression checks native
  GPU versus software output at odd dimensions and rejects invalid backend policy.
- Strict Clippy passed for affected production libraries/binaries, including the
  native-probe feature. Broad all-target Clippy remains blocked by existing
  motion/compositor lints and four `drop_non_drop` selector-test lints in UI;
  unrelated files were not changed to hide those findings. Native/media focused
  all-target Clippy, package boundaries, formatting and 12 Python tests passed.
- Final package: `/home/rgame/Documents/fold-native-pipeline/release-package`.
  All **45** recorded package-file hashes verified. `ldd` confirms neither main
  binary links FFmpeg/CUDA. The isolated helper loads the packaged LGPL libraries.
  Packaged native CLI versus CPU output differs by at most **one SDR code**.
  The final package also launched and rendered through the shared-device desktop.
- The final six-frame probe confirms versioned telemetry: two videos copy
  6,220,800 destination bytes GPU-locally per new frame; resident repeats report
  zero codec/copy work and zero pixel/geometry uploads. Subsequent new-frame helper
  codec calls total about 0.26–1.40 ms and copy/readiness about 1.65–2.09 ms;
  whole evaluation is about 22.4–25.1 ms in that closing run. These are inclusive
  host/API intervals, not hardware-engine measurements. CPU/GPU comparison and
  explicit-output export pass again.

An interrupted formatting/package attempt was recovered from the recorded source
and the restored native-plane lease test rerun before the successful final build.
No source loss or failed intermediate package is part of the final artifact.
Detailed logs are `core-closing.log`, `app-closing.log`, `ui-closing.log`,
`workspace-serial.log`, `clippy-production.log`, `release-verification.json`,
`closing-probe.log`, `package-release.log` and `release-desktop.log` in the evidence
directory.

Covered failures include pre/in-flight cancellation, nine repeated timeouts,
nine helper crashes, permit reuse, wrong UUID, missing/replaced/unlinked sources,
exact fractional/distant/reverse seeks, mismatched readiness receipts, allocation
pressure, cache eviction, drop before acquisition completion, teardown and an
explicitly destroyed parent wgpu device. Fake-helper stalls and `Device::destroy`
are not claims of physically injecting a hung NVIDIA driver or a GPU reset.

A packaged shared-device desktop rendered the native scene, played to frame 29,
and recomputed correctly on maximize. Capture evidence shows correct framing,
color composition and title/alpha content at normal and maximized sizes. Desktop
input is intermittent on this Wayland setup; OS dragging, physical DPI transitions
and physical scanout are not certified. Full/Quarter menu changes were not
successfully exercised through desktop input; their sizing/reconstruction paths
are covered by automated tests. No new artist-facing controls were added.

## Reproduction

```sh
python3 scripts/package_fold.py /absolute/external/build /absolute/new-package
# Main application needs no FFmpeg/CUDA library linkage or LD_LIBRARY_PATH change.
FOLD_RENDER_BACKEND=gpu FOLD_VIDEO_DECODER=cuda-native \
  /absolute/new-package/bin/fold-desktop --project /absolute/project.fold

export FOLD_COLOR_ROOT=/absolute/new-package
export FOLD_TEST_COLOR_ROOT="$FOLD_COLOR_ROOT"
export FOLD_VIDEO_HELPER="$FOLD_COLOR_ROOT/bin/fold-video-helper"
# Native ignored tests additionally require FOLD_NATIVE_FIXTURE (>=62 frames)
# and FOLD_NATIVE_SECOND_FIXTURE with a different verified source.
cargo test -p fold-media -p fold-render --features fold-render/native-video \
  -- --include-ignored --test-threads=1
FOLD_RENDER_BACKEND=gpu FOLD_VIDEO_DECODER=cuda-native \
  cargo run -p fold-app --release --features desktop --example execution_probe -- \
  /absolute/background.mp4 /absolute/overlay.mp4 /absolute/new-project.fold 6
```
