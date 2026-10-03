# Phase 19A — sustained shared playback correction

## Goal and boundaries

Close the throughput failures measured in [phase 19](phase-19.md), preserving its
full-quality 1080p30 target, UI P95 below 16.7 ms and warm seeks below 100 ms.
The GTX 1070 desktop probe admitted 882 distinct images in 30 seconds after
warm-up; mixed viewers plus ingest/export admitted 199 Real-time and 131
Every-frame images over 57.25 seconds. Audio had zero observed underruns and UI
P95 stayed below 12 ms, but video throughput did not meet acceptance.

Keep exact time, immutable sources/snapshots, explicit modes/quality, normal audio
and shared allocation ownership. Do not hide uncached execution behind range
preparation or revise targets implicitly. Device isolation requires native runs
outside the sandbox; preserve raw evidence and failed observations.

## Tasks

1. Measure per-consumer queue wait, retained-range reuse/reopen, codec RPC and
   output-readiness intervals under alternating viewer times and pinned export.
   Distinguish inclusive host/service time from physical codec/GPU execution.
   Correlate request identities across preparation, decode/preroll, GPU submission,
   readback and presentation. Compare single-viewer, independent-viewer,
   viewer-plus-export and complete mixed workloads to isolate contention.
2. Investigate decoder range locality, serialized codec waits holding graphics
   admission, and export decode/readback/encoding contention. Select a fix from
   evidence; no cause is established solely by the current hundreds-of-ms codec
   service intervals beside 5–7 ms shader work.
3. Keep long decode/source preparation off the UI and prevent it from withholding
   current-frame graphics capacity. Bound any staging queues and reserve memory
   before work; retain fair export/preparation service and cancellation ownership.
   Prefer prepared-input staging ahead of graphics admission. Measure range-cursor
   demand by source and playback position before changing retention; respect shared
   helper-process/audio capacity. Investigate short bounded read-ahead, selective
   eviction instead of clearing all reusable inputs, and ready-work scheduling by
   deadline and measured cost rather than job counts alone. Pipeline export waits
   only where evidence requires it; retain ordered outputs and bounded buffers.
4. Verify fresh-frame and mixed-mode presentation, audio loops/handover, independent
   seek/edit cancellation, pressure recovery and pinned preview/export equivalence.
   Keep Every-frame slow without gaps and Real-time clock-driven without silent
   quality/effect changes.
5. Rerun the release native desktop probe and report cold/warm conditions, actual
   admissions/gaps, underruns, export progress, queue/cache counters, transfer bytes
   and logical/process-tree/driver/scratch peaks. Add direct visual normal/narrow
   UI checks when the computer-use provider is available.

## Acceptance

The original phase-19 performance checkpoint must pass on the reference machine.
Before the corrective benchmark, document the mixed-workload measurement window,
Real-time admission/gap criteria and minimum export-progress criterion. Resolve
ambiguities explicitly without lowering existing targets or fitting criteria to
the result. Preserve consecutive Every-frame output even when it runs slowly.
Cached replay, correct slow playback, reduced quality and short headless averages
are separate evidence. If targets still miss, name remaining corrective work or
obtain an explicit target revision; leave the checkpoint unchecked. Full workspace,
Clippy and packaging qualification remain the vertical-slice checkpoint obligation.

Close this corrective phase and phase 19's checkpoint before starting
[phase 19B](phase-19B.md). Corrections necessary to pass existing targets remain
here; 19B owns subsequent measured efficiency work. Hardware-encoder qualification
remains phase 20; ten-minute A/V drift and native teardown hardening remain phase 21.

## Status

Completed on the reference GTX 1070. Phase 19 and its performance checkpoint
close with the repeated measurements and qualification below; 19B remains planned.

## Corrective measurement protocol

The user approved increasing the engine GPU allocation budget during corrective
work: the default is now 1 GiB rather than phase 19's 512 MiB. The presentation
cache stays 256 MiB within that total. Report each run's budget and physical
driver/process memory separately; retain 512 MiB and smaller pressure fixtures.
This changes resource headroom, not the playback/quality acceptance targets.

