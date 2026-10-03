# Phase 18 — Retained decoding and BC7 presentation caching

## Goal, boundaries, and status

**Implemented and verified on the Linux/GTX 1070 reference machine.** Replace small-batch decode processes and temporary RGB files, verify source hardware decoding, and implement the user-approved viewport-sized BC7 display cache without making current-frame presentation wait for compression.

Preserve the verified CFR profile, rational timing, source/export safety, independent viewer generations, held images, and Every-frame pacing. Phase 19 still owns general scheduling/fairness, review-range preparation, cache-policy tuning, and sustained playback acceptance. Delivery hardware encoding remains phase 20. No authoring schema or project transaction semantics changed.

The source profile is deliberately unchanged: MP4/H.264, progressive 8-bit limited BT.709 4:2:0, square pixels, left chroma, zero-origin verified CFR, at most 18,000 frames / 2 GiB / 4,194,304 source pixels. **This is not 4K source-format certification.** The 3840×2160 → 480×270 sizing test proves viewport geometry, not acceptance of a 4K media file under the current source limits.

## Implementation

### Retained range decoding and source ownership

- `crates/media/src/video_stream.rs` retains FFmpeg behind a demand-driven pipe. One bounded request and one bounded result slot, a cancellable 30-second frame deadline, and supervised kill/reap replace temporary RGB frame files. Idle retained processes are pipe-backpressured rather than killed by a wall-lifetime timeout. Input decoding, filtering, and the rawvideo output encoder are explicitly single-threaded; otherwise FFmpeg's automatic output frame-thread queue retains additional full float images.
- `Decoder` keeps two LRU range cursors, including two different ranges of the same source. Sequential reads and nearby forward gaps reuse the process; distant/reverse misses open an accurately sought range. Cached reverse requests do not launch a codec. FFmpeg owns packet reordering; the importer verifies every presentation timestamp and duration. Timestamp gaps/VFR are rejected, not inferred from average fps.
- Retained signal storage is planar GBR float32 after explicit YUV reconstruction, preserving signed/above-one values until OCIO. Legacy RGB8 remains a separate mode. Odd preview sizes reconstruct at native size before nearest float-RGB scaling because subsampled YUV conversion requires even dimensions. This fixes actual arbitrary-viewport decode failures without quantizing working color.
- `source_pin.rs` shares immutable, hash-verified source copies process-wide through weak registrations. Each decoder holds up to two local pins; active ranges retain their own source leases until their processes stop. Fresh consumers still hash the linked file and reject changed/missing sources. Existing consumers may continue using their immutable copy after replacement/unlink, as required for pinned work.
- Verified CFR metadata is cached by complete-file fingerprint, with 128 bounded records. A copy whose hash matches already verified bytes does not repeat full-frame probing. Untrusted persisted metadata is compared against the verified result; an unknown fingerprint is still probed. No path/mtime shortcut weakens verification.
- Bounds: **4 GiB aggregate immutable-source scratch**, **64 MiB / 512 decoded entries per decoder**, **128 MiB aggregate decoded-cache retention**, and **eight supervised media processes across decoding, probing, browser previews, audio, and encoding**. Exhausted process/source capacity reports backpressure; decoded-cache pressure can omit retention without withholding the current result. The browser already has one active thumbnail/waveform job and now shares the process ceiling. Fair priority reservations/retries remain phase 19.
- Cache counters exclude caller-held decoded images, codec-native buffers, and driver allocations. These are not a claim of a single exact aggregate RSS reservation. Process-tree measurements below include those costs. No new disk render cache was introduced.

### Verified NVDEC, not zero-copy

