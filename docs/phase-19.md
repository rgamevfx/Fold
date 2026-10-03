# Phase 19 — shared scheduling and bounded review preparation

## Goal and boundaries

Serve independent viewers, monitored audio, export and browser work within
declared aggregate limits. Preserve exact timing, immutable snapshots, explicit
playback modes, source verification, GPU ownership and preview/export semantics.
Presentation remains display-transformed SDR; export evaluates the pinned scene.
Existing user documentation edits are outside this implementation.

## Implementation tasks

1. Move preview demand ownership into a bounded per-consumer application queue;
   deduplicate equivalent work, retire obsolete generations independently and
   rotate consumers fairly. Bound completed-result retention.
2. Share desktop GPU host/accounting with export and reserve fair execution
   capacity. Keep audio independent of long graphics work; bound background work.
3. Audit content identity and source/decoded/geometry/intermediate/presentation
   accounting, including referenced and submitted leases. Preserve unrelated-edit
   reuse and apply backpressure before allocation.
4. Prepare a bounded selected review range through the same demand service.
   Expose preparation and explicit cached Real-time replay through existing viewer
   menus. Invalidate on seek/edit/retarget/range/color/quality changes; never switch
   modes implicitly. Report residency/eviction and stop cached replay on a miss.
5. Verify cancellation, fairness, deduplication, pressure recovery, multi-viewer
   modes, audio timing and native/reference-machine performance.

## Acceptance and verification

Focused affected-crate tests cover demand replacement, shared cancellation,
fairness, exact keys, held/in-flight retention, range bounds and eviction. Native
checks must exercise normal and narrow UI and two-viewer seek/play/edit while
ingest/export run. Measure sustained full-quality 1080p30 two-video/title/stereo
audio, UI P95 <16.7 ms and warm seeks <100 ms; report drops, underruns, queue/cache
statistics, transfers, memory/scratch peaks and export throughput. Cached replay
and short headless averages are separate evidence. Record unavailable checks and
missed targets explicitly; leave phase/checkpoint unchecked until acceptance.

## Evidence

Reference-machine acceptance is **complete through [phase 19A](phase-19A.md)**.
The original measurements below remain historical evidence of the throughput
failures; the corrective phase records the final implementation, repeated passing
measurements and checkpoint verification. Phase 19 and its checkpoint are checked.

### Shared execution and ownership

- `app/preview_worker.rs` owns a maximum of 128 demands, keyed by panel consumer
  and foreground/preparation role. Equivalent immutable keys share evaluation;
  completions carry captured consumer generations. Retiring one owner does not
  cancel another owner's work. Only one completed CPU/GPU result is retained.
- Viewer consumers rotate fairly. Four foreground evaluations reserve service
  for range preparation. The process-wide render scheduler rotates four viewer
  slots, one export slot and one preparation slot at frame boundaries, with FIFO
  order within each class. Audio and browser workers have separate lanes. This
  is bounded worker admission, not preemption of an already submitted GPU command
  or a slow codec call.
- Desktop delivery uses the viewer/compressor GPU host and its allocation budget.
  Each export retains its committed snapshot and explicit delivery selection;
  delivery evaluates scene content rather than presentation-cache images. One
  export can run beside one ingest/file job. Authoring proposals still commit on
  the application thread through transactions.
- Preview allocation/process pressure requeues live demand and yields off the UI
  thread, with a 30-second failure deadline. Cancellation releases waiting
  scheduler tickets. Submitted GPU leases remain protected until completion.
  Thumbnail capacity errors are retried on later browser demand rather than
  cached as permanent source failures.
  Export/import errors remain explicit; they are not silently retried indefinitely.
- Eight supervised media processes are allowed, with regular decode/export/
  browser work capped at six. Only monitored playback can use the two reserved
  audio slots. Audio callbacks still consume the bounded ring and never acquire
  scheduling permits or decode media.
- Content identity now includes serialized dependency references/output ports,
  asset IDs, source identity/input color, provider payload/version and evaluator
  version. Exact time, resolution, output and color view remain in presentation
  keys. Names, bins and workspace layout do not invalidate rendered content.

### Aggregate accounting

Reservations follow allocation lifetime, including caller-held decoded images,
temporary work, cache references and submitted GPU leases. Sharing decoded RGB/
float vectors does not allocate a second full-frame copy. The existing source
pinning and decoder-cache limits remain in force.