Use `scripts/check_shared_probe.py` against the native desktop report. The warm
single-viewer window remains 30 seconds. The mixed window remains at least ten
seconds and continues through observed completion of the 150-frame export, with
the existing 60-second phase deadline. Require 30 Real-time admissions per second
in both windows, allowing only two partial boundary frames per window; report
all observed gaps separately. Every-frame must advance without gaps. Require
the export to complete inside the mixed deadline, zero observed audio underruns,
UI draw-through-present P95 below 16.7 ms, and all three warm seeks below 100 ms.
Workflow completion alone is not performance acceptance. Native visual checks,
resource/equivalence tests and checkpoint qualification remain additional gates.

Fresh baseline evidence is in `/tmp/fold-19a-baseline`: 890 admissions in 30.005 s,
then 199 in 56.988 s. A second run after the user closed another Fold instance
(`/tmp/fold-19a-isolated`) still misses: 883 in 30.006 s and 156 in 48.515 s.
The latter also times out in review preparation; preserve this failed observation.
The host process check before that run found no Fold desktop/helper processes.

The isolated native regression
`alternating_viewers_preserve_two_source_decode_locality` reproduces the locality
failure without desktop/export: 48 seeks in 48 requests, zero forward reuses,
416.607 ms mean per two-source frame after initial warm-up. It alternates positions
0 and 240, advancing each for twelve frames, on the existing two 1080p30 sources.
This identifies repeated seek/preroll as a cause of mixed-viewer decode cost;
it does not yet establish the contribution of scheduling or cache pressure.

### Earlier corrective probe and quieter viewer

The user requested removal of the changing viewer frame/source title. Removed
that title and its per-draw document-label collection; playback controls, errors,
and reduced-resolution indicators remain. `cargo check -p fold-ui --offline`
and the release native-probe build pass. Direct normal/narrow visual inspection
remains unverified: Orca reported `runtime_unavailable` after a launch attempt.

The subsequent 1 GiB probe (`/tmp/fold-19a-quiet/report.json`, accompanying
`rss.json` and `run.log`) includes decoder locality, staged graphics admission,
Real-time priority, one-frame lookahead and review-range eviction protection.
It completed the workflow, including review preparation, but failed performance
acceptance: 858 Real-time admissions in 30.001 s (28.60 fps); mixed load admitted
347 Real-time and 141 consecutive Every-frame images in 12.721 s (27.28 fps).
The 150-frame export completed in 12.721 s. Warm seeks were 4.50, 4.35 and
13.92 ms. UI draw P95 was 9.79 ms single-viewer and 16.91 ms mixed.

New inclusive host-stage measurements put mixed-load controls P95 at 1.28 ms,
viewer drawing at 1.60 ms, preview handling at 8.00 ms and UI encoding at
9.51 ms. These stage percentiles are not additive; they include host-side GPU
API waits and do not isolate GPU execution. The result does not isolate label
removal from the simultaneous scheduling changes. Investigate readiness,
submission contention and the single-viewer regression before accepting the
lookahead policy. At this stage, phase 19A and phase 19's checkpoint remained open.

Before the next corrective run, refine probe boundaries to measure sustained
playback: warm-up lasts at least five seconds and requires both the playing and
paused viewers to have presented an image. Previously it ended when only the
first viewer exceeded frame 12, leaving the second decoder's cold open inside
the warm window. Mixed load now adds Every-frame playback, ingest and export to
the already-running Real-time transport rather than resetting it to zero.
Explicit seek checks remain separate. The 30-second warm window, complete mixed
export window, 30 fps, UI P95, audio and quality criteria remain unchanged.
The preceding three-frame run showed a 244 ms cold second-position open and
112 ms loop decode seeks; test six-frame (200 ms at 30 fps) bounded preparation.

### Retained corrections and investigation history

- Two positions per pinned source reduced the isolated alternating two-source
  regression from 48 seeks/48 requests and 416.607 ms mean to four seeks,
  44 forward reuses and 12.231 ms mean. Process limits remain unchanged.
- Source preparation now precedes graphics admission; delivery readback occurs
  after that permit is released. Delivery retries bounded allocation pressure
  without changing the pinned request. Preview preparation is bounded to six
  frames ahead of the current clock, distinct from explicit review playback.
  A submitted result permits preparation of its successor before presentation;
  GPU readiness and generation/clock validation remain presentation gates.
- Current scheduling reserves non-Real-time foreground service after at most
  eight Real-time jobs; four foreground jobs still reserve review preparation
  service. Existing graphics/export/audio scheduling remains separately bounded.
- Duplicate device polling inside native source preparation was removed; the
  evaluator's completion pump and sticky device-loss checks remain. Native input
  allocation and ownership-submission host timings are now separately visible.
