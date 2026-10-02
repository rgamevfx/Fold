# Phase 11 — native release baseline

This closes the native measurement gate in [phase 11](phase-11.md). It is a
baseline of the **existing CPU-render / GPU-present path**, not production GPU
evaluation, independent viewers, real-time playback or creative-workflow acceptance.

## Reproduce

Generate the phase-11 media/project using `scripts/phase11_baseline.py` first.
Build the opt-in diagnostic feature (normal `desktop` builds exclude the hooks):

```sh
cargo build -p fold-app --bin fold-desktop --features native-probe --release --locked
FOLD_NATIVE_PROBE=/absolute/new-native-report.json \
python3 scripts/measure_process.py \
  --report /absolute/new-resource-report.json \
  --scratch /absolute/new-scratch --timeout 100 -- \
  target/release/fold-desktop --project /absolute/fixture/baseline.fold
```

On this machine the build used its existing external ALSA development package:
`PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig`.
No machine-specific path was added to Cargo configuration. Report paths must be
new. `native-probe` alone does nothing without `FOLD_NATIVE_PROBE`; together they
force full-resolution preview, seek through the fixture and close this window
automatically. They do not save, export, change project content or interact with
other open Fold processes. An incomplete/aborted run records `success: false`.

The diagnostic runs the ordinary desktop client, PreviewHost, ImGui rendering,
texture upload/cache and swapchain; there is no substitute presentation loop.
It requests frames 0–9, revisits 0–9 from the RGBA8 cache, then requests cold frame
12 and replaces it with cold frame 29 after at least 50 ms. It rejects a cancellation
fixture that already completed and any stale ready image after replacement.
Frame 29 stays visible for twelve seconds, then the diagnostic exits. Limits:
90 seconds / 6,000 samples, plus the external process-tree timeout.

Unit coverage exercises the schedule, no-clobber report creation, cancellation
preconditions, stale-ready rejection, seek-only commands and incomplete reports.
The probe is diagnostic infrastructure, not a new product workflow or panel.

## Conditions and artifacts

2026-10-01, same reference machine/codecs as the headless baseline:
GTX 1070, driver 580.173.02, **Vulkan**; COSMIC Wayland, wgpu 27.0.1.
Release build from `43eccebedf26b388fced27a459b3298882e5b808` plus phase-11 probes.
Measured executable SHA-256:
`a8bf30aecec3af315180cadeee3e5f37a3aefcc036a8a5e7e43f3777789435ee`.
Surface: 1280×800, Rgba8Unorm, FIFO, desired maximum frame latency 2, scale 1.
Working evaluation and uploaded display frames: **1920×1080**, 8,294,400 bytes
per RGBA8 upload. The viewer fits this full-resolution texture into a smaller
panel; it does not render at the panel's small on-screen dimensions.

Normal background applications and an existing debug Fold remained running.
No concurrent compilation ran during the measurements. No filesystem caches were
flushed. The two-layer/title fixture contains audio, but these paused seek tests
never start audio playback or test the audio device.

All raw traces, build/check logs and screenshots are outside the repository at
`/home/rgame/Documents/fold-phase-11/native/`. Runs 1/2 were preliminary repetitions;
run 3 includes the final diagnostics and stale-frame assertions. All three finished
successfully. Files for the final run:

- `run3.json`: 2,952 redraw samples, command/present events and completion observations.
- `run3.log`: adapter, surface configuration and step transitions.
- `run3-resources.json`: process-tree RSS/scratch measurements; exit 0, no timeout.
- `run3-captures.json`: screenshot start/end wall times and triggering events.
- `run3-cancel_replace/Screenshot_2026-10-01_20-50-50.png`: frame entry **29**,
  empty viewer with “Waiting for the current frame” (the font displays the final
  glyph imperfectly); no stale frame shown.
- `run3-first_present_call/Screenshot_2026-10-01_20-50-51.png`: frame **29**, two-layer
  test pattern and procedural Fold title visible. Both screenshots were inspected.

