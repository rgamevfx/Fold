# EXR playback and compositor throughput

Implementation and validation, 2026-10-06. The reproduction is StockX-v1.fold,
saved revision 35, 1920×1080 at 24 fps with 120-frame DWAA sequences. The original
artist graph and media were not modified. This is an execution-path change;
compositor operators, color transforms, alpha and exact source-time policies are
unchanged.

## Execution changes

- EXR decompression and selected-channel unpacking use a shared pool of at most
  six workers. Both queued block scratch and retained decoder scratch are reserved
  before admission. Under memory pressure the window shrinks to sequential work.
  Native-size, matching-window reads return their decoded pixels directly.
- CPU and GPU consumers share verified raw source samples. Keys include the source
  fingerprint, metadata, file-change identity, channel tuple and dimensions.
  Independent tickets prevent one cancelled viewer from cancelling another
  consumer. Source replacement is checked before reuse and after decoding.
- The source queue is bounded to eight pending jobs and prioritizes demand.
  Speculation has a separate admission allowance, at most half the decoded budget.
  Playback prepares the next exact frame from immutable snapshots, respects review
  bounds and loop/pause state, and cancels obsolete work on seeks and edits.
  Ordinary forward playback preserves an in-progress decode for the demanded frame.
- GPU evaluation queues a small source window ahead of consumption. Graphics
  admission covers submission, so source I/O and decode do not hold the graphics
  scheduler permit. This does not change playback clocks or final-result publication.
- Scene-linear source textures are shared across renderers on one GPU host,
  capped at one third of its budget (at most 384 MiB and 64 entries). Expendable
  caches are cleared under pressure; idle pooled textures do not falsely trigger
  source eviction. Held frames and in-flight submissions retain their own leases.
- Mapped upload buffers are reused only after completion permits remapping. Pixels
  copy directly into staging, eliminating an intermediate full-frame byte vector.
  Staging has a separate pool cap and remains charged to the GPU host allocation
  budget. Partial-frame errors and cancellation cannot expose a buffer for reuse
  while its work is still in flight.

CPU worker evaluation also uses the configured aggregate working allowance,
rather than a second fixed 128 MiB limit that rejected this valid three-source
composition. Allocation leases still enforce the shared ceiling; the small
`render()` convenience entry point retains its existing policy.

No production decoder dependency or project schema was added.

## Reproduce

```sh
FOLD_COLOR_ROOT=/path/to/packaged-color-runtime \
  cargo build --offline --release -p fold-app --features desktop \
  --example exr_profile --bin fold-desktop
FOLD_COLOR_ROOT=/path/to/packaged-color-runtime \
  target/release/examples/exr_profile /path/to/StockX-v1.fold 12 \
  f5598496-9686-4a1a-becb-427b7c119357
```

Run each document in a separate process when comparing uncached source work;
otherwise the shared source cache can legitimately reuse inputs from a previously
measured document. The probe reports frame zero, distinct frames 1–11, and a repeat
of frame zero. A repeat is a cache hit only when its `resident_stills` and upload
counters demonstrate it; after a long sequence it may have been evicted.
`FOLD_PROFILE_PIPELINE=1` overlaps preparation of the next frame with current GPU
work. `FOLD_PROFILE_VERIFY=1` compares CPU/GPU display pixels at the first, last and
repeated frame of the measured span, allowing at most one code value. Verification is excluded from the
reported frame time but affects later cache state, so use a separate verification
run. `FOLD_PROFILE_MAX_MS` makes the distinct-frame mean an executable acceptance
threshold.

Timings are headless display-ready wall time, without pixel readback, encoding,
IPC or physical presentation. Shader/source initialization affects the first
frame. OS file caches were warm; “uncached” below means new Fold source/frame
identities, not cold storage. The GPU is an NVIDIA GTX 1070 using native Vulkan,
driver 580.173.02, on an Intel i7-8750H (six cores / twelve threads). Performance runs are serialized with builds/tests stopped.

## Measurements

Final measurements and raw samples are recorded in
[exr-playback-results.json](exr-playback-results.json). The baseline has only two
distinct-frame samples per document and is therefore not a sustained-playback
qualification. Optimized runs use eleven distinct-frame samples per document, plus 119
new-frame samples for the combined sequence with and without preparation.

