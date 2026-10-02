# Phase 15A — Playback modes and uninterrupted viewer presentation

## Status and goal

Implemented and verified on the reference machine. Phase 15A and its playback-correctness checkpoint are complete on the automated, GPU/device, and native application-command evidence below. Native OS menu click-through is not independently certified because desktop input became unreliable; actual ImGui menu input is covered by focused tests. Existing phase numbers and historical acceptance remain unchanged.

Make slow compositor/motion previews usable without compromising real-time timeline playback. Separate document frame rate, playback policy, requested time, and presented-frame identity. Normal frame preparation must never flash loading UI or clear a previously valid image.

This refines the universal clock-driven and exact-current-key-only presentation contracts recorded in phases 11/15. It preserves independent viewers, exact rational time, shared bounded resources, generation safety, and export isolation.

## Implementation boundaries

- Use the existing CPU renderer, asynchronous preview worker, and shared presentation cache. Change admission/presentation only as needed to implement the agreed policies correctly.
- Keep application transport/render authority outside panels. Expose provider-owned defaults through a minimal feature-neutral capability/metadata contract; the generic shell must not inspect creative feature models.
- Follow the Fold UI skill before implementation of mode controls or presentation changes. Store playback preferences in versioned workspace state, not authoritative project content; restore paused.
- Do not implement range pre-rendering, a general scheduler/resource redesign, new decoding, or GPU evaluation here. Phases 17–19 retain those responsibilities.

## Tasks

1. Add independent per-viewer **Real-time** and **Every-frame** playback modes, compact mode selection, provider-driven defaults (timeline Real-time; compositor/motion Every-frame), explicit overrides, and workspace migration/persistence.
2. Define source-change/default-versus-override behavior and mode transitions before implementation. Rebase clocks/requests without unexpected jumps, cross-viewer changes, or unintended audio activation.
3. Keep Real-time playback audio/monotonic-clock-driven, allowing intermediate video demands to be skipped. Admit suitable completed frames without requiring every frame to meet an already-passed exact-time demand.
4. In Every-frame mode, prepare consecutive frames and advance the playhead with presentation. Slow down rather than skip frames when rendering misses the frame interval; pace fast/cache-hit frames at no more than document fps. Keep timing rational and UI work nonblocking.
5. Mute audio in Every-frame mode regardless of instantaneous rendering speed. Preserve explicit monitor ownership and define safe transitions back to Real-time; do not time-stretch or dynamically toggle audio as rendering speed varies.
6. Retain the last valid presented image until a suitable replacement is ready. Remove ordinary preparation text, ellipses/spinners, and intermediate image blanking from the viewer. Define initial-empty and actual-failure behavior separately; do not repeatedly replace the picture with diagnostics.
7. Separate requested time and presented-frame identity. Define held-image validity across seek, edit, retarget, source removal, mode switch, loop, and pause. Holding an image must not authorize publishing a retired request or misrepresent old content as newly evaluated content. Keep keyframe insertion and overlays associated with the correct explicit time/context.
8. Protect all held, referenced, and in-flight images against eviction. Preserve bounded shared work, independent request cancellation, fair progress for mixed-mode viewers, teardown, and pinned export isolation. Do not introduce per-viewer unbounded workers/caches.

## Acceptance criteria

- With deliberately slow rendering, Real-time playhead progression follows the clock while video may drop/repeat; the viewer does not flash preparation messages or blank between frames.
- Under the same conditions, Every-frame playback presents consecutive frames in order and advances the playhead with them. Rendering delay slows playback rather than skipping document frames.
- Fast rendering and cache hits cannot accelerate Every-frame playback beyond document fps. Fractional-rate pacing, range endpoints, and repeated loops do not accumulate timing drift.
- Seek, edit, retarget, pause, mode switch, and close reject obsolete result publication while obeying the declared held-image policy. Initial empty state and genuine failures are not confused with per-frame preparation.
- Two viewers, including two views of one document, can use different modes without cross-wiring time, presentation, audio, cancellation, or editor context.
- Every-frame audio stays muted; explicit Real-time monitoring and handover remain correct. Workspace restore preserves policy/override state and remains paused.
- Held-frame resource protection, cache pressure, UI responsiveness, bounded teardown, and export isolation pass focused tests.

## Verification

- Add deterministic clock/readiness tests using controlled slow and fast completions, stale generations, fractional rates, repeated loops, and cache hits. Avoid relying solely on wall-clock sleeps or lightweight scenes.
- Add focused integration/presentation tests for mixed-mode viewers, workspace migration, audio transitions, edit/overlay targeting, and held-frame eviction safety.
- Format touched files and check/test affected crates during iteration. At the slice checkpoint run workspace tests/checks, Clippy, and package-boundary checks; distinguish inherited failures.
- Run native slow-composition plus independent Real-time viewer acceptance, including mode controls, seek/loop, and uninterrupted picture presentation. Record actual evidence and any unavailable checks separately from model tests.
- Do not claim physical presentation timing, native acceptance, or sustained real-time throughput from headless/model tests. This phase establishes correctness under slow rendering, not phase-19 performance acceptance.