- Software remains the default. Desktop preview, CLI rendering, and delivery accept explicit `FOLD_VIDEO_DECODER=software|cuda`; invalid values fail clearly. This policy is independent of `FOLD_RENDER_BACKEND`.
- CUDA decoding requests device 0 and CUDA output surfaces, then explicitly downloads NV12 before CPU scaling/YUV reconstruction. There is **no CUDA/Vulkan external-surface import or zero-copy claim**. On this machine CUDA device 0 and the Vulkan presentation adapter are the GTX 1070, PCI `00000000:01:00.0`.
- NVDEC output matches software GBR float planes exactly in the tested sequential, fractional-rate, seek, and resolution fixtures. Verified source sizes include 48×16, 64×32, 320×180, and 1920×1080. Hardware setup/download costs do not consistently beat software, so availability is not presented as a speed guarantee.
- The GTX 1070 H.264 adapter rejects source dimensions outside 48..4096 by 16..4096 before launching a CUDA decoder, in addition to the ordinary media-profile restrictions. A forced-CUDA run of the pre-existing 16×16 color fixture exposed this real device limit; it is now an explicit software-selection diagnostic and a regression test, not silent hardware fallback. The supported 64×32 application workflow passes with CUDA selected.
- CUDA resources stay inside the supervised child. Cancellation kills/reaps it; no external GPU handle outlives an untracked owner. Native download, pipe transport, float storage conversion, and later wgpu upload are separate copies. A 1080p delivered GBR frame is **24,883,200 bytes**; a 480×270 frame is **1,555,200 bytes**. NV12 download has a logical 1.5-byte/source-pixel payload, before pitch/alignment and codec preroll/prefetch. Those bus transfers are not misreported as measured zero-copy. Decode statistics count frames/bytes received from the range pipe, not all internal codec preroll.

### Viewport-sized BC7 retention

- `Shell` fits requested resolution to the actual drawable image area in **physical pixels**, preserving aspect ratio and never upscaling past the document's quality resolution. Full/Half/Quarter now apply relative to that fitted area; the menu tooltip explains this. Resize/DPI/quality changes affect the existing dimension-bearing key. Export dimensions remain independent.
- The native performance-probe override deliberately retains full document resolution. Its regression test prevents a future 1080p checkpoint from accidentally measuring a smaller viewport.
- The immediate image remains RGBA8. Foreground misses and readiness take priority: new compression is admitted only without an outstanding foreground render, during idle periods or resident-frame replay. One background worker compresses only unused, unheld, completed cache entries; it never replaces a currently displayed registration or gates current-frame presentation. Evicted/retargeted results cannot publish a new viewer frame. Cache hits directly sample their registered texture without decode, OCIO processing, or image upload/readback.
- `gpu::PresentationCompressor` uses pinned **`block_compression` 0.7.0 / wgpu 27**, MIT-licensed (including the Intel attribution). Only BC7/WGPU features are enabled. The upstream fixed-block shader's runtime-loop-bounding exception was reviewed; input sizes/settings/task count are bounded. The license is shipped and hashed by `scripts/package_fold.py`.
- The selected mode-6 opaque-ultra-fast encoder is inexpensive on the reference GPU. Shared endpoint p-bits can otherwise turn opaque alpha into 254; a GPU endpoint fix keeps alpha exactly 255, changing RGB endpoints by at most one code. All display images are already black-matted; alpha-edge appearance is tested after that matte, not by caching scene-linear transparency.
- A GPU quality pass rejects any result with a channel difference **greater than 24 SDR codes** or nonopaque alpha. The original RGBA8 entry remains intact. Accepted gradient/thin-stroke fixtures additionally pass **RMS <3 codes**, with measured maxima 3–4. Deliberately difficult multicolor blocks and the reference color-chart footage exercise the rejection path. **BC7 is lossy and compression is not guaranteed for every image.** The more expensive presets were evaluated rather than hiding their latency or increasing tolerances to pass the chart.
- Non-multiple-of-four dimensions use replicated edge padding and cropped UVs; padding never becomes displayed content. BC7 uses `Bc7RgbaUnorm`, not the sRGB sampling format: OCIO has already transformed the image. Overlays remain outside the cache. Compressed leases explicitly reject the delivery/readback API.
- The host accounts for compressed textures, padding textures, block buffers, parameter buffers, and submission-held leases within the existing **512 MiB** budget. Queue completion and consumer release are both required before reuse. The presentation LRU retains its **256 MiB / 512-entry** policy; compression does not remove the entry ceiling. Tiny images without a storage saving remain RGBA8. CPU-renderer mode and unsupported devices retain the existing RGBA8 path.
- Compression failure/disconnection reports an honest diagnostic in the existing statistics area and retains RGBA8. Shutdown joins the compression worker before device/driver teardown; ordinary polling never joins or waits for it. A detached-worker shutdown crash found by the native diagnostic was fixed and has a maintained regression. Cold pipeline compilation is worker-side and can be expensive; it is not claimed to have zero driver-level contention.