| Resource | Declared aggregate limit / policy |
| --- | --- |
| Verified immutable video source copies | 4 GiB disk; existing shared pin service |
| Decoded CPU planes/RGB/float images | 256 MiB, including leases outside decoder caches |
| Raw decoder-pipe buffers | 64 MiB, including queued/in-progress buffers |
| CPU float work and conversion/vector/blur scratch | 512 MiB |
| CPU encoded/display output | 64 MiB |
| Shared desktop GPU allocations | 512 MiB across working, presentation, decoded, geometry and scratch categories |
| Viewer presentation LRU | Existing 256 MiB within the shared GPU host for GPU-rendered frames; held/in-flight entries protected |
| Prepared stereo PCM | 512 MiB disk, reserved before codec output and shrunk after completion |
| Pinned WAVE copies | 512 MiB disk |
| Delivery intermediate/encoded files | 4 GiB disk, reserving permitted file ceilings before codec work |
| Delivery PCM | 256 MiB disk |
| Browser thumbnails/waveforms | One worker/result, existing 128-entry thumbnail cache |

These are logical engine-storage limits, not a total process RSS or physical
driver-memory cap. Codec/native-helper internals, driver allocations, UI/font
storage, geometry construction, metadata and allocator overhead require separate
process-tree/driver measurements. Headless jobs in other processes have their own
budgets; they do not share the desktop host.

Viewer menu → **Diagnostics** exposes current/peak queue and allocation data,
pressure retries, shared GPU categories and history retention. History reports
unique reachable state roots/document-payload capacity and external handles;
retired snapshots no longer reachable from history and metadata/allocator overhead
are excluded. Use RSS alongside these logical counters for acceptance.

### Review workflow

Pause a viewer, mark its range, then choose **Review cache → Prepare marked
range**. Preparation submits at most one next-frame demand per viewer through the
same shared worker. It uses exact document-rate times and never advances the
viewer. Admission is limited to 512 frames and conservative RGBA8 capacity with
current/replacement headroom; active preparation plans share that capacity. BC7
quality success is never assumed when promising range residency.

Status reports preparation, resident frames, readiness, capacity limits,
interruption and eviction. It overlays the image area without changing the
viewport resolution/key. Secondary controls and raw counters stay in menus.
Completed preparation does not endlessly refill evicted frames.

**Play cached Real-time preview** requires the existing Real-time mode and the
entire selected range resident. It starts at the marked in point using normal
transport and audio-monitor ownership. It never changes playback mode. A cache
miss stops explicit cached playback with an eviction message rather than silently
continuing uncached playback. Seek/edit/retarget/range/color/quality changes
interrupt preparation/replay; cancellation removes preparation intent and stops
its active cached transport. Regular Real-time dropping and Every-frame pacing
remain separate existing policies.

The shortened audio-review-range failure inherited from phase 15A is fixed as a
prerequisite: `AudioPlan::clip_end` clips/removes regions without rebasing their
source offsets. A focused regression covers clipping and source mapping. Native
short-range and long-running drift evidence are recorded separately below.

### Verification on this environment

All Cargo commands used `--offline`; multi-crate ImGui/codec runs were serialized.
The final affected production desktop code type-checks. ALSA development metadata
was initially absent; temporary metadata pointing at the installed 1.2.11 runtime
allowed early checks. The user then installed `libasound2-dev`, and the standard
desktop build/check works without that workaround. Repository dependencies were
not changed.

- Headless and GPU/native-video affected-crate checks passed, including
  `cargo check -p fold-app -p fold-ui --features fold-app/native-video,fold-ui/native-probe`.
- `cargo check -p fold-app --features desktop` passed with the installed ALSA
  development package, including playback and thumbnail code. The release
  desktop with `native-probe` builds successfully.
- Application library: **13 passed** headless; **19 passed** with desktop,
  including audio callback/clock and Every-frame monitoring behavior.
- Media library: **10 passed**, including caller-held budget recovery, pipe
  cancellation/reaping and reserved audio process capacity.
- Render library: **13 passed**, including scheduler fairness/cancellation,
  CPU allocation lifetime and shortened audio-range clipping.
- UI library: **43 passed, 2 opt-in tests ignored** with `native-probe` (41 passed
  in the default build). The actual ImGui menu test
  exercises review controls at normal (440 px) and narrow (170 px) widths,
  preparation while paused and explicit mode/readiness requirements. Range policy
  tests cover fractional time, capacity, eviction and invalidation.
