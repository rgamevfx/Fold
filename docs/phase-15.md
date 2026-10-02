# Phase 15 — Independent viewer transport and request lifetimes

## Goal and boundaries
Each viewer owns playback, exact local time, review marks and looping. Source navigation, another viewer and pinned export must remain independent. Reuse the shared preview/cache path and one audio device worker; do not duplicate decoder budgets or move evaluation into panels. Follow phase-11 monitoring and source-switch policies and the Fold UI skill.

## Tasks
- Add instance-addressed transport configuration/status, independent monotonic clocks and a single explicit audio monitor with generation-safe handover.
- Route bottom transports and editor seeks through explicit viewer contexts; persist loop/monitor choices and restore paused.
- Keep shared content requests bounded and reject obsolete presentation without canceling another consumer's valid work.
- Test independent same-document playback, marks/loops, seek/pause/retarget/closure, monitoring, persistence and export isolation.

## Acceptance and verification
Run focused affected-crate tests, formatting/checks, then slice checks and native two-viewer workflow where possible. Record actual evidence and limits here. Do not mark complete on a spike, or claim native acceptance/real-time performance from headless tests.

## Implementation

- `crates/app/src/viewer_transport.rs` owns instance-addressed transport intent/status, independent integer/rational monotonic clocks, per-viewer review marks and loops. Up to 64 runtime clocks share one existing `Playback` audio worker; there are no per-viewer decoders/device threads. Paused subframe edit times remain exact. Monotonic looping uses the original anchor rather than accumulating frame-rounding error.
- Source changes pause/reset marks and clamp absolute time. Changes to a viewer's render content pause that consumer; organization-only changes retain content identity. Missing documents pause without selecting another source. Project replacement retires all runtime transports.
- Monitoring is explicit: the first newly created viewer is selected, focus/play/create do not steal it, closing the monitor clears it, and handover rebases only the outgoing clock before priming the incoming clock at its own time. Existing generation-tagged bounded SPSC/device handling supplies latency compensation, silence on retired generations and underrun diagnostics. Device failure is visible in the monitored viewer and falls back to its own monotonic clock.
- `PanelContext` returns local transport intent instead of navigating/seeking/playing the global host. The shell addresses that intent to the viewer instance. Existing explicit editor associations continue to supply keyframe/overlay/seek time. The obsolete single `playback_viewer` owner is removed. The legacy non-instance command adapter remains for headless clients, regression fixtures and the old native diagnostic; it cannot take the device while instance transports exist.
- Bottom transport controls remain in each viewer. Secondary **Loop playback** and **Monitor audio** choices are in the viewer context menu; the selected monitor has an **Audio** header indicator. Workspace sidecars persist loop/monitor/marks/time; runtime playing state is never serialized, so restore is paused. Invalid restored monitor IDs are cleared.
- `PreviewHost` tracks current exact presentation demands by viewer ID. The immutable content/output/time/resolution key acts as the presentation generation: returning to the same key intentionally reuses identical content. Equivalent demands share content work. One admitted request/CPU pending frame and one globally budgeted 256 MiB GPU cache remain shared. Consumer churn does not cancel another consumer's content work; obsolete content can finish into cache but is never resolved as that consumer's current presentation. When all demands disappear, the shared request is canceled. Round-robin admission avoids starvation. Export retains its separate committed snapshot/job lifetime.

## Verification evidence

Commands use the existing local ALSA environment:
`PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig`.

- Slice suite: `cargo test --workspace --all-features --offline -- --test-threads=1` — **171 passed, 0 failed, 1 ignored**, including doc tests. The ignored test is the opt-in native audio test, separately run below. The first invocation timed out during compilation at 200 seconds; the subsequent invocation completed. After adding the exact-subframe/repeated-loop regression and strengthening export/context assertions, the affected app library and `viewer_transport`/`panel_instances` suites were rerun: **16 passed**. This is not a claim of rerunning every unchanged suite.
- Native device: `cargo test -p fold-app --all-features --test native_playback --offline -- --ignored --nocapture --test-threads=1` — **1 passed**. Six-second stereo 48 kHz test, mixed 30/24 fps sources: maximum clock-vs-monotonic drift **0.000783 s**, **0 underrun callbacks**, 174 video demands. The added application-level test hands monitoring between two playing viewers without changing either time, closes the monitor without changing the other transport, asserts close under 100 ms and worker teardown under two seconds. The test passed those bounds; they are assertions, not separately logged timings. The ten-minute device run remains phase-21 hardening; the deterministic monotonic-clock test covers 600 simulated seconds.
- `cargo check -p fold-app --no-default-features --offline` passed; desktop compilation and tests passed.
- `cargo clippy --workspace --all-targets --all-features --offline` completed with inherited warnings: four platform test `field_reassign_with_default`, four UI test `drop_non_drop`, one compositor `collapsible_if`, and ten Motion warnings (plus duplicate lib-test reports). No new-code warning was emitted. No unrelated cleanup was made.
- `python3 scripts/check_boundaries.py` passed. Touched Rust files formatted; `git diff --check` passed.
- Tests cover same-document independent clocks, paused rational edit time, isolated seeks/marks/retarget/close, full-range and fractional-rate boundaries, nonaccumulating loop arithmetic, monitored device handover, inactive callback generations, current-key presentation rejection, shared request deduplication/fairness, paused workspace restore, explicit edit contexts, and a concurrently running pinned export completing after viewer lifecycle changes.

## Native UI evidence and acceptance

Personal test artifacts are outside the repository at
`/home/rgame/Documents/fold-phase-15/` (isolated workspace state, logs and screenshots).

The native desktop on the GTX 1070 restored two viewers of the same motion document at distinct exact times (**11/15 s** and **2/5 s**), rendered both frames, showed separate bottom transports, and displayed the restored monitor indicator. Both viewer panels were approximately 350 px wide in the 1280×720 application window. Screenshots:

- `screenshots/Screenshot_2026-10-02_12-54-11.png`
- `screenshots/Screenshot_2026-10-02_12-54-56.png`

The documented `wdotool --backend libei mousemove 382 529` invocation timed out (exit 124). The follow-up capture confirmed no interaction occurred. Input was not blindly retried or replaced with an unsupported backend. Consequently native simultaneous playback, loop/menu interaction, pin/follow changes, direct edits, close/reopen and the motion-plus-composite click-through are **not certified**. Automated/model/audio evidence does not stand in for that required native workflow.

### Demo and product-owner approval

A demo copy was subsequently launched at `/home/rgame/Documents/fold-phase-15/two-viewers.fold` with isolated workspace state under `demo-state/`. The left viewer follows Group A's video sequence and monitors audio; the right viewer follows Group B's separate animated title. Both restore paused with looping enabled. `screenshots/Screenshot_2026-10-02_13-17-19.png` verifies both distinct sources rendered with their own bottom transports.

After reviewing the running demo, the user explicitly approved completion: **“looks good. Mark this as complete and merge”**. This accepts phase 15 and the panel-navigation/viewer-instance slice checkpoint with the native automation limits recorded above. It is product-owner acceptance, not a claim that the agent independently exercised every native interaction or demonstrated real-time performance. The demonstrated sources were a sequence and a motion document, not the exact motion-plus-composite click-through originally specified.

**Status: complete and accepted by the user.** Phase 13's separately recorded acceptance gaps and phase-21 long-run hardening obligations are unchanged.
