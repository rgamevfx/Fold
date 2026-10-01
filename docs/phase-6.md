# Phase 6 — Media through the shared engine

## Goal and decisions

Connect import, exact-time scrubbing, preview caching, and range export through
one immutable-snapshot evaluator (product sections 5–8, 14, and 17).

The accepted viewer-cache decision is recorded in
[`adr/0001-viewer-cache.md`](adr/0001-viewer-cache.md): keep display-transformed
RGBA8 GPU textures, not a collection of linear RGBA16F/32F images or PPM files.
The initial PPM-only increment is retained for fixtures/image sequences; it is
not the primary video path and is not the cache format.

## Tasks and acceptance

- [x] Implement one explicit FFmpeg import/export profile, with unsupported input
  errors rather than guessed color/timing metadata.
- [x] Import source fingerprints and video-layer authoring in one undoable batch;
  preserve project data across save/reopen. Pin verified source copies for jobs.
- [x] Evaluate one or two layers through shared media/opacity/over IR at exact
  rational times. Verify linear-light blending and matching pre-encode output.
- [x] Add desktop import, save/open, undo/redo, opacity, frame scrubbing, preview
  resolution, export-range, and cancellation controls. All media/I/O runs on workers.
- [x] Add latest-request preview scheduling, bounded queues, stale-result rejection,
  independently cancellable export, and explicit pending/error states.
- [x] Implement byte-bounded GPU RGBA8 LRU caching with content/time/resolution/view
  identity, displayed-frame protection, and GPU-completion protection.
- [x] Test codec timing/color, source changes, cancellation, resource policy,
  persistence, and concurrent preview/export; run media-slice workspace checks.
- [x] Obtain user acceptance of the delivered media slice. The user reviewed the
  reported implementation, verification, and native-testing limits and explicitly
  requested completion and commit. Native import/render and a seek passed; export
  interaction, visible cache hits, and normal-close cleanup remain unverified by
  the agent and are retained as follow-up checks in `phase-6-checkpoint.md`.

Phase 6 and the media checkpoint are complete by user acceptance. This closes the
acceptance gate without claiming that the deferred native checks were executed
or that the architectural performance targets have been achieved.

## Supported profile

System prerequisites: `ffmpeg` and `ffprobe` on PATH, with `libx264` and `zscale`.
Verified against FFmpeg **6.1.1-3ubuntu5** on Linux. No FFmpeg binaries are bundled;
codec distribution/licensing and a broader OS/version matrix need release review.

- MP4-family container recognized by FFmpeg's MOV/MP4 demuxer, exactly one video
  stream, H.264, progressive `yuv420p`, left chroma location, square pixels.
- Explicit BT.709 primaries/transfer/matrix and limited (`tv`) range. Missing or
  different tags, rotation, VFR, and nonzero-origin timestamps are rejected.
- Frame PTS and durations are validated against an exact positive rational rate.
  Source intervals and export ranges are half-open. Decoder seeks use whole-second
  accurate input seeks plus an exact CFR frame offset, including fractional rates.
- One or two synchronized layers. Two-layer import currently requires identical
  dimensions, rate, and frame count. Foreground opacity is an explicit transaction.
- Audio is ignored on import and omitted from export, visibly disclosed in the UI.
  Synchronized audio/playback and richer clip editing remain phase 7.
- Input conversion is explicit BT.709 limited YUV → sRGB RGB8 → scene-linear
  premultiplied RGBA32F. Operations run in linear space. The SDR view transform is
  applied once; RGBA8 textures and the native surface use non-sRGB UNORM formats
  to avoid a second transfer conversion.
- Export evaluates original sources at full project resolution, converts its own
  engine result to RGB8 sRGB, then explicitly converts to limited BT.709 for
  H.264 `libx264`, CRF 18, `veryfast`, MP4. It never reads the viewer cache.
  Output is video-only and lossy; pre-encode comparisons are exact, encoded tests
  use declared RGB8 tolerances.

## Scheduling, ownership, and bounds

- One preview worker, at most one pending request and one completed display result.
  Replacing a request cancels its token, clears obsolete queued/results, and prevents
  stale publication under the same short mailbox lock. Evaluation runs outside it.
- One independent background job for import/open/save/export. A second background
  request is rejected rather than queued without bounds. Export pins one committed
  snapshot; later edits do not change it. Import/open proposals cannot overwrite a
  project that changed while the job ran.
- Codec processes are supervised: cancellation kills/reaps them even while an
  encoder pipe write blocks. Per-process timeout is 600 seconds; diagnostics are
  bounded. Save uses the existing atomic persistence path; cancellation cannot undo
  a save/export already finalized and the status must not claim otherwise.
- Preview cache: **256 MiB**, at most **512 entries**, LRU eligible eviction.
  Displayed entries remain pinned; submission callbacks protect textures still in
  GPU use. Upload waits for eligible space by retrying on later UI frames, not by
  waiting for GPU completion on the UI thread. No explicit destruction of resources
  still referenced by submitted work.