| Workload | Baseline mean ms (2 frames) | Optimized mean ms | Optimized P95 ms | New-frame samples |
| --- | ---: | ---: | ---: | ---: |
| Gucci single-part | 152.84 | 70.36 | 92.89 | 11 |
| Multipart source 1 | 154.92 | 57.02 | 68.03 | 11 |
| Multipart source 2 | 156.65 | 56.50 | 63.48 | 11 |
| Multipart source 3 | 152.40 | 58.74 | 66.92 | 11 |
| Combined, short run | 383.46 | 151.28 | 183.13 | 11 |
| Combined, 120-frame run | 383.46 | 146.24 | 168.20 | 119 |
| Combined, 120 frames with source preparation | 383.46 | 117.69 | 140.85 | 119 |

The long-run improvement is about 2.6× without look-ahead and 3.3× with it.
Both long runs allocate four upload buffers and reuse them 360 times. GPU logical
allocation peaks at 672.37 MiB within the 1 GiB host budget. With preparation,
sampled decoded storage peaks at 158.20 MiB within 256 MiB, and sampled CPU
working storage at 217.00 MiB within 512 MiB; those CPU values are observations
at frame boundaries, not allocator peak measurements.
Separate three-frame runs demonstrate resident-cache hits: **7.55 ms** for the
single-part source and **15.74 ms** for the combined composition, with zero pixel
uploads. These are individual cached samples, not new-frame throughput.

The preparation probe overlaps decoding and consumption of individual sources;
a whole-graph wait before evaluation would serialize uploads and misrepresent
this path. These numbers do not establish physical desktop FPS.

The original fresh single-source frames took 143–162 ms and combined three-source
frames 371–396 ms. Baseline source-cache hits took about 7–8 ms. Decode dominated;
the original combined GPU graph took about 12 ms. The new GPU timestamp interval
also includes mapped staging transfers, so its GPU milliseconds are not directly
comparable to the old interval as a shader-only regression metric.

## Verification and remaining work

36 focused tests pass, including native GPU checks, and the release desktop
build, package boundaries, touched-file formatting and diff review pass. All four
single-source StockX documents match CPU/GPU display within one code at frames
0 and 2 and on repeated frame 0.

**Full-project CPU/GPU equivalence remains open.** Enabling the combined CPU
reference exposed its former 128 MiB cap; after correcting that cap, the combined
image differs by up to 82 display codes. An intermediate-output trace locates the
first divergence at node 9, a nearest-neighbor Transform (0.7124 scene-value
maximum error), with further differences at the other shoe transforms. Source
samples, Grade and Shuffle agree to float precision before the transforms. The
CPU uses f64 inverse coordinates and the GPU uses f32, and both paths are
unchanged by the performance work. This is a separate sampling-precision issue,
not evidence of equivalent full-project CPU/GPU output. The strict verification
probe intentionally fails this composition; no larger tolerance masks it.
The trace is preserved in the results artifact. Resolve the sampling contract
and add boundary-focused regression coverage before closing this acceptance.

The focused tests cover sequential/parallel sample equality for DWAA, ZIP and PIZ;
scanline/tiled multipart routing; missing alpha and image windows; 4K and twenty-AOV
memory pressure; shared consumer cancellation; same-inode/same-length replacement
with restored mtime; source preparation and held sample leases; source-cache byte
and entry retention; native GPU staging reuse and abandoned-frame recovery; held
frame immutability; CPU/GPU channel/color/alpha output; sequence delivery; exact
fractional look-ahead time and seek/range/pause invalidation.

The six-worker pressure test initially failed because decoder-retained scratch
was admitted after queued scratch. Reserving both before queueing fixed the
failure without raising the budget. Existing resize/cache tests now assert native
source reuse; a separate fixture genuinely crosses the source byte limit.

Full-resolution uncached 24 fps remains open: its budget is 41.67 ms per frame.
A larger source cache and look-ahead reduce stalls but cannot make a decoder with
insufficient sustained throughput meet that budget. The next investigation is
codec/block profiling (especially DWAA) and a rigorously equivalent faster decoder
backend or reusable decode-format cache. A native OpenEXR experiment showed promise
but is not production-integrated or qualified for Fold's complete format, memory,
cancellation, color and alpha contracts. Interactive scrubbing, multiple physical
viewers, audio cadence, packaging, and sustained desktop playback still require
native application acceptance. Full workspace tests and Clippy remain deferred to
the broader integration checkpoint.