- Project retention regression: **1 passed**. Panel-instance and project-ingest
  regressions: **10 passed**.
- Shared scheduling, viewer transport, audio and video integration tests:
  **13 passed**. The new CPU fixture verifies shared routing plus two seek
  consumers beside actual asset-only ingest and pinned export. This is a small
  correctness fixture, not native throughput evidence.
- Explicit Vulkan UI ownership/review test passed on **llvmpipe**, verifying
  held/in-flight textures, stale completion rejection, cached replay without
  decode demand, unchanged mode and stopping after eviction. GPU render
  operator/lifetime/budget test passed on llvmpipe, including category totals.
  Both UI opt-in tests (including compressor teardown) and the GPU render test
  also passed on the **GTX 1070** outside the sandbox.
- Native probe scheduling/report-safety regression and **4 Python measurement
  tests** passed. NVIDIA sampling counts only the measured process tree.
- Package-boundary/headless-isolation check passed. Touched files were formatted
  and `git diff --check` passed. Existing user documentation changes were preserved.
- The opt-in native playback test initially failed at `snd_pcm_open` with
  **Operation not permitted**, because the sandbox denies audio-device access.
  Its authorized run outside the sandbox passed: six seconds of 30/24 fps source
  playback into stereo 48 kHz PCM, **178 video demands**, maximum measured
  clock-vs-monotonic drift **0.000526 s**, maximum video decode **65.207 ms** and
  **zero underrun callbacks**. It also exercises seeks, monitor handover and
  explicit Every-frame muting. This short device test does not qualify sustained
  1080p30 or ten-minute A/V drift.

The extended native audio test also passed a short marked range (frames 5–7),
including the exact clipped end sample. One run subsequently failed its existing
two-second Session teardown deadline; a direct rerun of the same binary passed
in 7.96 seconds with zero underruns and 0.737 ms maximum measured clock drift.
The teardown failure is intermittent and its cause is not established; its
deadline was not relaxed. Phase 21 retains that native teardown obligation.

One earlier viewer-transport integration export exceeded its 15-second timeout
while another Vulkan build/test was running. Its isolated rerun and the closing
serialized integration run passed; no timeout was relaxed. This observation does
not establish a performance target.

### Native desktop measurement

The sandbox hides graphics/audio devices: its Vulkan adapter is llvmpipe and its
`nvidia-smi` fails. Authorized execution outside the sandbox sees the **GTX 1070,
NVIDIA 580.173.02**, and the native audio device. The driver is available; the
initial sandbox observation was not a hardware failure.

The release desktop `FOLD_SHARED_PROBE` driver uses ordinary UI draw/submission,
Session demands, transports, review cache and audio monitoring. It checks every
presented key is **1920×1080**, with two verified 30 fps videos, an animated
procedural title and stereo audio in the inherited 450-frame fixture. A cold
startup/warm-up is recorded separately from a **30-second warm playback** phase.
The second phase runs independent Real-time and Every-frame viewers while
asset-only ingest and a pinned 150-frame export execute. Later phases seek,
prepare/replay a 30-frame marked range with audio and verify seek interruption.
No render/effect/quality substitutions are made. Other desktop/GPU applications
remained active; diagnostic/logging/sampling overhead is included.

[Native measurement summary](phase-19-native.json) retains the numbers and
conditions plus binary/fixture/helper/config hashes; raw desktop/sample/resource
logs are in `/tmp/fold-phase19-final`. Final small fixes prevent duplicate work
when a consumer joins an active key, release delivery preparation PCM after it
is written, and wrap review status at narrow widths. Those have focused checks;
this failed-throughput measurement is not a requalification of the later fixes.