## Reference measurements

Environment: Linux `7.1.5-76070105-generic`, Intel i7-8750H, approximately 32 GiB RAM, GTX 1070 8192 MiB, NVIDIA `580.173.02`, Vulkan/wgpu 27, Rust 1.95.0, FFmpeg `6.1.1-3ubuntu5` (libavcodec 60.31.102, libavformat 60.16.100, libavutil 58.29.100), OCIO 2.4.2 / ACES 1.3 Studio 2.2.0. Build: `c59c544` plus the phase-18 working tree; package manifests record dirty state and source/binary hashes.

All local fixtures, logs, screenshots, packages, and diagnostic sources live under `/home/rgame/Documents/fold-phase-18/`, outside the repository.

### Decode and pinning

`crates/media/examples/decode_probe.rs` measures inspect, first-frame preparation, subsequent pipe/decode/conversion time, exact plane equality, seeks, retained bytes, and launches. Times below include CPU reconstruction/transport/storage conversion, not isolated GPU codec timestamps. These are short reference runs, not sustained playback acceptance; OS file cache was not flushed.

| Source/request | Software first / warm mean / P95 | CUDA first / warm mean / P95 |
| --- | --- | --- |
| 1080p30 source, 30 frames, 480×270 request | 85.192 / 2.457 / 3.271 ms | 219.428 / 6.516 / 2.381 ms |
| Same source, 1920×1080 request | 145.678 / 22.364 / 45.634 ms | 291.562 / 24.731 / 31.238 ms |
| 14,385-frame, 320×180, 24000/1001 source; first 180 frames | 255.559 / 0.911 / 1.355 ms | 406.157 / 0.854 / 1.220 ms |

Each sequential pass launches **one** range decoder. The 480×270 CUDA mean exceeds P95 because a large startup-adjacent outlier remains after the first frame; this small sample is not a steady-state throughput claim. The odd-size 479×273 software/CUDA plane comparison also passes.

The nearly ten-minute source is 45,814,434 bytes. Full import verification takes **3.479 s**; first decode no longer repeats that full probe. Retained-cursor seeks at distant positions take approximately **49–89 ms software / 154–250 ms CUDA**, with a subsequent request in a retained second range about **0.6 ms**. Software is intentionally still preferred here.

Sampled process-tree RSS/scratch peaks:

- 1080p decode/seek probe: **938,266,624 / 1,114,774 bytes**.
- Long-source probe: **455,110,656 / 45,814,434 bytes**.
- Long-source CUDA process-tree GPU allocation, sampled with `nvidia-smi`: **196 MiB**, covering two retained CUDA range processes. This is separate from the wgpu host budget, not a whole-application or worst-case VRAM certification.

RSS sampling may double-count shared mappings and miss short peaks. Scratch contains verified sources/probe diagnostics, not RGB frame batches. Full-file hashing and initial complete CFR probing remain worker costs; there is no unsafe mtime-only fast path.

### BC7 and playback

