# Render responsiveness correction — October 2026

The reported workload was one image, a 20 px Gaussian blur, a transform, and a grade with an ellipse mask. The reproduction uses the actual compositor compiler and GPU evaluator at 1920×1080 with ACEScg processing and the bundled SDR display transform. It advances document time without using the presentation cache. No mouse/drag automation is required.

## Findings and changes

The main still-image problem was repeated source processing. Every evaluation reopened, hashed, decompressed and repacked the same EXR, then uploaded 33,177,600 bytes and reapplied its input transform. Removing all effects still took about 194 ms per frame; graph compilation was about 0.2 ms. This reproduces the slowness without UI involvement. Inspection also confirmed the desktop retains its renderer on the preview worker and deduplicates unchanged preview demands; no UI change was justified by this reproduction.

The GPU renderer now retains validated, immutable working EXR inputs across time and downstream effect edits. Keys include source content, metadata, selected channels, dimensions and input processor identity. Each renderer retains at most eight entries and the smaller of 128 MiB or one eighth of its host budget. These textures are already accounted in the shared host's working allocation budget, are never writable scratch, and are released under the existing host-pressure policy. Submitted and held frames keep independent leases. Retention is separate from the display-image cache and is also used by GPU delivery.

Source misses still hash and validate the complete bytes through the existing EXR decoder. On Unix, reuse checks device/inode, size, modification time and change time, including nanoseconds. Pre/post-decode stamps must agree before retention. Replacement, missing files and same-size rewrites with restored mtime therefore do not silently reuse stale source pixels. Other platforms conservatively rehash cache hits. Entries are usable only after successful GPU completion/validation. The initial experiment rehashed every hit on Linux too; that removed decode/upload costs but still took about 68 ms per frame, so it was replaced by the stronger-than-mtime file-change check.

The Gaussian shader also repeated the same exponential weight calculation and overlapping texture fetches for every pixel. Its replacement shares the CPU reference's finite kernel weights and cooperatively loads samples in 64-pixel stripes using 2 KiB of workgroup storage. Both passes retain the same support, summation order, normalization, alpha clamp and transparent/clamped borders. There is no reduced resolution, approximate blur or hardware-filter substitution. The old GPU Gaussian branch is removed; legacy box blur is unchanged.

The supplied H.264 sample exposed a separate metadata handling bug: FFprobe omitted frame/packet durations although all exact PTS values and the stream end were present. Import now proves missing intervals from adjacent integer PTS values and, for the last frame, a zero-origin stream's exact end. It still rejects VFR, unknown final duration and contradictory explicit durations. The original `ltxsamplevideo.mp4` imports and renders directly after this fix.

## Benchmark method

Baseline commit: `382a11bdce230298eb8f7fb66b8f4be83ebbc9b1`. Both binaries are release builds on the same GTX 1070, NVIDIA 580.173.02/Vulkan host. GPU commands run outside sandbox device isolation. Native decoding uses the existing `/tmp/fold-19b-package/bin/fold-video-helper` and `cuda-native`; OCIO uses the same package. Compare before/after binaries sequentially, three repetitions per workload, 120 consecutive frames each. Report frame zero separately; warm distributions include every frame from 1–119. Frame wall time includes graph compilation, preparation/decode, GPU execution and display readiness, but excludes process/device startup and initial import. GPU time is the scene graph's timestamp interval, not codec time. There is no presentation-cache replay, frame readback during timing or UI scanout measurement.

The image is a full-resolution EXR extracted from the supplied H.264 video. Video comparisons use six-second H.264/8-bit/BT.709/25 fps derivatives of all three supplied files, because the baseline rejects the original H.264 timing and the current adapter does not support 10-bit HEVC or ProRes/MOV. The ProRes derivative is opaque and does **not** qualify original alpha fidelity. Originals remain untouched. Separate after-only verification uses the original H.264 file. HEVC/ProRes native ingest remains a recorded format limitation, not a performance result.

Build and reproduce:

```sh
cargo build -p fold-app --example render_profile --features native-video --release --offline
FOLD_COLOR_ROOT=/tmp/fold-19b-package \
FOLD_VIDEO_HELPER=/tmp/fold-19b-package/bin/fold-video-helper \
FOLD_VIDEO_DECODER=cuda-native \
target/release/examples/render_profile /absolute/source.mp4 120 20 full
```

Use `source` or `blur` instead of `full` to isolate stages. Set `FOLD_PROFILE_VERIFY=1` in a separate run for first-frame CPU/GPU and matched delivery comparisons. The benchmark executable emits one JSON record per frame. Fixture conversion commands and measured binary/source hashes accompany the results below.

## Results

Ranges below span the three independent repetitions, not confidence intervals. Per-run cold times, all distribution summaries and source/binary hashes are retained in [the benchmark results](render-responsiveness-results.json). Raw logs and the sequential comparison runner are under `/tmp/fold-render-perf/` on this machine.

