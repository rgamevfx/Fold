# Rendering optimization implementation

Follow-up to [further optimization research](further-optimization.md). This is a
bounded shared-render change, not M6 completion or physical playback acceptance.

## Implemented behavior

- Automatic startup GPU budgets use Vulkan `EXT_memory_budget` through the unsafe
  native adapter boundary. The limit is the smaller of half physical device-local
  memory and three quarters of current heap headroom, rounded to MiB and bounded
  to 128 MiB–32 GiB. Missing telemetry keeps the conservative 1 GiB default.
  Explicit preferences still win. The desktop viewer allowance is one quarter of
  that limit, capped at 1 GiB. This is startup selection, not continuous pressure
  polling; Fold's logical accounting does not measure all driver/decoder memory.
- Each GPU video cache can retain up to one eighth of the host budget, capped at
  512 MiB/512 entries. A shared retention entitlement caps all video caches at one
  quarter of the host budget. Allocations and outstanding consumers still hold
  their separate shared-host memory leases. Software decoded retention uses half
  its CPU budget per decoder, with a three-quarter aggregate retention cap.
- Software-plane ingress reuses mapped staging and completed destination buffers.
  A buffer cannot be recycled while encoded or submitted consumers retain its
  reservation. Uploads remain unmapped through submission and become available
  only after their map callback. Layout uniforms are reused with matching layouts.
  Cache hits require explicit publication after the containing encoder is submitted;
  an abandoned upload cannot publish initialized pixels. A regression reproduced
  the missing-publication bug before this guard and passes afterward.
- One-frame software prefetch overlaps pipe reading with current-frame work.
  Codec threads are leased process-wide: six regular process slots each retain a
  baseline thread; the remaining logical CPUs supply up to seven extra threads
  per stream. On 12 logical CPUs the first stream receives seven codec threads.
  Filter/output threads are additional; this is not a whole-process CPU limit.
- Independent hardware/software source lanes prepare concurrently. Software-only
  graphs use at most two lanes, with one retained cursor per lane, preserving
  regular process capacity for preview and export. Single-source requests avoid
  spawning preparation threads. The UI never performs decoding.
- Single-use pointwise chains of up to 16 grade, opacity and unary operations use
  one compute dispatch. Shared results and spatial operations remain boundaries.
  Ordinary and fused pipelines share WGSL math and Rust parameter packing. Each
  logical intermediate is validated, so a later clamp cannot hide overflow.
- RGBA16 delivery performs output transformation, alpha-preserving unpremultiply,
  validation and integer packing on the GPU. Readback is eight bytes per pixel,
  versus the previous sixteen-byte scene readback followed by CPU conversion.
  Queue admission ends before waiting on readback. An encoder writer thread with
  a one-frame mailbox overlaps conversion/pipe IO with the next render. Incomplete
  production cannot finalize an encoder. The encoder's sRGB-to-Rec.709 conversion
  remains CPU work; its separate transfer contract is unchanged.

## Deliberate remaining boundaries

The [native reuse design](native-reuse-design.md) explains why CUDA/Vulkan buffer
and import pooling cannot safely be bolted onto the current public buffer API.
wgpu owns destruction and cloneable raw buffer handles can outlive a cache entry.
A complete opaque source-read/submission gateway plus helper register/write/
unregister protocol and external-ownership transitions is required first. Native
HEVC allocation/import pooling is **not implemented** in this change.

General incremental graph compilation, uniform arenas and multi-frame preview
submission remain separate work. This change overlaps source preparation and
software encoding without changing the latest-request preview publication model.
It does not move ProRes decoding onto unsupported GTX 1070 hardware. ProRes GPU
retention is still keyed by decoded frame identity: revisits beyond CPU retention
can re-decode despite older GPU planes being retained. Stable source/sample keys
are a remaining opportunity; increasing VRAM alone does not solve that case.

## Verification and measurements

Results below come from the final build. Timing runs are serialized
against other test workloads, use the supplied 1080p24 HEVC and ProRes clips, and
compare with the preserved binary from the preceding audit. GPU clocks are not
locked; GPU timings and trial variation must be read alongside wall times.

### Correctness and repository checks

- 14 media unit tests and 51 render unit tests pass. Native checks: six GPU
  operator/fusion/delivery tests, two OCIO/ProRes color tests, four YUV/reuse tests,
  one aggregate-retention test and three submission/cancellation/device-failure
  tests pass. Ignored native tests were selected explicitly against NVIDIA Vulkan.
- Five video workflow tests and three EXR/alpha-delivery tests pass with GPU
  output enabled, including real ProRes encoding, changed-source cleanup and
  cancellation. The export suite was rerun after publication gating.