- Preview output metadata/content hashes are cached by retained immutable root
  identity, bounded to two roots and 128 entries per query category per root.
  Same-revision transient updates and undo have a focused invalidation test.
  Median mixed-load viewer UI work decreased from 0.975 to 0.375 ms, and preview
  handling from 0.583 to 0.144 ms; this did not alone close throughput acceptance.
- A batched native-ownership submission experiment passed receipt/lifetime tests
  but regressed end-to-end UI tail latency and did not meet throughput. It was
  removed; single-allocation submission and its original safety boundary remain.
- Unconditional UI redraw was measured around 142 Hz for 30 fps content. Automatic
  redraw now uses a 60 Hz baseline, raised for faster playing content, with
  immediate input redraw. An initially introduced indefinite event-loop wait was
  corrected: redraw deadlines remain armed through redraw dispatch. Single-viewer
  draw P95 fell to approximately 6–7 ms. Up to two already-ready results can be
  drained per draw without increasing the existing retained result capacity.
- A host-neutral result publication callback wakes the desktop immediately rather
  than waiting for the redraw timer. Its focused application test verifies that
  publication precedes notification without a UI polling loop and that teardown
  releases the notifier. Result collection defers new worker selection until
  successor demands are refreshed, avoiding a race that bypassed Real-time
  priority with an old queued Every-frame request.
- Audio looping now retains one device stream and a monotonic sample clock. The
  source plan wraps at marked boundaries; exact rational boundary calculation
  avoids cumulative sample rounding at fractional frame rates. Short loops prime
  across repeated boundaries. The previous stream restart skipped frames 0–2 at
  each full-range loop; continuous looping admitted 901 frames in 30.012 seconds
  with zero gaps in the first corrective run. Its mixed phase still missed
  throughput (354 admissions in 12.015 seconds); this is not acceptance evidence.
- New BC7 cache compression is deferred while any viewer is playing. A full
  lookahead buffer is reserved timing headroom, not permission to start optional
  compression. A repeat of the otherwise passing scheduling fix showed a single
  439.8 ms draw (436.6 ms in preview handling), losing eleven admissions. Mixed
  playback still passed with 374 admissions/12.460 seconds and zero gaps. Retain
  this failed repeat at `/tmp/fold-19a-compression-stall`; the first passing run
  at `/tmp/fold-19a-qualified-1` alone did not establish repeatability.

Probe reproducibility refinements: isolate `XDG_STATE_HOME` inside the evidence
folder; await asynchronous workspace restoration, configure exactly two viewers,
and reapply full resolution. A reused project path exposed restoration replacing
probe transports with paused saved state; that run is invalid performance
comparison evidence. Warm-up requires at least five seconds, an actually advancing
playing transport beyond 0.5 seconds, its current requested image presented, and
the second viewer initialized. This retains the original clock-advance condition
and prevents old held images or audio priming from counting as warm playback.

Additional raw failed runs remain under `/tmp/fold-19a-{ahead3,ahead6,overlap,timing,
poll,share8,metadata,batch,paced,drain-before-timer-fix,drain-restoration-race,
drain-isolated}`. Preserve these observations; do not infer acceptance from a
completed workflow or choose a passing run without reporting repeatability.


## Final performance qualification

Two consecutive unchanged release probes passed `scripts/check_shared_probe.py`.
Durable summaries, raw-report hashes, phase resources, native transfer totals and
failed corrective observations are in [phase-19A-native.json](phase-19A-native.json).
Raw reports, logs, scratch/RSS measurements and fixtures are preserved under
`/tmp/fold-19a-final-{1,2}`; the instrumented executable is
`/tmp/fold-19a-probe-desktop`.

| Measurement | Run 1 | Run 2 |
|---|---:|---:|
| Warm single Real-time admissions / seconds | 900 / 30.003 | 901 / 30.011 |
| Single observed gaps | 0 | 0 |
| Mixed Real-time admissions / seconds | 369 / 12.305 | 385 / 12.838 |
| Mixed Real-time observed gaps | 0 | 1 |
| Mixed Every-frame admissions / gaps | 180 / 0 | 201 / 0 |
| UI draw-through-present P95, single / mixed | 6.54 / 15.11 ms | 6.34 / 15.91 ms |
| Slowest of three warm seeks | 24.80 ms | 23.78 ms |
| 150-frame export completion | 12.305 s | 12.838 s |
| Observed audio underruns / errors | 0 / 0 | 0 / 0 |
| Sampled process-tree RSS peak | 1525.4 MiB | 1519.3 MiB |
| Sampled GPU process memory peak | 2040 MiB | 2040 MiB |
| Sampled scratch peak | 97.9 MiB | 103.4 MiB |