| Workload, 1080p with all effects | Before warm mean | After warm mean | Before warm P95 | After warm P95 |
| --- | ---: | ---: | ---: | ---: |
| Still EXR | 215.84–219.95 ms | 15.45–15.72 ms | 249.88–254.18 ms | 18.52–19.21 ms |
| H.264-derived video | 23.11–24.10 ms | 18.72–18.99 ms | 29.69–32.19 ms | 22.60–23.14 ms |
| HEVC-derived video | 22.87–23.53 ms | 19.25–19.43 ms | 27.93–30.21 ms | 23.72–24.51 ms |
| ProRes-derived video | 23.36–23.89 ms | 18.97–19.74 ms | 29.23–30.34 ms | 22.25–24.22 ms |

The still workload is approximately 14× faster warm. Across video runs the reduction in mean wall time is about 17–20%; scene GPU time drops from 12.43–12.70 ms to 8.34–8.47 ms, approximately 33%. Initial exploratory video timings were noisier and are not substituted for these repeated results. An effect-free baseline was already relatively fast; the retained change targets Gaussian execution, not a speculative decoder rewrite.

Warm still pixel uploads fall from **33,177,600 bytes per frame to zero**, with zero CPU image-adapter nodes and one resident source hit. Logical host allocation rises from 166.29 to 197.93 MiB, the expected single retained 31.64 MiB working image. Video peak logical allocations remain 228.58 MiB, and warm video pixel/geometry uploads remain zero. These are engine-accounted allocations, not physical driver/RSS measurements. First-use still frames remain 246.8–254.3 ms after the change: the cold decode cost is not solved or hidden.

A separate **450-frame run of the original H.264 sample** (18 seconds of document time) averaged 19.43 ms warm, P95 23.15 ms, maximum 29.68 ms; cold frame 282.96 ms. Peak logical allocation remained 228.58 MiB and all 449 warm frames uploaded zero pixel/geometry bytes. This is uncached engine throughput, not a real-time desktop/audio acceptance claim.

Fixture creation, using the existing system FFmpeg:

```sh
ffmpeg -v error -i ~/Videos/ltxsamplevideo.mp4 -frames:v 1 \
  -compression zip16 /tmp/fold-perf-still.exr
# Repeat for each supplied video, with a distinct NEW temporary output path:
ffmpeg -v error -i /absolute/original-video -an -t 6 \
  -vf 'fps=25,setsar=1' -c:v libx264 -preset fast -crf 18 \
  -pix_fmt yuv420p -color_range tv -colorspace bt709 \
  -color_primaries bt709 -color_trc bt709 /tmp/new-benchmark.mp4
```

## Validation and limits

Final checks passed: 43 GPU-enabled renderer/application test executions (including explicitly enabled native tests), plus 27 focused CPU/media test executions. Seven channel tests run in both feature configurations; these are 70 successful executions, not 70 distinct test functions. The CUDA **host-download** compatibility test in `retained_decode` remained opt-in/ignored; the GPU-resident `cuda-native` route was exercised by the benchmarks and native scheduling test. `cargo check -p fold-app --features desktop --offline`, touched-file `rustfmt --check`, `git diff --check`, and `scripts/check_boundaries.py` passed. Full workspace tests, Clippy and packaging are deferred to the broader integration checkpoint.

Focused commands:

```sh
cargo test -p fold-media --offline --lib --test retained_decode --test exr
cargo test -p fold-render --offline --test compositor_ops --test channels
cargo test -p fold-render --features native-video --release --offline \
  --lib --test channels --test gpu --test gpu_adapters --test gpu_fusion \
  --test gpu_scheduling --no-run
cargo test -p fold-app --features native-video --release --offline \
  --test multilayer_workflow --no-run
```

Run the GPU-enabled test executables outside device isolation with `--include-ignored --test-threads=1`, `FOLD_TEST_COLOR_ROOT`/`FOLD_COLOR_ROOT` pointing to the pinned package, `FOLD_VIDEO_HELPER` to its helper, `FOLD_NATIVE_FIXTURE` to the original H.264 sample, and `FOLD_VIDEO_DECODER=cuda-native`. Logs are `/tmp/fold-render-perf/test-*.log`.


The source-reuse regression failed before the fix (`cpu_adapter_nodes` was 1 instead of 0 after an effect edit) and passed afterward. The exact-duration regression likewise failed before the metadata correction. Tests cover shared input reuse, downstream edits/time changes, resize/channel invalidation, byte/entry eviction, source changes with restored mtime, held-resource immutability and bounded host allocations. Gaussian comparisons cover both border policies, fractional/asymmetric/zero sizes, partial stripes and the maximum 256 px sigma. Full 1080p still and original-video graphs differ from CPU display output by at most one SDR code; matched GPU preview and delivery bytes are identical.

These are engine measurements and focused regressions. Native desktop input/dragging, physical presentation latency, audio synchronization, mixed-viewer/export performance and the complete phase-19/23 acceptance suites are not recertified by this correction. CPU Gaussian execution remains the correctness reference; the measured acceleration is in the GPU path. First-use EXR decode remains a cold cost. This does not close the broader phase-23 or delivery/hardening obligations.