## Follow-on ownership

- Phase 17 preserves readiness, generation, and held-image ownership semantics in the GPU path.
- Phase 19 adds bounded review-range preparation and explicit cached Real-time replay with audio, budget/eviction behavior, interruption/invalidation, and mode-aware scheduling. No implicit playback-mode switch.
- Phase 21 hardens mixed-mode long-running workflows, cached-preview invalidation, and failure/resource-pressure recovery. A/V drift requirements apply to Real-time playback, not muted Every-frame playback. The separate short audio-review-range failure observed below is a named hardening obligation.

## Implemented policy and ownership

- `platform::desktop::PlaybackMode` and provider-owned output metadata define the two policies. Timeline providers default to Real-time; compositor and motion providers default to Every-frame. A viewer's optional workspace override survives retargeting; **Use source default** re-evaluates the new output's provider default. Workspace v3 upgrades v1/v2 with no override and restores paused. No project schema or render semantics changed.
- Mode selection is in the viewer context menu under **Playback mode**. The header shows the active mode and displayed-image time; the bottom transport remains the requested playhead. This distinguishes a deliberately held image without introducing a loading notification. Every-frame is explicitly labelled muted in the menu, and its monitored viewer does not show an active Audio header indicator.
- `app/src/viewer_transport.rs` owns request generations, readiness admission, and rational pacing. Real-time accepts suitable late frames without moving its clock backward. Every-frame requests the next consecutive frame, admits it no earlier than its rational deadline, and updates its playhead with admission. Slow completions rebase pacing rather than accumulating catch-up debt. The final frame dwells before a nonlooping transport stops; looping remains sequential across the range boundary.
- Explicit seek, pause, source/mode/range change, and changed render content retire the request generation. Mode changes preserve the current transport position subject to the existing playback-range rules and rebase pacing; source changes pause/reset marks and retain the existing absolute/nested-time mapping rules. Audio worker retirement occurs on entry to Every-frame; returning to Real-time uses the existing explicit monitor ownership and priming/handover service.
- `ui/src/preview.rs` keeps per-consumer admitted generations and held keys over the existing globally shared cache/serial worker. A late admitted render can be offered to the application even after a Real-time demand advances. Obsolete generations may populate reusable cache content but cannot replace a consumer's picture. Exact cache hits are still reusable across generations when they match the current demand.
- Ordinary preparation draws no message, ellipsis, spinner, or intermediate blank. Seek/edit/resolution changes within the same output hold the previous image until replacement; source/output change, source removal, and viewer closure clear that association. Initial-empty viewers have no fabricated image. Genuine render errors appear in chrome while a previously held image remains visible.
- Held and submitted textures remain protected until replacement/completion. Backpressure never waits on the UI thread or allocates beyond the existing cache budget. Under fully pinned pressure, work waits for resources; closing/retargeting consumers releases their holds. General cache reservations and range preparation remain phase 19, not an unbounded cache expansion here.
- Overlay interactions are suppressed during playback and whenever the held key differs from the exact paused edit context. Paused rational subframe time is preserved. Restored viewers now initialize their host transport even when their stored output already matches; a native restore check exposed this admission prerequisite and a regression test locks it down.
- The existing opt-in native baseline probe now reports actual held presentation/current-demand readiness and routes its seeks through the instance transport instead of reporting the worker's most recent upload as the visible image.

## Automated and device verification

All commands were offline on this machine. Desktop/audio builds used the existing local ALSA development files:
`PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig`.
The first desktop check without this environment failed because `alsa.pc` was unavailable; no dependency was installed or silently substituted. Two initial focused-suite builds hit their 200-second compilation timeout; the 600-second retry completed.

