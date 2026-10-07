# Phase 50: GPU texture baking and multi-video performance

User-authorized implementation: an Image Bake node producing GPU-friendly mipmapped textures for scaling and instancing, plus ten-video benchmarks and evidence-led optimization.

## Design and acceptance

- Keep Image Source → Image Bake → cards/grid/sampling editable. Baking happens off the UI thread, with range/resolution/quality controls, progress/cancellation, rebuild/clear, stale-source detection and normal undo for applying a bake.
- Store source-time-addressable, bounded, versioned cache data outside authoritative project content. Preserve source references, exact time, working color and premultiplied alpha. BC7 is a lossy SDR texture option; HDR/negative values require a preserving format or a visible rejection, never silent clipping.
- Sample native-sized mipmapped textures directly. Share storage across instances and batch compatible image collections in texture arrays while preserving order. Keep a CPU reference for preview/export/headless equivalence; no per-frame image readback in the GPU playback path.
- Benchmark 1/2/5/10 distinct videos, repeated copies of one frame, different times from one video, and held resident frames. Compare baked animation with the same card layout and output dimensions; report bake cost, cache size, quality error, mean/p95 and memory separately.
- Verify invalid/missing/stale caches, cancellation, budgets, save/reopen, UI normal/narrow captures and focused tests. Record unsupported behavior explicitly.

## Investigation

Initial ten-source probe exposed a two-source native decoder LRU: cycling ten inputs evicts helpers and repeats seeks/startup every frame. The process supervisor currently permits six regular children, so increasing the LRU alone is not a correct fix. Track native source scheduling separately from texture format work.

The current textured path also evaluates every source and intermediate at canvas resolution, and different-source cards do not batch. A texture bake must bypass that path to realize the intended benefit.

An initial clipped-layout run was discarded after visual inspection; retained comparisons use fully visible cards.

## Implemented

Image Source → Image Bake retains typed image references and source time. Worker preparation compiles the same source graph used in rendering, generates linear-premultiplied mipmaps, and commits one document edit. Applying/clearing can be undone. Progress and cancellation use the existing extension worker and public jobs API. Failed/cancelled jobs remove their temporary directory; completed cache files remain available for undo.

BC7 stores transfer-encoded working-space RGB and linear coverage; sampling decodes the transfer back to the original working space. Float16 preserves negative/HDR values within half-float range. Auto chooses per texture. Explicit compact SDR rejects out-of-range values unless the artist enables Clamp to SDR. This matters even for SDR video: YUV reconstruction can create small negative/above-white RGB. Clamping and BC7 are lossy choices, not hidden color conversions. GPU encoding uses the pinned existing `block_compression` backend (opaque fast path with exact-alpha correction; alpha-capable path for translucent images); CPU-only builds retain a cancellable reference encoder.

Storage is a versioned internal `.foldtex` payload with mip dimensions, format, working-space/config identity and content hash. It is not KTX2 import/export. A source-time manifest records the authoritative requests and project/dependency identities. Missing, corrupt, changed or uncached data errors explicitly. Source files retain their linked identity; source replacement/project edits invalidate the cache. Direct filesystem stamps additionally catch changes to linked asset paths; externally modified non-anchor EXR sequence members still require relink/rebuild.

Compatible adjacent textures batch into one native-sized 2D array. Instance data selects the layer; geometry retains UVs. Mip LOD follows texture-coordinate derivatives, with a CPU reference implementing the same sampling. Source images are not expanded to the entire output canvas before each card. Repeated instances share texture storage and unchanged resident arrays need no pixel upload. Distinct animated frames still incur storage reads, validation and GPU uploads. Reordering/altering array membership can require another upload.

Bounds: 16–4096 bake resolution, 600 document frames, 10,000 unique requests, 2 GiB per bake, 8 GiB aggregate disk guard, 64 MiB shared CPU texture cache, up to 128 MiB GPU array cache (also constrained by the host budget), 64 compatible layers per batch. Originals remain authoritative. Clear detaches the cache rather than deleting undo dependencies. Project transfer to another machine requires rebuilding; stale/conflicted completed jobs may leave disposable bounded directories. There is no automatic disk-cache LRU in this slice.

## Reproduction

Create an isolated `fold-cli agent serve` session, then run `skills/fold/scripts/video_textures.py --session SESSION --directory NEW_DIRECTORY`. Requires ffmpeg; produces ten independent 5-second 1920×1080 30 fps H.264 testsrc2 clips, hue/box variations, ultrafast CRF 23, GOP 30, no B-frames. The six documents cover one/two/five/ten sources, ten shared-time copies and ten offsets of one source. Cards are 360×202.5 in a fully visible 5×2 grid, with object transform explicitly zero.