The 1 GiB engine budget is separate from driver/helper allocations; shared logical
GPU peaks remain below that budget (about 729 MiB). The 256 MiB presentation
cache is bounded; in-flight/held presentation leases remain separately accounted.
Both runs completed the seek/edit, review preparation and cached replay steps.
Normal preview still has zero video pipe traffic and zero image readback; geometry
uploads remain about 84 KB per fresh frame. Export performs explicit final-image
readback for the software encoder. The JSON retains totals and queue/cache counts.

The final diagnostic correction buffers per-frame GPU records in the existing
bounded sample report and writes them at completion. Earlier synchronous stderr
writes were on the UI path; rare long stalls persisted even with playback-time
compression disabled. The earlier stage-timing run had zero video gaps but missed
mixed UI P95 (16.98 ms). Buffering diagnostics preserved the same timing boundaries
and targets; the two final runs above are consecutive, not selected around failures.
BC7 compression remains paused-only: lookahead headroom is reserved for playback.

These are admissions and present-call measurements, not physical scanout. They do
not claim universally zero Real-time drops, ten-minute A/V drift, or clean-machine
hardware certification. Phase 19B starts from this measured baseline, including
metadata reuse, scheduling handoff and UI pacing; phase 20 still owns native
encoder input and phase 21 owns long-run drift and teardown hardening.


## Checkpoint verification and limits

- Workspace: **235 passed**, 32 opt-in native tests initially ignored. Headless
  `cargo check -p fold-app --offline` and package-boundary checks pass. Sixteen
  Python tests pass. Workspace/all-target Clippy completes with inherited media,
  platform/UI-test, native-probe, motion and compositor warnings; none introduced
  by this corrective implementation. Touched Rust files are formatted.
- All **32 native checks** pass under their stated conditions, including native
  source locality, software equivalence, receipt mismatch/early-drop ownership,
  allocation pressure/recovery, destroyed-device rejection, renderer admission
  cancellation, GPU color/alpha/vector operators, BC7 quality/lifetimes, held UI
  textures and continuous audio loops. The short audio test is not ten-minute
  drift qualification. Exact fractional loop mapping also has a 10,000-cycle unit
  test; the native loop test crosses short marked ranges without re-priming.
- Preserve two initial native qualification failures. The helper-supervision test
  exceeded its two-second assertion twice with the 31 MB background source; the
  assertion includes debug-build source copying/hashing, not just helper teardown.
  It passes with the existing 115 KB verified `sustained/qualified.mp4` fixture.
  No timeout target was relaxed. The device-loss test initially bypassed the new
  graph-level completion pump by calling a private source-preparation helper;
  it now exercises the public renderer entry point and verifies rejection before
  native decoder construction. That focused native rerun passes.
- Production package: `/tmp/fold-19a-package`, rebuilt desktop/CLI/helper with the
  existing hash-verified pinned OCIO/FFmpeg runtime. All **45 package-file hashes**
  verify. Without resource/helper path overrides, relocated CPU and native GPU
  CLI renders of the full 1920×1080 workload differ by at most **one SDR code**.
  This is same-host relocation, not a clean-machine installation certificate.
- Native production desktop launch and normal (1280×800) / narrow (800×650)
  window captures succeed through direct X11 window capture. Viewer frame/source
  titles are removed; mode, audio and reduced-quality labels remain. The normal
  capture precedes cold media readiness; the narrow capture includes the image.
  At an approximately 230-pixel viewer width, the existing transport row clips
  at the right; broader transport overflow cleanup is outside this correction.
  Captures are `/tmp/fold-19a-ui/{normal,narrow}.png`. Orca reports screenshots
  unsupported and root-window capture fails, so neither is claimed as validation.
  Synthetic Alt-F4 was unverified; the inspection process was cleaned up. Native
  probe completion and the explicit compression-worker teardown test do pass.

Validation logs remain `/tmp/fold-19a-{workspace,native-tests,native-remaining,
supervision-repeat,supervision-small,device-loss,clippy,headless-check,
python-tests,boundaries,production-final}.log`. Failed and corrected observations
remain in the evidence rather than being removed from the record.
