# Phase 7 — multi-track timeline editing and synchronized audio

## Status: complete

The corrected scope is a minimal Premiere/Resolve-style multi-track NLE, not a clip-parameter interface. The timeline is a statically linked first-party package registering document, command, video/audio evaluation, and custom-panel contributions. Runtime plugin loading and a durable native ABI are not part of this phase.

## Completed work

- [x] Ordered video/audio tracks, stable clip IDs, linked A/V groups, standalone music, track visibility/mute/solo/lock, clip opacity/gain, and shared video compositing/audio mixing.
- [x] Transactional move, compatible cross-track move, trim, split, lift, and unlink. Same-track overlaps, unavailable source handles, incompatible destinations, locked edits, and stale proposals are rejected before publication. Unknown extension data is preserved.
- [x] Custom Dear ImGui canvas: fixed headers, ruler, clip rectangles and edge hit targets, playhead, snapping, zoom/pan, horizontal scrollbar, vertical scrolling, and keyboard editing. Drags preview locally; release submits one revision-checked transaction; Escape cancels without history.
- [x] Timeline-owned media/selection inspector, append and playhead imports, destination-track selection, linked MP4 import, and independent PCM16 WAVE import. Imports preserve existing edits.
- [x] Minimal shared package/panel registration and generic host command dispatch. Timeline UI is optional and uses the shared UI SDK. Project/render infrastructure and the shared shell do not own timeline models; headless builds exclude ImGui and CPAL.
- [x] Worker-prepared audio, device-clock transport, latency compensation, generation-safe seek replacement, bounded buffering, and underrun diagnostics. Silent plans use a monotonic clock without requiring an audio device.
- [x] Automated model, registration, canvas interaction, codec, persistence, rendering, mixing, and export checks; native visual inspection and user-run interaction acceptance.

## Editing behavior and use

The default sequence starts with V1/V2 and A1/A2. Higher video tracks composite over lower tracks. Audio tracks mix independently; solo restricts playback to enabled solo tracks. E enables video, M mutes audio, S solos audio, and L locks edits. Additional tracks can be added from the toolbar and empty unlocked tracks removed from the inspector.

Import MP4 footage or PCM16 WAVE from **Media / Selection**. Select a track header to target imports; choose **Append media** or **Place at playhead**. MP4 sources with audio produce separate linked video/audio clips; video-only sources do not create phantom audio clips. **Audio only** imports video sound independently.

Drag clip bodies to move and edges to trim. Linked editing is on by default. Disable **Linked** or hold Alt when starting a drag to edit independently and dissolve the link. Shift temporarily bypasses snapping. Ctrl+wheel zooms around the pointer, Shift+wheel pans, wheel scrolls tracks, and middle-drag pans. The ruler seeks. Space toggles playback; S or Ctrl+K splits; Delete lifts; arrows step frames; Home/End seek to sequence bounds. Undo/redo uses project history.

Save/open/export remain background jobs under **Project / Delivery**. The inspector offers precise frame-boundary trims and clip opacity/gain, supplementing rather than replacing direct manipulation.

```sh
cargo run -p fold-app -- import-timeline new.fold a.mp4 b.mp4
cargo run -p fold-app -- edit-timeline new.fold 0 split 24
cargo run -p fold-app -- edit-timeline new.fold 0 trim 2 20
cargo run -p fold-app -- export-video new.fold output.mp4 0 48
cargo run -p fold-app --features desktop --bin fold-desktop -- --project new.fold
```

CLI clip indices refer to document clip-array order, including audio clips. `fold-cli command <project.fold> <registered-command-id> <json-arguments>` exposes registered commands headlessly. Desktop media arguments default to timeline import; `--layers` retains legacy import. Legacy layer documents remain video-only and preserved; a supported sequence takes precedence as active output. General document selection/nesting belongs to later work.

## Contracts and budgets

- Sequence schema 2: exact rational, half-open placement/source ranges, explicit track/link IDs and source metadata. The uncommitted schema-1 prototype is not migrated. Unknown/future project documents remain preserved, not silently rewritten.
- Bounds: 32 tracks, 4,096 clips, 1 MiB authoring payload, ten-minute duration, and the existing 18,000-frame video profile. Supported video remains H.264/MP4 SDR BT.709 CFR; WAVE import supports PCM16 mono/stereo at 8–192 kHz.
- Audio evaluation is stereo float32 at 48 kHz. Project boundaries round up to samples; source offsets round down after exact rational subtraction. Adjacent splits share one boundary and preserve source alignment. Preview and range export consume the same evaluation semantics.
- The CPAL callback performs no decode, allocation, or locking. A worker owns preparation and a tagged one-second SPSC ring, with half a second primed before playback. Missing/late samples become silence; device timestamps compensate reported output latency. Timestamp jitter cannot reverse the playhead within a generation.
- Seek/pause/edit invalidates old generations, destroys the old stream, and primes a replacement. Already submitted hardware audio remains subject to device flush latency. Device failures are reported rather than hidden by fallback to another clock.
- Retained PCM is bounded to 128 sources / 512 MiB. RGB read-ahead retains multiple source/resolution batches within 512 frames / 32 MiB. Media preparation and decode remain worker-only. Video requests may be dropped without stalling the audio clock.
- Export stages video and exact-range PCM, muxes H.264/AAC, and publishes without clobbering an existing output. Cancellation/failure does not publish a partial final file.
- The static contribution contract validates package/API/dependency compatibility and contribution ownership/uniqueness. Commands stage immutable-snapshot proposals; the host project coordinator alone commits history. Custom panels use shared semantic colors and docking. Raw ImGui exposure is an explicitly unstable trusted-editor escape hatch, not a plugin ABI promise.