- Full/half/quarter preview resolution is explicit (half by default), included in
  cache identity and visible in the viewer. UI overlays are drawn afterward.
  Cache keys hash authoring data and ordered source fingerprints plus evaluator/
  conversion versions; exact time is the frame index plus the document's rational
  rate. Unrelated project revisions do not invalidate a result. View version is a
  separate key field; only SDR sRGB v1 is currently supported.
- CPU reference evaluation: at most 4,194,304 pixels and 4096 graph nodes; 64 MiB
  live working pixels in the original convenience entry point, **128 MiB** in the
  retained-worker entry point (enough for two 1080p layers with opacity/merge).
  Caller-owned RGB8 graph inputs have a separate 64 MiB bound. A video decode holds
  one temporary RGB8 frame; linear results are released after conversion/use, not
  stored in the viewer cache. Display/upload copies are separate allocations.
- Sources: at most **2 GiB per file**, **18,000 frames**, and two verified temporary
  source copies per decoder worker (evict before adding another). These are original
  compressed source bytes, not rendered-frame caches. Preview and export own
  separate decoders, so up to 8 GiB of pinned-source disk space is possible together.
  A fresh job refuses a changed fingerprint; an already pinned copy remains valid.
- Per-frame raw RGB decode output is temporary scratch and removed after use.
  Probe JSON is capped at 8 MiB; codec output at 2 GiB for an export. The process
  monitor polls limits rather than providing an OS sandbox/hard memory quota.
  Allocator/driver/codec heap overhead and overall process RSS are not covered by
  the image-buffer budgets. No production memory/performance guarantee is claimed.

## Verification

Passed:

- `cargo test --workspace --all-features --locked` (full media checkpoint suite),
  followed by focused tests for subsequent media/cache refinements.
- `cargo check --workspace --all-features --all-targets --locked`.
- `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`.
- `python3 scripts/check_boundaries.py`, touched-file formatting, and diff checks.
- Five real-codec workflow tests: fractional-rate boundary seeks/shuffled frames;
  grayscale and saturated colors; explicit unsupported-profile errors; import/
  undo/redo/save/reopen; two-source linear compositing; byte-exact preview versus
  pre-encode PPM; MP4 range count/rate/order and RGB8 tolerances (3–5 code values);
  source replacement versus pinned ownership; active encoder cancellation and
  removal of partial output; concurrent preview/export and nonblocking commands;
  unknown future-schema documents and extension fields surviving import/edits.
- Four PPM/media-sequence tests plus decoder tests; the existing graph/foundation
  tests remain green. These PPMs are fixtures, not cached viewport files.
- Cache policy tests: byte budgets, LRU ordering, protected entries, distinct
  content/time/resolution/view keys, and 10,000 seek/eviction iterations.
- Headless ImGui control/viewer construction at three window sizes.
- Real CLI import of two 640×360, 24 fps, 120-frame sources and full MP4 export.
  ffprobe verified H.264, 120 frames, 24/1, BT.709 limited, `yuv420p`.

Native evidence and the remaining acceptance gap are in `phase-6-checkpoint.md`.

## Usage

```sh
# Interactive shell, or immediately import one/two matching videos on its worker:
cargo run -p fold-app --features desktop --bin fold-desktop -- background.mp4 foreground.mp4
cargo run -p fold-app --features desktop --bin fold-desktop -- --project demo.fold

# Headless import and half-open range export:
cargo run -p fold-app --bin fold-cli -- import-video demo.fold background.mp4 foreground.mp4
cargo run -p fold-app --bin fold-cli -- export-video demo.fold output.mp4 0 120

# Exact-time still output, e.g. frame 1 at 24000/1001 fps:
cargo run -p fold-app --bin fold-cli -- render demo.fold output.ppm 640 360 1001 24000
```

CLI import requires a new project path; desktop import replaces the active layer
source document transactionally, preserving unrelated/unknown documents and assets.
Export refuses an existing destination, writes a same-directory temporary file,
and finalizes only after success. Desktop opacity changes use **Apply opacity**
for one undo entry. Media paths are currently absolute; relinking UI is not present.

## Explicit later work

Hardware decode/encode, GPU evaluation, native-plane decoded-frame caching,
intermediate caching, read-ahead/playback, BC7, proxies, persistent disk caches,
HDR, additional codecs/profiles, VFR, mismatched-layer timing, and relinking.
FFmpeg installation/library versions must remain stable during a job; a persisted
runtime toolchain lock and protection against mid-job toolchain replacement are
hardening work, not implemented environment pinning.
The cold path launches codec processes and uses CPU conversion/evaluation/upload;
it is a verified fallback, not a sustained real-time playback implementation.
Do not claim the architecture's 1080p30, 100 ms seek, or 16.7 ms UI P95 targets
without the corresponding measurements.