Final reference GPU encode/pad/opacity-fix/quality timings are approximately **0.43 ms at 479×273** and **0.74 ms at 480×480**. Worker submission-to-observed-completion wall times were approximately **13–15 ms**, including polling/readiness overhead. Pipeline construction was about **0.16–0.17 s warm**; an initial cold driver compilation took **12.5 s**. Do not confuse pipeline setup, GPU execution, worker latency, and scanout latency.

- 480×480 storage: **230,400 bytes BC7 vs 921,600 RGBA8**, exactly 4:1.
- 479×273 storage with padding: **132,480 vs 523,068 bytes**; logical size and UVs remain 479×273.
- Accepted gradient/thin-stroke fixture: RMS **0.746–0.757**, maximum **3–4** SDR codes, alpha exactly 255. Cropped/blurred spatial alpha-edge fixture: RMS **0.576**, maximum **1**. RMS is measured across all four RGBA channels; opaque alpha contributes zero. Runtime admission is governed by the per-channel maximum and exact opacity, not a universal RMS threshold.
- Film fixture: a frame at 60 seconds from **Big Buck Bunny**, © 2008 Blender Foundation, CC BY 3.0, downloaded from the [official Blender archive](https://download.blender.org/peach/bigbuckbunny_movies/BigBuckBunny_640x360.m4v.zip). A one-second excerpt was resized from its 640×359 presentation to 640×360 and re-encoded into the existing tagged CFR fixture profile; this does not certify import of the original movie. At the two viewport test sizes, RMS was **1.481 / 1.459**, but maximum errors **37 / 53** triggered RGBA8 retention. This confirms the conservative quality gate on film imagery, **not broad BC7 compression coverage of natural footage**. Fixtures remain external; `bc7-film-acceptance.log` records the run. An initial optional-footage diagnostic incorrectly applied the synthetic-only RMS qualification to a substituted 4×4 film sample; the diagnostic now preserves its dedicated tiny/alpha fixtures and applies the declared runtime contract to footage. No production tolerance was relaxed.
- Reference chart attempt: RMS about **2.7**, but isolated errors **143–169** codes; the GPU quality gate correctly retains RGBA8. The adversarial 4×4 block is likewise rejected. Average error alone is not acceptance.
- GPU quality/padding/lifetime test host peak: about **23.7 MB**, below its 128 MiB test budget. Image readback is used only by explicit comparison tests; normal compression performs only the small status/timestamp readback.
- A temporary real-device cache diagnostic exercised immediate RGBA8, background replacement of retired entries, padded UVs, and a BC7 cache hit with no new render request. Cache-hit application admission was about **29 µs**, not a physical presentation latency measurement. Its artifact is `preview_compression_probe.rs`; it was kept outside the UI crate rather than introducing forbidden render/media dependencies solely for a diagnostic.
- The reused native mixed-viewer application-command probe passes delayed Every-frame rendering alongside independent Real-time playback/audio, mode switches, and paused seek to frame 3. **64 cached admissions took 2.135 s at 30 fps**. This checks pacing/isolation, not uncached performance or OS drag input.

### Full-resolution scene checkpoint

The existing release `execution_probe` reruns the two-video/nested procedural-title/stereo-audio workload at **1920×1080**, not viewport resolution. GPU preview and explicit delivery agree; CPU/GPU output differs by at most **one SDR code**.

- First preparation: **523.262 ms**; warm subsequent frames: **154.604–177.337 ms**.
- GPU evaluation/output: **5.59–7.19 ms**.
- Subsequent CPU adapter work: approximately **81–106 ms**.
- Uploads: **82,944,000 bytes/frame**; normal preview image readback: **zero**; validation/timestamps: 48 bytes/frame.
- Host-accounted GPU peak: **265,422,040 bytes**, within 536,870,912 bytes.
- Explicit GPU output export succeeds (`checkpoint-final.mp4`).

This improves the phase-17 warm preparation measurement of 425–511 ms, but **does not meet sustained uncached 1080p30**. Phase 19 must address remaining adapter/transfer costs and aggregate scheduling rather than using smaller viewports or cached replay to declare that checkpoint passed.

## Verification and acceptance evidence

- Media: **15 tests passed**, including shared immutable-source ownership/cleanup, eviction of idle rather than active source copies, retained two-range cursors, fractional rates, reverse/random seeks, odd sizes, stale/missing media, VFR rejection, forged metadata, blocked-pipe cancellation/reap, failed-process diagnostics, CUDA equality, and explicit unsupported-CUDA geometry. Final retained-decoder rerun after bounding output threads: **4 passed**.
- Render: **19 unique tests passed** across the affected suite and final focused GPU reruns, including OCIO/operators/adapters, BC7 direct sampling, padded edges, opacity/alpha, quality rejection, source-image preservation, unsupported features, and allocation pressure. BC7 fixtures use real Vulkan, not a mock encoder.
- UI: **43 tests passed**, including real texture lifetime/pressure tests, compression-worker failure/shutdown, viewport/DPI/quality sizing, full-resolution probe protection, existing normal/narrow ImGui controls, held-image admission, and stale-generation rejection.
- Application: **14 focused tests passed** for video/color/audio/viewer transport, including the native GPU-connected Session and pinned delivery. The supported video workflow separately passes **5 tests with CUDA selected**. Forcing CUDA onto the old 16×16 fixture is deliberately unsupported, as documented above; its software path passes.
- Strict all-target Clippy for media/render passes. App/UI Clippy completes with inherited warnings only: four UI test `drop_non_drop`, one compositor conditional, and ten Motion warnings. New-code warnings were corrected.
- Touched-file formatting, diff whitespace, package boundaries, no-feature headless checks, and **12 Python tooling tests** pass. Full workspace/sustained-load checks remain at the phase-19 slice checkpoint, per the development plan.
- The relocatable desktop/CLI package builds with pinned BC7 notices/checksums. The packaged native GPU+CUDA viewer was launched from `/` with `FOLD_COLOR_ROOT` unset and `$OCIO` invalid, and visually inspected. The normal native two-video/title output renders at fitted viewport resolution; no extra toolbar or loading indicator was added. Screenshot evidence includes `22-49-39` (software decode/GPU render) and `23-20-32` (packaged CUDA/GPU render).
- Development failures found and fixed include odd YUV dimensions, BC7 opaque-alpha quantization, a test timestamp-query setup error, and detached GPU-worker teardown. A temporary cross-layer UI diagnostic was removed from the crate after the boundary check rejected its test-only dependencies. These failed iterations are not counted as passing gates.

## Reproduction and manual review

```sh
export PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig
export FOLD_COLOR_ROOT=/home/rgame/Documents/fold-phase-18/final-package
export FOLD_TEST_COLOR_ROOT="$FOLD_COLOR_ROOT"
cargo test -p fold-media -- --include-ignored --test-threads=1
cargo test -p fold-render --features gpu -- --include-ignored --test-threads=1
cargo test -p fold-ui --all-features --lib -- --include-ignored --test-threads=1
cargo run -p fold-media --release --example decode_probe -- /absolute/source.mp4 90 480 270
```

Native OS click/drag/DPI interaction and physical scanout were not independently certified; computer input on this machine remains limited. The command-driven native probe and automated normal/narrow tests cover the changed contracts. An artist review is still useful: open `viewport-review.fold`, play/loop then seek backward, switch Full/Half/Quarter, resize/maximize the viewer, and compare thin titles/gradients during replay. Expect the same fitted framing, no loading flashes, and no alpha darkening; complex pictures may intentionally stay RGBA8. No left-click-and-drag automation is claimed.

Generalized range preparation, longer cache retention than the current entry ceiling, source-profile expansion, persistent disk caches, CUDA/Vulkan zero-copy, and hardware delivery encoding are not smuggled into this phase's acceptance.
