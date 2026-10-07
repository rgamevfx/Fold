# Phase 55 — Common video and audio media

## Goal and boundaries

Import and render the user's HEVC Main 10 MP4 and ProRes 4444 MOV without reducing precision or discarding alpha. Add ProRes 4444 XQ, ProRes 422 profiles, DNxHD/DNxHR, WAV PCM16/24/32 and float32, MP3 CBR/VBR, AAC/M4A and FLAC. Preserve project serialization, transactional import, exact CFR/sample time, immutable sources and bounded background work. Encoding choices remain the existing delivery profiles; this phase expands source decoding.

## Implementation

- [x] Shared import extensions across the file picker and folder/drop scanner. Linked inspection and existing public import use the same media adapters. MOV/MP4/MXF video and WAV/MP3/AAC/M4A/FLAC audio are accepted; original stream facts remain in source metadata.
- [x] Native 8/10/12-bit YUV 420/422/444 and planar GBR samples, optional alpha, full/limited-range reconstruction, and chroma location. CPU and GPU agree; encoded RGB is premultiplied before the existing OCIO unpremultiply/transform/premultiply contract. Working images retain precision and alpha. Browser alpha bypasses the limited-range color transform.
- [x] Retained video ranges, up to four decoder threads per process (six regular process permits bound the total to 24 decoder threads), one filter/output thread, shared immutable source pins and bounded caches. A frame larger than the 64 MiB local retention allowance can be returned without caching; the aggregate pipe allowance is 128 MiB so a 4K 12-bit 444+A frame fits. Referenced decoded/GPU leases remain separately budgeted.
- [x] Per-source backend selection: software-only formats work in native-NVDEC projects. Compatible H.264/HEVC still require the selected hardware backend to succeed; driver errors do not masquerade as hardware success. The isolated LGPL runtime enables HEVC alongside H.264. CUDA copies NV12/P010 into engine-owned buffers; the GPU preserves P010's ten significant bits.
- [x] Standalone audio retains the historical `WaveInfo`/`Wave` archive representation to preserve existing projects. WAV integer/float PCM, MP3 CBR/VBR, AAC/M4A and FLAC are decoded into 48 kHz stereo float PCM. Finite float headroom survives source decoding; the existing final mix/output clamp remains.
- [x] Audio prepares 4096-sample blocks on demand into budgeted disk storage, keeping at most two backpressured streams per worker. Retained PCM blocks survive cursor eviction. Restarting an evicted cursor trims exact resampled samples from the source origin, avoiding approximate compressed seeks. First use of a distant uncached sample still decodes the preceding range. Monitored playback retains reserved process capacity; the audio callback does not perform file or codec work.
- [x] MP3 decoder delay/discard metadata and exact M4A container ends determine usable sample duration. Raw ADTS AAC has no equivalent gapless-end guarantee; its decoded samples define duration. Standalone audio inspection counts decoded samples once per content identity and reuses that proof for immutable copies.
- [x] MOV/MP4/MXF inspection verifies presentation-sorted packet PTS/durations for the accepted frame/access-unit codecs, avoiding a full video decode at import. It rejects duplicate/gapped/VFR timing and keeps exact rational CFR. Linked video inspection hashes before/after the combined inspection rather than nesting redundant full-file hashes.

## Verification

Generated fixtures cover HEVC Main/Main 10; ProRes Proxy/LT/422/HQ/4444/4444 XQ; DNxHR LB/SQ/HQ/HQX/444, including RGB 444; DNxHD in MOV and MXF; PCM16/24/32/float WAV; MP3 CBR/VBR; AAC/M4A and FLAC. Native bytes are compared exactly with independent FFmpeg output across sequential and reverse/random requests. Float RGB reconstruction is compared with FFmpeg separately from alpha because zscale's limited-range conversion can incorrectly scale alpha.

Final focused checks passed: **23 media tests, four native-plane GPU tests, one XQ/ACES compositing test and five application ingest tests (33 total)**. Three hardware/fixture-dependent media tests remain ignored. The earlier full media-crate run also passed the unchanged EXR/output suites. The rebuilt pinned runtime and helper compile successfully. The final release desktop build passes (`target/release/fold-desktop`); diff whitespace review passes.

