# Media slice checkpoint — verification record

## Goal

Preview and export two composited media sources through the same engine without
blocking UI input. Verify cancellation and bounded caching before phase 7.

## Status

**Complete by user acceptance.** After reviewing the implementation, verification,
and native-testing limits, the user explicitly requested that phase 6 be marked
complete and committed. Automated integration checks and native preview/seek passed.
The remaining native checks below are accepted as follow-up verification, not
claimed as executed. No desktop scrubbing, cache, or range-export feature is deferred.

## Evidence collected

### Automated

- Workspace all-feature tests/checks, strict Clippy, and package-boundary checks.
- Real FFmpeg fixtures at 24000/1001 and 24 fps, including shuffled seeks across
  whole-second/keyframe-seek boundaries, saturated colors, grayscale, half-open
  range export, encoded frame count/order, and explicit unsupported-profile errors.
- Two-source linear-light composition and exact display/pre-encode PPM agreement.
- Import as one transaction; undo/redo, save/reopen, content identity independent
  of whole-project revision, changed-source rejection, and retained-source pinning.
- Active encoder cancellation removes partial output. Preview request replacement
  only presents the latest requested key. Preview runs while export is active;
  nonblocking command/poll calls in the fixture are asserted below 100 ms. This is
  not a measured native input-to-presentation latency or UI frame-time percentile.
- LRU byte/entry bounds, pinned eligibility, key distinctions, 10,000 cache-policy
  iterations, and headless ImGui controls/viewer construction at three sizes.

### Native desktop

Reference session: Linux `7.1.5-76070105-generic`, COSMIC/Wayland, Intel i7-8750H
(12 logical CPUs), 31 GiB RAM. PCI hardware lists a GTX 1070 Mobile. The adapter
used for this initial run was not logged; current builds log selected wgpu adapter
and driver information on startup. FFmpeg is `6.1.1-3ubuntu5`, with libx264/zscale.
This was a debug build, CPU evaluation and CPU-to-GPU upload, not GPU evaluation.

Personal artifacts are outside the repository:
`/home/rgame/Documents/agent-desktop-research/fold-phase6/`.

- Generated `background.mp4` (testsrc2) and `foreground.mp4` (SMPTE bars), each
  640×360, 24 fps, 120 frames, tagged limited BT.709, H.264/yuv420p, square pixels.
- Desktop imported both on its worker and displayed their 0.5-opacity composite.
  `Screenshot_2026-09-30_23-06-29.png`: frame 0, half-resolution preview, 0.2 MiB
  RGBA8 cache, one miss. No large linear-frame cache was retained.
- A native seek to frame 60 changed the composite correctly.
  `Screenshot_2026-09-30_23-13-34.png`: frame 60, two misses, 0.4 MiB texture cache.
- Input used the prescribed `wdotool --backend libei` route. Multiple pointer/click
  invocations timed out during device setup; screenshots were inspected before
  retries. Window positions/focus subsequently changed, so desktop control was
  paused to avoid acting on the wrong window. No alternate input backend was used.
- The test window was left available rather than forcibly terminating it. The
  native observations predate final validation/diagnostic/UI-label refinements;
  rerun the final build for the remaining acceptance checks.

### Actual headless delivery measurement

Imported those same two files with `fold-cli import-video`, then exported [0,120)
with `fold-cli export-video`. ffprobe confirmed 120 frames, 24/1, 640×360, H.264,
`yuv420p`, limited BT.709, progressive, left chroma. Output is video-only.

`export-measurement.txt` records `/usr/bin/time -v`: **42.63 s wall time**, **76,732
KiB maximum resident set size reported by time**, exit 0. Filesystem caches were
not flushed; decoder handles started cold. This is about 2.8 frames/s for the debug
CPU/process fallback, not a real-time claim and not an aggregate simultaneous
RSS trace of all subprocesses. An idle desktop `ps` sample was 496,340 KiB RSS;
that includes the native UI/driver runtime and is not the 0.4 MiB cache counter.

The 1080p30 playback, <100 ms warm seek, and <16.7 ms UI P95 architectural targets
have **not** been established. Do not substitute this small-fixture export result
for those measurements. Cold-path overhead includes per-frame codec processes,
temporary RGB scratch I/O, conversion, CPU evaluation, and upload.

## Deferred native verification

Use a newly built executable (`cargo build -p fold-app --features desktop --bins`).
A quiet desktop is needed because the portal tool uses absolute coordinates.

1. Open/import the two supported fixtures through the desktop controls. Confirm
   the frame label, expected composite, and half-resolution dimensions.
2. Seek to a different frame and back. Confirm a **cache hit** without a new miss;
   switch full/half/quarter and verify separate cache entries and correct labels.
3. Start an MP4 export to a fresh destination. While it is running, scrub, edit
   an input field, and change preview resolution. Verify native input remains
   responsive, the latest frame wins, and no old result is shown as current.
4. Confirm successful output status and inspect timing/frame count with ffprobe.
   Run a second export, cancel it, and confirm no final/partial destination remains.
5. Save/reopen and undo/redo through the desktop controls. Close the window normally;
   verify codec jobs terminate and retained source files/resources are released.
6. Record selected GPU/driver, cache residency/hits, and native responsiveness
   evidence. Append the results here when these follow-up checks are performed;
   the tracker already records user acceptance, not completion of these checks.