- App/native-video and UI checks, formatting, package boundaries and focused
  Clippy complete successfully. Clippy reports existing warnings; this is not a
  warning-free workspace claim. Full workspace/Spatial acceptance was not rerun;
  this is not a rewrite vertical-slice checkpoint. Physical desktop playback and
  settings visual review remain unverified. The previously recorded unrelated
  compositor operator-inventory failure remains outside this change.
- The actual machine selects a 4 GiB automatic host limit. Both distinct real
  clips compose correctly with hardware/software preparation, and the all-software
  two-lane path is also exercised. Exact comparison commands/logs are retained.
- Alpha verification compares GPU packing against CPU packing of identical GPU
  scene pixels, separately from full CPU/GPU rendering. ProRes frame 3 exposed
  hidden RGB differences at alpha code zero: raw straight-RGB maximum error 10549,
  visible maximum 5, same-scene packing maximum 1, display maximum 1. The visible
  check ignores RGB **only when both quantized alphas are zero**, still compares
  alpha, and retains the raw difference in evidence. Its explicit bound is 16
  codes out of 65535; the same-scene bound is also 16. No renderer tolerance or
  pixel behavior was changed to conceal this oracle mismatch.

Test transcripts, frame samples, run logs, binary hashes and measurements live in
[the evidence directory](improvements/). The pre-fix abandoned-upload failure and
transparent-pixel diagnostic are retained alongside passing results.

### Final sequential rendering measurements

Three trials per build/clip/stage, 90 frames per run, first ten omitted from
steady-state summaries. The table reports the median of trial wall-time P50s.
Both builds use the same 1 GiB host limit; the new automatic desktop limit is
verified separately. “Before” is the preserved binary from the preceding audit,
not the earlier unaudited renderer. Binary hashes are in `provenance.json`.

| Workload | Before | After | Latency reduction |
| --- | ---: | ---: | ---: |
| HEVC source | 10.62 ms | 10.67 ms | -0.5% |
| HEVC full | 21.12 ms | 20.52 ms | +2.8% |
| PRORES source | 23.11 ms | 18.13 ms | +21.6% |
| PRORES full | 30.88 ms | 24.96 ms | +19.2% |

These final-build numbers supersede the preliminary 31%/20% observations. No
material sequential HEVC speedup is established. GPU clocks remain unlocked and
the desktop shares the device; per-trial samples and `clocks.csv` preserve that
uncertainty. These are serial evaluation latencies, not physical playback FPS or
end-to-end export throughput.

The full fixture remains 15 passes. Its improvement is preparation/upload
scheduling, not pointwise fusion. The separate fusion regression verifies three
pointwise operations become one dispatch with matching pixels and preserved
intermediate overflow errors. New ProRes GPU timestamp intervals include the
explicit staging copy recorded in the command encoder; old initialization copies
were outside that interval. GPU interval medians should not be read as isolated
shader cost or added to inclusive CPU/decode medians.

Warm forward source retention uses more logical memory: HEVC peak increases from
about 162 to 227 MiB; ProRes from 166 to 245 MiB. Full-effects peaks are about 291
and 309 MiB after the change. No claim is made that larger caps accelerate a graph
that already fits without eviction.

### Budget and revisit experiment

One forward 60-frame HEVC sweep followed by those same frames in reverse per
budget. This isolates stable native-frame identities; it is not viewer/BC7 cache
playback or a ProRes retention result. All 60 return requests are included below;
P95 uses nearest rank. One trial per budget establishes the copy/retention counts,
not a general latency guarantee.

| Host limit | Native copies on 60 revisits | Revisit wall P50 | P95 | Peak logical memory |
| --- | ---: | ---: | ---: | ---: |
| 1 GiB | 39 | 34.25 ms | 75.44 ms | 227.4 MiB |
| 2 GiB | 17 | 7.92 ms | 27.91 ms | 357.9 MiB |
| 4 GiB | 0 | 7.87 ms | 8.90 ms | 458.8 MiB |

Larger limits help this workload because they prevent source eviction. They do
not reduce unchanged per-pixel shader work. The 4 GiB automatic limit leaves
headroom rather than reserving all 8 GiB. Observed driver memory in the paired
sequential run is 564–1107 MiB; graphics clocks range from idle 164 to 1885 MHz,
memory clocks from 405 to 4006 MHz, and temperature from 65 to 77 °C.

Final real-clip verification covers hardware/software composition, two software
source lanes, ProRes reverse requests and HEVC randomized requests. Preview has
zero pixel readback outside the explicitly enabled oracle. Same-scene packing
maximum error is 1–2 codes, visible full-pipeline RGBA16 maximum is 2–5 codes,
and display maximum is one code. Hidden straight RGB also differs on transparent
HEVC edge pixels (raw maximum 38481); the recorded zero-alpha rule applies there
as well. Encoded-export speed and physical desktop playback remain unmeasured.