| Closing observation | Result |
| --- | --- |
| Cold startup/warm-up | 4.57 s |
| Warm single playing viewer / 30.00 s | 882 distinct admissions, about 29.4/s; 30 skipped frames between observed admissions, including wraps |
| Two playing viewers + ingest/export / 57.25 s | 199 Real-time admissions, 1,520 skipped frames; 131 Every-frame admissions with zero skips |
| UI draw-through-present-call P95 | 11.45 ms warm, 11.62 ms mixed; includes surface acquisition; cached review 13.79 ms |
| Warm presentation-cache seek repeats | 5.70, 4.74, 11.75 ms; target below 100 ms passes |
| First seek passes | 398.97, 259.10, 11.55 ms; the third frame was already held by the second viewer |
| Audio underrun callbacks | Maximum observed zero in all phases; unmonitored viewer reported no audio counter |
| 150-frame pinned export | Completion observed after 57.25 s, about 2.62 frames/s; ingest completed independently |
| 30-frame review preparation | 1.41 s; complete range resident |
| Cached Real-time review | 29 distinct admissions; no cache misses added; normal audio, unchanged mode; seek interruption passes |
| Shared engine GPU peak | 507.4 MiB / 512 MiB budget; presentation LRU about 253.1 MiB / 256 MiB |
| Sampled process-tree RSS peak | 1,239.4 MiB |
| Sampled NVIDIA process-tree memory peak | 1,479 MiB, including native helpers/driver allocations; 94 samples, no query errors |
| Sampled dedicated scratch peak | 103.4 MiB |
| Execution queue peak / pressure retries | 2 / 0; separate pressure-recovery fixtures pass |

Skipped counts are reconstructed from first-viewer sample sequences, including
450-frame wraps; they measure gaps between observed image admissions rather than
physical scanout drops. Sampled RSS may count shared pages repeatedly; driver
process counters are separate from logical host reservations. The nominal RSS/
scratch interval is 50 ms and GPU interval one second; enumeration/query overhead
and short peaks limit the measurements. Export timing includes queueing and
completion observation, not isolated encoder speed.
FFprobe verifies the completed file is 150 H.264 frames at 1920×1080, 30/1 fps,
with stereo 48 kHz AAC and a five-second duration.

Across 1,096 recorded preview evaluations, telemetry reports **92,447,008 bytes
of geometry uploads**, **6,817,996,800 bytes of GPU-local native video copies**,
zero viewer pixel readback/native pipe bytes and 52,608 status-readback bytes.
Delivery's explicit GPU-to-CPU encoder readback is separate from these viewer
counters. Final cache counters are 170 hits / 2,631 misses; preparation-to-replay
adds hits while misses remain fixed. These count key selections, not every draw.
The fixture's BC7 quality failures correctly retain RGBA8.

UI latency, warm seeks, bounded allocation and functional cached-review behavior
pass. **Sustained Real-time and mixed-consumer throughput do not pass** the
performance checkpoint. GPU shader work is around 5–7 ms in the telemetry while
mixed native codec service intervals often span hundreds of milliseconds; this
is an observation of inclusive service time, not a proven hardware-codec cause.
[Phase 19A](phase-19A.md) owns the required corrective work. No target revision
or performance acceptance is implied.

### Outstanding acceptance and reproduction

This original observation left phase 19/checkpoint unchecked; phase 19A subsequently
closed corrective acceptance with repeated native measurements.
Native normal/narrow **visual** review and DPI/physical scanout remain unverified:
the computer-use provider reported `runtime_unavailable` because Orca was not
running. The actual ImGui controls have normal/narrow interaction tests. Native
device/audio and shared desktop workflows were exercised independently of that
provider. Ten-minute A/V drift and intermittent teardown remain phase 21.

```sh
cargo build -p fold-app --release --features native-probe --bin fold-desktop --offline
# Use a NEW directory/report/scratch and a temporary copy of the fixture.
phase19_evidence=/tmp/fold-phase19-new
mkdir "$phase19_evidence"
cp /home/rgame/Documents/fold-native-pipeline/sustained/qualified.fold "$phase19_evidence/fixture.fold"
FOLD_SHARED_PROBE="$phase19_evidence/report.json" \
FOLD_COLOR_ROOT=/home/rgame/Documents/fold-native-pipeline/release-package \
FOLD_VIDEO_HELPER=/home/rgame/Documents/fold-native-pipeline/release-package/bin/fold-video-helper \
FOLD_VIDEO_DECODER=cuda-native FOLD_RENDER_BACKEND=gpu \
python3 scripts/measure_process.py --gpu \
  --report "$phase19_evidence/rss.json" --scratch "$phase19_evidence/scratch" \
  --timeout 180 -- target/release/fold-desktop --project "$phase19_evidence/fixture.fold"
```

Native commands must have graphics/audio access outside the device-isolating
sandbox. The probe is opt-in, uses a temporary project and never changes modes
implicitly in the product workflow. Sampling and phase reports fail explicitly
on unavailable hardware, errors or deadlines; incomplete reports never claim
success.

Full workspace suites, all-target Clippy and desktop/headless packaging are
deferred to the vertical-slice acceptance checkpoint, per repository instructions.