Build `cargo build -p fold-app --release --features desktop --bin fold-cli --example motion_profile`. Set the installed OCIO/native-video runtime paths and select native decoding. Run:

```sh
FOLD_PROFILE_SOURCE=motion FOLD_PROFILE_DOCUMENT=DOCUMENT_ID \
FOLD_PROFILE_RATE=30 FOLD_PROFILE_STEP=1 \
  target/release/examples/motion_profile PROJECT.fold 31 1920 1080
```

`STEP=0` measures held resident frames. `FOLD_PROFILE_IMAGES=NEW_DIRECTORY` saves rendered PPMs; `FOLD_PROFILE_VERIFY=1` compares CPU/GPU output. Capture/verification runs are separate from timing. Frame zero is cold setup and excluded from warm means/p95. GPU-ready time includes graph compilation, preparation, submission, scene and output completion; it excludes final output readback. GPU timestamps cover scene work, not physical display presentation.

For the baked ten case insert Image Bake after its collection source, set resolution 2048, end 31, Compact SDR and explicit Clamp to SDR. This retains all 1920×1080 source pixels; 310 unique source frames plus full mip chains. Run the public `fold.app.image-bake` command, poll to completion and save before profiling. The benchmark cache is local/disposable and intentionally not committed with the source tree.

## Measured optimization decisions

The initial full-resolution baked GPU scene was cheap (~0.6–0.8 ms), while file reads and SHA-256 verification dominated first-pass graph preparation. Retain a bounded 4,096-file verification ledger across pixel-cache evictions, keyed by content identity and strong filesystem stamp. Saving a newly encoded texture verifies its bytes and seeds this ledger; changed files still require verification. Cache parsed, verified manifests separately (16 entries / 8 MiB serialized, with owned working-memory reservations). These optimizations retain bounded memory and corruption detection.

A complete 31-frame 512×288 bake fits both texture caches; its second pass needs only geometry/instance uploads. Full 1920×1080 sequences do not fit those caches and continue streaming compressed texture bytes. Accordingly report first-pass, repeated-loop and held-frame results separately. This is why matching bake resolution to the largest intended card size matters even with GPU-native formats.

A real-image CPU/GPU check exposed up to 10 display codes of disagreement near clipped ACES colors with hardware sRGB texture decoding (mean <0.05 code per channel). The implementation now samples BC7 UNORM and evaluates the transfer function explicitly before bilinear/mip interpolation, matching the CPU reference; the real ten-texture scene now passes at both sampled times with a maximum difference of **one display code** (960×540, times 0 and 0.5 s). See [parity evidence](video-texture-parity.txt). This does not change the stored bake format or require rebaking.


## Results

GTX 1070, NVIDIA 580.173.02, Vulkan, release build, 1920×1080 output, native CUDA video decoder. [Final raw results](video-texture-baked-profile.json), [baseline comparisons](video-texture-baseline.json). Timed runs follow completed preparation/builds; the first full-resolution run was repeated twice. Frame zero is excluded; looping rows use the complete second 31-frame cycle (`FOLD_PROFILE_CYCLE=31`, 62 total frames). Files were recently written/read, so these are not cold-disk benchmarks. Physical desktop presentation and sustained 60 fps are not certified.

| Workload | Mean ready ms | P95 ms | Scene GPU ms | Pixel/parameter upload per frame |
|---|---:|---:|---:|---:|
| 10 live, distinct videos | 2666.458 | 2974.166 | 22.302 | 1,280 bytes |
| 10 live, held frames | 33.988 | 36.120 | 21.148 | 1,280 bytes |
| 10 full-resolution baked videos, first pass | 160.731 | 173.353 | 0.786 | 27,662,656 bytes |
| Full-resolution bake, second loop | 34.472 | 37.629 | 0.883 | 27,662,656 bytes |
| 512×288 bake, first pass | 25.165 | 32.329 | 0.686 | 1,967,936 bytes |
| 512×288 bake, second loop | 18.083 | 21.568 | 0.632 | 416 bytes |
| 10 full-resolution baked cards, held | 12.366 | 14.592 | 0.630 | 416 bytes |
| 1,000 cards / 10 textures, first pass | 162.004 | 174.987 | 0.705 | 27,694,336 bytes |
| 1,000 cards / 10 textures, held | 14.173 | 15.444 | 0.634 | 32,096 bytes |

All retained final runs exit successfully with **zero scene readback and zero CPU adapter nodes**. Baked playback has **zero native video decode nodes** and two compute passes plus one instanced raster draw; the ten-live graph uses 59 compute passes. Full-resolution first-pass means across three processes are 158.9–162.6 ms: compression alone does not remove cold validation, disk reads or upload costs. Its warm loop still streams 27.66 MB/frame. The 512 bake's second loop uploads only 416 bytes of parameters/instances; held 1,000-card playback uploads 32,096 bytes, not 1,000 image copies.

