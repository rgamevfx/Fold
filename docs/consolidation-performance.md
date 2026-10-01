# Consolidation execution probe

## Scope and reproducibility

This is a **release-build headless probe**, not native UI/audio acceptance or a
claim of sustained 1080p30. The fixture contains two overlapping 1920×1080 30/1
H.264 sources, foreground opacity 0.5, a procedural text title with glyph wave,
and stereo audio. Each source is one second long. Ten frames are evaluated
sequentially, then the same ten frames again with the same decoder. Audio
preparation is measured twice with one retained decoder; one offline block is
also evaluated. No audio device is opened.

Environment: Linux 7.1.5-76070105-generic; Intel i7-8750H, 12 logical CPUs;
Rust 1.95.0; FFmpeg 6.1.1-3ubuntu5. Rendering is CPU-only, with no GPU upload or
presentation. Filesystem caches were not flushed. Sources are small generated
fixtures, not large camera originals. Measurements are single runs with normal
machine activity; they are useful for bottleneck identification, not statistical
acceptance. Background OS load and codec startup vary.

Tools added:

- `crates/app/examples/execution_probe.rs`: builds an editable project from two
  supplied videos, pins its selected output and logs compilation, decode/render,
  display conversion and audio preparation separately.
- `scripts/measure_process.py`: samples Linux process-tree RSS and a dedicated
  scratch directory. It sums RSS (shared pages can be counted more than once),
  can miss short peaks, and reports nominal rather than guaranteed 50 ms sampling.
  Scratch bytes are on-disk file lengths, not resident memory. It imposes a timeout
  and does not overwrite reports or delete user files.

```sh
cargo build -p fold-app --example execution_probe --release --no-default-features --locked
python3 scripts/measure_process.py \
  --report /absolute/new-report.json --scratch /absolute/new-scratch \
  --timeout 180 -- target/release/examples/execution_probe \
  /absolute/background.mp4 /absolute/overlay.mp4 /absolute/new-probe.fold 10
```

Use the verified SDR profile (limited BT.709, H.264/yuv420p, progressive,
zero-origin CFR, square pixels), with stereo audio on at least the background.
Fixture generation and inspection artifacts from this run are outside the repo:
`/home/rgame/Documents/fold-consolidation-probe-PphM9Y/`.

## Measurements and resulting changes

The initial run already includes cancellation propagation, prepared motion roots,
and retained audio preparation. It precedes the transfer-conversion optimizations.
A second intermediate run changed only output quantization; the final run includes
both input and output transfer optimizations.

Mean time per frame, milliseconds:

| Stage | Initial first pass | Initial repeat | Final first pass | Final repeat |
| --- | ---: | ---: | ---: | ---: |
| Compile (including motion evaluation) | 0.18 | 0.19 | 0.19 | 0.22 |
| Decode plus CPU render | 431.71 | 385.88 | 337.32 | 274.51 |
| Display conversion | 84.49 | 83.78 | 44.64 | 46.67 |

The repeat pass is **not a fully warm frame-cache test**: ten two-layer 1080p
RGB8 frame pairs exceed the decoder's 32 MiB read-ahead cache. It intentionally
shows bounded-cache behavior rather than claiming every seek hits a cache.

- Initial/final whole-process wall time: 10.76 / 7.98 seconds, including import,
  fixture construction, save, audio preparation and both video passes.
- Final sampled aggregate process-tree RSS peak: 287,535,104 bytes (~274 MiB).
  Earlier runs sampled ~212–218 MiB. Do not interpret this sampling variation as
  a proven memory regression or improvement.
- Final sampled scratch peak: 14,398,565 bytes (~13.7 MiB).
- Final cold/reused audio preparation: 383.76 / 0.0065 ms. Retained PCM: one source,
  385,024 disk bytes. This isolates reuse, not hardware playback latency.

The observed conversion cost justified two small changes:

1. RGB8 input conversion now retains all 256 exact reference transfer values.
   Tests compare every possible input against the original formula.
2. RGB8 output quantization uses thresholds built from the original floating-point
   transfer on this environment. Tests cover each quantization boundary and
   100,000 varied f32 bit patterns against the original formula. No lower-quality
   color approximation or changed render semantics was introduced.

## Remaining bottleneck and acceptance

These results **do not meet the 1080p30 target**. Video decode still launches
FFmpeg for small read-ahead batches and uses temporary RGB files; CPU render and
display conversion are also well above a 33.3 ms frame budget. Replacing that
adapter with retained/range-oriented decoding is a further execution project,
not something that can be justified as complete by these cleanup changes.

Still unmeasured: native UI P95, GPU upload, warm viewer-texture seeks, simultaneous
export/playback, long-source preparation, sustained playback, ten-minute A/V drift,
and repeated edit/seek memory pressure. Per-module budgets remain explicit, but
there is no unified CPU/GPU/disk reservation manager. This probe does not establish
an aggregate hard allocation limit.