Focused acceptance commands:

```sh
cargo test -p fold-media --features native-video --lib --test codec_expansion --test retained_decode --test thumbnails --offline -- --test-threads=1
cargo test -p fold-render --features gpu,native-video --lib gpu::yuv --offline -- --ignored --test-threads=1
FOLD_TEST_COLOR_ROOT=/tmp/fold-ocio-package cargo test -p fold-render --features gpu,native-video --test gpu_color prores_xq --offline -- --ignored
cargo test -p fold-app --test project_ingest --offline -- --test-threads=1
cargo build -p fold-app --features desktop --bin fold-desktop --release --offline
```

The native runtime was rebuilt from the pinned source archives into `/tmp/fold-codec-runtime`; helper compilation uses `FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`. The matching helper is staged at `/tmp/fold-codec-runtime/bin/fold-video-helper`, with its adjacent runtime libraries verified by the dynamic loader. Use `FOLD_VIDEO_HELPER=/tmp/fold-codec-runtime/bin/fold-video-helper` for hardware qualification. The existing `target/lib` and `target/share` symlinks point into a read-only previous package; those external files were not replaced. A full package rebuild through the existing packaging script includes the new runtime automatically. Default software imports do not depend on that optional helper. The physical NVIDIA driver is unavailable. P010/NV12 reconstruction and CPU/GPU parity run on Vulkan software rendering; they do not certify real NVDEC execution or GTX 1070 throughput.

The new XQ test verifies CPU/GPU ACES conversion and alpha-over-background, including software decoding in a native-NVDEC-configured project. Audio tests compare exact resampled samples, gapless MP3/M4A duration, EOF silence, float headroom, incremental preflight, cursor eviction and cancellation. Existing import/relink/undo/persistence and stale-source tests remain applicable.

## Performance evidence and limits

Both original `~/Videos` files successfully import and decode in release mode through `media_probe`, including 120 sequential frames and last/first/middle seeks. Source files are untouched.

```sh
cargo run -p fold-media --release --example media_probe --offline -- /path/to/source.mov
```

Initial full-frame probing took about 4.7 s for HEVC and 69 s for the 422 MiB ProRes file under concurrent build load. Switching to packet timing reduced those observations to about 0.27 s and 13 s before the redundant linked-import hashes were removed. The same run measured native-plane warm means of 6.95 ms/frame (HEVC) and 22.81 ms/frame (ProRes), excluding GPU work; the final repeated run under concurrent compilation measured 8.25/38.83 ms and import times of 0.26/9.59 s after the redundant hashes were removed; immutable pinning dominated ProRes first-use time (about 10 s). These short, non-isolated CPU/pipe measurements are **not** sustained application playback or hardware-performance acceptance. Re-run the probe on the reference machine, then measure complete rendering and memory under multiple sources/viewers.

Retained restrictions: progressive SDR, square pixels, zero-origin CFR video, at most 18000 frames, at most 8,847,360 pixels, 2 GiB source files; mono/stereo audio, 8–192 kHz, ten-minute source/plan duration and aggregate scratch limits. The native helper retains its existing 4,194,304-pixel geometry ceiling; larger sources require software decoding. VFR, interlacing, arbitrary HDR, multichannel mixing and new export profiles remain separate work. Missing SDR color tags use a recorded Rec.709 reconstruction assumption; raw facts remain unknown and artist OCIO input assignment remains authoritative. Untagged alpha planes are interpreted as straight coverage.

## Acceptance and follow-up qualification

- [x] **Artist acceptance (2026-10-07):** user reviewed the result, reported “looks good,” and requested completion and commit. Phase 55 is complete. Detailed file-picker/drop/playback gestures were not separately verified; this change extends existing controls without adding layout.

The following hardware checks remain follow-up work outside the completed implementation scope:

- [ ] Physical GTX 1070 H.264/HEVC Main 10 NVDEC correctness, cancellation/device failure and performance with the rebuilt helper/runtime.
- [ ] Sustained uncached multi-source native desktop playback, first-use pinning latency and memory measurements on the reference hardware.

Workspace-wide tests and Clippy are deferred; focused crate/behavior checks and the desktop build are the acceptance scope of this change. User acceptance does not certify the unperformed hardware checks above.