- `cargo test --workspace --all-features --offline -- --test-threads=1`: **180 passed, 0 failed, 2 ignored** (opt-in native audio and GPU tests). This checkpoint includes deterministic slow/fast readiness, 18,000 fractional-rate loop deadlines, stale-generation/content rejection, late Real-time admission, last-frame dwell, muted monitoring, provider defaults, exact paused edit context, restore/migration, export isolation, and held-image lifetime tests.
- After the final mode-menu and diagnostic refinements, `cargo test -p fold-ui --all-features --offline -- --test-threads=1`: **36 passed, 0 failed, 1 ignored**. The added actual ImGui mouse-input test selects Every-frame, Real-time, and source default at both 440- and 170-pixel parent widths. Existing normal/narrow shell and editor-context tests also pass. This is an affected-crate rerun, not a claim of rerunning the unchanged full workspace.
- `cargo test -p fold-ui --offline real_textures_hold -- --ignored --nocapture --test-threads=1`: **1 passed** on NVIDIA GTX 1070 / Vulkan / driver 580.173.02. Real registered GPU textures remain Ready over 100 waiting iterations, a late completion replaces the held image, and retired-generation completion cannot replace it. A two-texture budget protects held images and simulated in-flight references (controlled counters), allows eviction when those references complete, and releases holds on consumer closure. Native desktop runs separately exercise the actual queue-completion callbacks. This is a lifetime/presentation test, not GPU evaluation or throughput acceptance.
- `cargo test -p fold-app --all-features --test native_playback --offline -- --ignored --nocapture --test-threads=1`: **1 passed**. Six-second 48 kHz stereo device run: maximum clock-versus-monotonic drift **0.000551 s**, **0 underrun callbacks**, 175 video demands. Added monitored Real-time → Every-frame → Real-time coverage verifies readiness-gated time while muted, resumed device-clock progress, and rejection of the retired generation. Existing handover/close/teardown assertions pass.
- `cargo check -p fold-app --no-default-features --offline`, desktop builds, `python3 scripts/check_boundaries.py`, `cargo fmt --all -- --check`, and `git diff --check` passed.
- Workspace/all-target/all-feature Clippy completed with the recorded inherited warnings: four platform test initializer warnings, four UI test `drop_non_drop` warnings, one compositor `collapsible_if`, and ten Motion warnings (plus duplicate lib-test reports). Final affected-UI Clippy has only those four inherited UI-test warnings. No new-code warning remains.

## Native presentation evidence and limits

Machine-local fixtures, harness, logs, reports, and screenshots are outside the repository at `/home/rgame/Documents/fold-phase-15A/`. The native pipeline harness in `probe/` delegates to the real Session, renderer, shared GPU preview host, and desktop UI; it drives application transport commands rather than OS input. Its cold-result adapter delays delivery by at least 200 ms without blocking the UI. It is test equipment, not a production playback mode or range-preparation implementation.

- Final `native-probe-report.json` reports success with a composite referencing animated motion in Every-frame mode beside an independently playing audio-monitored sequence in Real-time. The composite admits **0,1,2,3,4,5,6,7** in order; seven cold inter-frame intervals span **7,642.968 ms** in this debug-build run. The Real-time viewer continues independently and may drop/repeat images.
- The same eight-frame loop then reuses the existing hot cache: **64 sequential admissions in 2.145 s** at a document rate of 30 fps (first-to-last admission span 2,104.145 ms). No sequence skips or clock-driven catch-up occurred in Every-frame. These are application admission timestamps, not physical scanout measurements or the phase-19 sustained workload benchmark.
- The harness switches both policies, pauses/seeks both viewers to frame 3, verifies both exact images, and asserts no audio-monitor errors throughout the final full-range audio run. The bounded harness exits after its observation interval; independent device tests cover teardown.
- `screenshots/Screenshot_2026-10-02_14-37-20.png` verifies both restored paused outputs render. `screenshots/Screenshot_2026-10-02_14-51-31.png` shows simultaneous slow-composite/Real-time playback with stable pictures and distinct image/playhead times. Narrow native viewer columns retain one-row chrome and bottom controls; long existing source labels can clip. Other pane/DPI redesign is not part of this phase.
- The tested COSMIC screenshot/libei route initially delivered a successful Play click. Later pointer commands timed out; captures confirmed no unintended action, and OS input was not blindly repeated or replaced with another backend. Therefore exhaustive native menu/keyboard/resize click-through is **not certified**. Actual menu choices and normal/narrow layout are covered by ImGui input tests; mixed playback, loop, mode switch, and seek are covered through native application commands and inspected screenshots.
- An initial native fixture restricted the monitored sequence to frames 0–7 and encountered **`Audio monitoring unavailable: invalid audio region`**, falling back to its monotonic clock. The unchanged audio preparation path sets `plan.end` without clipping longer regions. This is recorded separately, not hidden by the successful full-range rerun; investigate/fix shortened monitored ranges in phase 21. Every-frame muted review ranges and the native device mode-switch tests pass. No shortened-range A/V claim is made here.

## Deferred work

No range pre-render workflow, cache-budget expansion, retained decoder, ACES conversion, or GPU evaluation was added. Those remain phases 16–19. Ten-minute mixed-workload/A/V hardening, the separate shortened audio-range failure, and exhaustive native input/DPI coverage remain phase 21. Phase 15A establishes correct presentation and playback under slow rendering; it does not certify real-time rendering performance.