Captures used the machine's tested `cosmic-screenshot --interactive=false
--notify=false --save-dir=<new-directory>` route, with a 20-second timeout. An
external watcher launched captures on the cancellation and replacement-present
log events. No desktop keyboard/mouse input or unrelated window manipulation was
needed. The watcher and machine-specific capture artifacts remain outside the repo.

## Final results

Milliseconds; P95 is nearest-rank over the stated sample count, not a confidence
interval. Small cold/upload sample P95 values are effectively maxima.

| Observation | Samples | Mean | P95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Surface acquisition, host | 2,952 | 0.060 | 0.076 | 0.685 |
| Desktop draw, acquisition through present return | 2,952 | 0.705 | 0.940 | 5.456 |
| Queue submit, host | 2,952 | 0.146 | 0.187 | 1.471 |
| Present notification + call, host | 2,952 | 0.083 | 0.105 | 1.500 |
| Submission return → completion callback observation | 2,951 | 6.648 | 7.209 | 24.228 |
| Texture allocation/write/registration, host | 11 | 2.501 | 4.508 | 4.508 |
| Upload start → shared upload/UI completion observation | 11 | 7.581 | 9.939 | 9.939 |
| Uncached presentation seek → first present call, frames 1–9 | 9 | 370.688 | 705.955 | 705.955 |
| Cached presentation seek → first present call, frames 0–9 | 10 | 6.765 | 7.324 | 7.324 |

There were exactly eleven uploads: frames 0–9 and 29. Cache revisits uploaded
nothing, and cancelled frame 12 was neither uploaded nor presented. “Uncached”
means absent from the **presentation cache**; alternating requests reuse decoder
read-ahead, explaining the large spread. Do not call all nine seeks cold decode.

Draw-start interval mean/P95: 6.962/7.495 ms. This includes time outside the draw
function, unlike the 0.940 ms draw P95. Neither metric measures input-to-photon
latency or sustained video throughput. Most samples redraw an unchanged image.

Whole process: 21.453 s, 271 resource samples, sampled process-tree RSS peak
523,714,560 bytes (~499.45 MiB), scratch peak 14,671,322 bytes (~13.99 MiB).
RSS may double-count shared pages; 50 ms nominal sampling may miss peaks. These
numbers include the diagnostic buffers and do not establish aggregate bounds.
Preliminary runs had cold-seek means 363.809/361.062 ms and cached-seek means
6.990/7.144 ms; resource variation is not evidence of a leak.

### Cancellation and actual visibility

Replacement of frame 12 by frame 29 occurred while frame 12 was pending:

- Next pending-state present call returned **6.756 ms** after cancellation marker.
- Inspected desktop capture proved frame 29's pending state visible by **70.281 ms**.
- Replacement frame 29's first present call returned **755.431 ms** after its seek
  command (the cancellation log marker follows that command by about 0.05 ms).
- Inspected desktop capture proved the replacement image visible by **829.237 ms**
  after the cancellation marker. Frame 12 never appeared ready in the trace.

Screenshot numbers are conservative **upper bounds**, calculated from command
wall time and capture completion, not exact compositor presentation timestamps.
This demonstrates prompt removal of a stale image and eventual correct replacement;
it does not assert that cancelled worker work stopped in 6.756 ms. The separate
headless stage probe measures cancellation-to-worker-join for source preparation.

### Measurement limits

`queue.on_submitted_work_done` is observed during subsequent nonblocking device
polling. Therefore completion timings bound already-finished work; they are not
GPU timestamp-query durations. Upload and ImGui drawing intentionally remain in
the same normal submission; their GPU durations are not artificially separated
with a blocking wait or extra submission. One final completion was unobserved at
shutdown and is **null**, not zero, in the trace.

`present()` returning does not establish Wayland compositor scanout. Screenshots
provide separate visible-state evidence; physical scanout/input-to-photon timing
remains a later performance-instrumentation task, not an invented phase-11 result.
Instrumentation allocations, step logging and completion callbacks add overhead.
No new widgets or presentation semantics were introduced; DPI/narrow-window,
keyboard, creative interaction, audio/drift and two-viewer acceptance remain with
their owning later phases.

**Conclusion:** the native baseline gate is satisfied. Cached presentation is
fast in this small fixture, while uncached results remain dominated by CPU/media
work. This does not meet or revise the later sustained 1080p30 product target.