## Verification

### Automated

- `cargo test --workspace --all-features`: passes (opt-in native device tests remain ignored by this command).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passes.
- `cargo check -p fold-app --no-default-features`: passes.
- `cargo fmt --all -- --check`, `git diff --check`: pass.
- `python3 scripts/check_boundaries.py` and `python3 scripts/test_boundaries.py`: pass, including nine boundary-checker regression tests and headless UI isolation.
- Model tests cover linked move/trim/split/lift, cross-track movement, locks, overlap/kind rejection, unlinking, exact fractional/sub-sample boundaries, source handles, stale transactions, immutable snapshots, extension preservation, and undo/redo.
- Actual ImGui canvas input tests cover linked selection, cross-track dragging, no intermediate history, rejected overlapping drops, linked edge trim, Escape cancellation, and ruler seeking. Separate tests cover pointer-anchored zoom and handle hit testing.
- Registry tests cover incompatible/duplicate contribution rejection, staging without mutation, stale/cancelled commands, and history. An independent test package contributes a panel and dispatches its registered command without timeline or shell-specific code.
- `nle_workflow` builds a two-video/two-audio-track scene through registered commands: linked shots, translucent cutaway, and independent music. It checks split undo/redo, solo/mute/mix behavior, save/reopen, and one-second range export. Decoded H.264 output is compared to preview with mean byte error below 3; decoded AAC is compared to the shared mix with MSE below 0.0001.
- Other codec/callback tests cover 44.1→48 kHz resampling, mixed video rates, silent gaps/missing audio, pinning/metadata checks, cancellation, underflow silence, late blocks, obsolete generations, and monotonic timestamp correction.

### Native and user verification

The agent launched and visually inspected the multi-track project on COSMIC/Wayland. The custom canvas displayed both video lanes, linked audio, independent music, ruler, track controls, and package inspector correctly. Desktop input automation intermittently timed out while initializing the libei virtual pointer, so it was not used as proof of successful native dragging.

The user then ran the requested interaction checks and reported: **“It looks good. Everything is working.”** This is recorded as user verification of linked movement and undo/redo, cross-track movement, linked trimming, Escape cancellation, overlap rejection, and ruler seek/playback/pause. Automated tests independently cover edit/save/reopen/export; no physical speaker/display synchronization measurement is claimed.

The earlier ten-minute native audio run passed on the same transport infrastructure: 30 fps output, 30/24 fps sources, stereo 48 kHz, 16,713 video demands, maximum clock-versus-monotonic drift **4.109 ms**, and zero underruns. The first run exposed a backward timestamp correction; a regression test and monotonic clamp fixed it. Worst individual video decode was **16.692 seconds**, so this is a clock-isolation result, not a video-performance pass. The long run predates the multi-track correction and was not repeated as a multi-track benchmark.

Reference machine: Intel i7-8750H, 32 GiB RAM, GTX 1070 Mobile, Linux/COSMIC/Wayland, PipeWire 1.6.8 analog stereo at 48 kHz, debug Rust build, FFmpeg/libx264. Personal evidence is outside the repository under `~/Documents/fold-phase-7-verification/`; `drift-fixed.log` records the long run, and `multitrack/` contains the generated scene, export, native log, and screenshots.

To retain the automated reference scene for manual testing:

```sh
FOLD_NLE_ARTIFACT_DIR=/absolute/test-artifact-directory \
  cargo test -p fold-app --test nle_workflow
```

Use a dedicated test directory: generated fixtures are replaced on rerun. Native drift testing remains opt-in:

```sh
FOLD_NATIVE_SECONDS=600 cargo test -p fold-app --features desktop \
  --test native_playback -- --ignored --nocapture
```

Linux desktop builds require ALSA development files. On the reference machine, an externally extracted `libasound2-dev` package was selected through `PKG_CONFIG_PATH`; no machine-specific dependency paths were added to the repository.

## Deferred work and limitations

Transitions, ripple/roll tools, waveforms/thumbnails, time stretching, nested documents, and runtime plugin loading are not part of phase 7. Nested composition is phase 8. Device hotplug/recovery, other device formats/platforms, partial-source low-latency preparation, sustained high-resolution decode performance, and physical A/V calibration remain hardening work. Current native output requires stereo float32 at 48 kHz; whole-source PCM preparation can delay priming. These limits do not change the completed minimal multi-track editing acceptance scope.
