# Large still images and EXRs — 11 October 2026

User-directed continuation of common image import. This does not close a Motion
phase gate or begin general incremental execution work.

## Behavior

- Shared image geometry accepts up to 67,108,864 pixels (8192 × 8192).
  Import records the original dimensions. Native samples, source coordinates and
  authored formats are preserved; no automatic Reformat operation is inserted.
- Existing viewer-size/region sampling remains a transient evaluation request.
  Native-resolution rendering and EXR output preserve the original raster.
- Common image files no longer use the roughly 25 MiB encoded-file cap. Their
  encoded buffer, decoder scratch and decoded samples acquire memory leases.
  JPEG's additional encoded-input copy is accounted separately. PPM retains its
  geometry-derived payload/header validation.
- EXR inspection/decode no longer uses a 256 MiB file cap. Decoding reads a seekable
  buffered file and selects existing compression blocks/parts/channels. It does
  not copy the whole compressed file into RAM. Initial content hashes and source
  stamps protect against changed files; cancellation remains worker-side.
  Generic fingerprinting no longer imposes a 2 GiB image-file cap.
- PIZ and other non-ZIP decoder scratch estimates use compression-block geometry,
  not all pixels/channels of the image. Selected-channel output remains bounded.
- The fixed 64 MiB render-source ceiling is removed. Immutable sources already
  own shared decoded-memory leases; graph references do not allocate duplicate
  source bytes. Still retention and Motion image sampling use shares of configured
  CPU memory. CPU preview admission includes native source dimensions, even for
  a small output demand. Output/display buffers use a configured memory share.
- Automatic CPU image allowance on Linux is sampled once: the smaller of half
  physical memory or three quarters of available memory, bounded to 128 MiB–16 GiB.
  A visible cgroup-v2 memory ceiling is considered. Platforms without this telemetry
  retain the 768 MiB portable default; manual memory settings remain authoritative.
  One third covers decoded storage and two thirds working images. Output and other
  caches retain separately tracked allowances. Exhaustion reports an allocation
  failure; it does not silently resize the source.
- Shared desktop/headless GPU creation requests transfer buffers up to 1 GiB,
  within adapter capability. Transfer/readback checks device buffer limits before
  allocation. Oversized native-plane storage bindings use the existing RGBA upload
  representation. GPU CPU-adapter storage retains its memory lease through upload.
- EXR export accounts channel/scratch storage, writes with one bounded compressor,
  and checks cancellation while writing. Publication remains atomic.

## Automated evidence

Reference host: Linux, about 31.3 GiB physical RAM. Automatic CPU image allowance
was 16,002 MiB. Native hardware check: NVIDIA GeForce GTX 1070, 8 GiB VRAM.
Tests below use debug builds; their elapsed times are not interactive performance claims.

- `cargo test -p fold-media --test large_images -- --ignored --nocapture`: passed.
  Real 6144 × 4096 BMP above 25 MiB; 8192 × 8192 16-bit RGBA PNG with a distinct
  last pixel; 405 MiB, 4096 × 2160 EXR containing twelve float AOV channels.
  The EXR decoded its last AOV with a 768 MiB CPU image allowance and 300 MiB of
  working memory deliberately held: its compressed file exceeded available
  working memory, yet selective streamed decoding succeeded. Logical decoded peak
  across the test was 1096 MiB; working peak was 2051 MiB during the 8K PNG decode.
- `cargo test -p fold-render --features gpu --test large_images -- --ignored
  --nocapture --test-threads=1`: both passed. Native 8K CPU pixels, a 128-square
  preview, full-size display conversion, native-size half-float EXR export above
  512 MiB and export reopen/sample checks. The sandbox Vulkan adapter was llvmpipe;
  the GPU upload, native readback and reduced preview also passed there.
- Physical GPU rerun outside the device-isolated sandbox:
  `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/nvidia_icd.json
  target/debug/deps/large_images-2a353d9e220c2495
  native_gpu_eight_k_upload_preview_and_readback --ignored --exact --nocapture`:
  passed on the GTX 1070. Logical GPU peak 2064 MiB under a 4096 MiB test allowance.
  Pixel comparisons cover native corners/center and reduced preview; the source
  retains its 8192-square dimensions.
- `cargo test -p fold-media --lib --test raster --test exr --test exr_sequence
  --test thumbnails -- --test-threads=1`: 32 passed. Includes existing lossy/lossless
  EXR block parity, multipart/tiled reads, changed-source checks, cancellation,
  shared ticket ownership, color/alpha precision and bounded decode pressure.
- `cargo test -p fold-render --test graph -p fold-platform --test
  application_preferences -p fold-motion --test images`: 18 passed. Covers shared
  render lifetimes, preferences, image sampling, source time and persistence.
- `cargo check -p fold-app --features desktop`: passed on final implementation.
- Final thumbnail check: 3 passed after adjusting large-source scratch accounting.
- Native Dear ImGui settings capture: `settings::tests::playback_settings_native_normal_and_narrow`
  passed, run through the compiled UI test binary. The initial command used the
  wrong exact module prefix and selected zero tests; it was corrected before
  recording acceptance. Reviewed [normal](automatic-normal.png) and
  [narrow](automatic-narrow.png) automatic-budget text. It wraps cleanly, and the
  existing controls/scrolling remain available.
- Touched-file formatting and `git diff --check` pass. Diff review found no changes
  to authored resizing semantics or unrelated user artwork.

## Remaining limits and acceptance

Actual RAM, configured budgets, GPU texture/transfer capabilities and live graph
complexity still govern admission. Supporting one 8K source does not promise an
unbounded 8K multilayer graph on every machine. Low-memory machines may need explicit
memory settings or an artist-authored Reformat. Manual limits are not overridden.

The first-frame-only image policy, sRGB/ICC limitations, EXR channel/deep/subsampling
restrictions, and separate video codec/hardware limitations remain. This does not
qualify 8K spatial rendering or all video encoders. Other-OS automatic memory
telemetry, physical desktop artist interaction and production responsiveness remain
unverified. Existing GPU-only dead-code warnings in `gpu/yuv.rs` remain unrelated.
No workspace-wide test/Clippy checkpoint or Motion phase completion is claimed.