Measured peak host GPU allocation is about 521 MiB for ten live videos, 235 MiB for full-resolution baked animation, and 169 MiB for the complete 512-pixel loop. These figures exclude driver/decoder-owned allocations. The 1,000-card layout uses 50×20 cards at 36×20.25 pixels; it covers roughly the same image area as ten 360×202.5 cards. This isolates much of the instance-count cost while reusing the same ten animated source frames.

The initial source-count baseline measured one/two/five videos at 16.542 / 23.026 / 1,298.103 ms mean ready time; ten copies of one source-time at 16.982 ms; ten different times of one source at 169.202 ms. Five/ten independent sources repeatedly evict the two retained native source helpers and seek again. A six-regular-process supervisor limit means simply retaining ten helpers is not an adequate fix.

Offline preparation: 310 unique frames at 1920×1080 occupy **857,529,440 payload bytes (817.8 MiB)** plus small headers/manifest, measured wall time **462.6 s**. At 512×288 they occupy **60,993,120 payload bytes (58.2 MiB)**, measured wall time **85.3 s**. Those bake wall times include concurrent development/check activity and are preparation observations, not isolated encoder throughput measurements. Both include mip generation, color processing, compression and durable file writes. Cache files are intentionally kept out of git; the public script recreates them.

## Verification and limits

Focused checks: ten Motion image/bake tests; four baked-render tests including native BC7/Float16/alpha/array reuse; five existing textured-render tests; two app worker/cache tests (plus the GPU worker execution); release CLI/desktop/profiler builds. Cache tests cover damaged texture bytes, identity changes, changed manifests, external linked-file changes, static transparent-time keys, stale source documents, missing caches, cancellation and undo/redo. Native UI captures were inspected at normal and 280-pixel inspector widths: [normal](media/motion-images-bake-normal.png), [compact/narrow](media/motion-images-bake-compact-narrow.png). Public long-job cancellation removed partial directories without applying a bake. Full-workspace tests, Clippy, other adapters and interactive pointer/keyboard behavior are deferred.

Preview/output rendering uses the same baked scene semantics; CPU/GPU parity is at most two display codes at 1920×1080, and one at 960×540, on the sampled real-image scene. The GPU frame remains resident until an explicit screenshot/export readback. BC7 plus the optional SDR clamp is deliberately lossy; mipmapping also changes minified test-pattern pixels compared with the live path's single-level sampling. The native-size and minified quality evidence is recorded separately below.

Next performance work is **native decoder source scheduling** (source LRU and process admission together), **asynchronous validated texture streaming/prefetch and selective mip residency**, and profiling the shared output path. Those are follow-ups, not claims that this slice makes ten arbitrary original videos real-time. Per-instance independent time requests need their own cached frames; arrays require compatible size/format/color and preserve draw order. Generic ImageField-to-CPU field conversion and KTX2/HAP interchange remain outside this slice.


### Quality and lifecycle evidence

[Raw quality measurements](video-texture-quality.json) distinguish the effects of minification from the native-size comparison. On native-size source 0/frame 0, BC7 plus the explicit SDR clamp differs from the original by **0.929 mean display codes**, **2 codes at P95**, **14 at P99**, with a **177-code maximum** on this difficult synthetic pattern. This is not a lossless image format; use Float when that tradeoff is unsuitable. The minified ten-card comparison is about 1.9 mean codes, with larger isolated differences from mip filtering, compression and clipping; it is not a compression-only metric. [Final baked layout](media/ten-video-baked.png).

Public bake undo/redo and Clear/undo restore the selected reference. The final saved project was reopened in a new process and its 960×540 public-render PNG is **byte-identical**. Public previews cap requested image sizes, so the native quality comparison used matched 1920×1080 GPU-profiler outputs. A public CPU/headless render cannot use the cuda-native-only decoder service for live sources; the original comparison therefore explicitly used the GPU profiler, not an undocumented fallback. Baked CPU/headless rendering succeeds without a live decoder.

The checked-in public benchmark script was exercised end to end against the final build: six documents, linked fixture reuse, small 20-source-frame bake, and saved live/baked projects. Full-size fixture generation/import and 310-frame bakes were verified separately above. It skips already-linked canonical paths to avoid an empty import transaction.


Full-output verification: the [strict one-code check](video-texture-parity-1080-strict.txt) fails at 1920×1080 with a maximum difference of two codes at frame 0. Repeating with the explicitly selected `FOLD_PROFILE_CODE_TOLERANCE=2` passes both samples (frame 0: 2; frame 15: 1), recorded in [full-resolution parity evidence](video-texture-parity-1080.txt). The profiler's default tolerance remains one code. This residual float/raster sampling difference is retained as a precision limitation; the strict failure is not reported as a passing check.
