# Phase 56 completion evidence — 2026-10-07

The six authorized implementation/qualification tasks are delivered. This extends the qualified first slice; it does not rerun or alter the frozen GPT-Sol-6.1 trial. The original artist edit to `demos/timeline-playground.fold` was left alone.

## Implementation

- Retained timing: clamped progress, cubic easing, periodic phase, per-instance stagger/translation, scalar mixing and exact rational-time native keyframes. Responsive contain-fit and anchor alignment follow evaluated geometry.
- Artist interfaces: scalar drag ranges, descriptions, sections and advanced controls in source, public port edits and native inspectors. Drag limits are soft metadata; source can explicitly clamp evaluation.
- Large assets: bounded full-construction diagnostic fingerprints, plus direct private instance-input overrides for control-response checks. Returned element samples remain bounded. Images/effects and unresolved varying fields remain inconclusive; geometric changes do not prove pixel visibility.
- Explicit atomic rename/remove/type-replacement migration. Rename retains bindings, visibility and native keys. Removal/replacement explicitly discard obsolete edits. Ordinary nested instances migrate; coordinated dependent source-owned programs are rejected rather than silently rewritten.
- Portable `.foldasset` bundles: transitive native groups, source/identity history, controls/defaults and fonts; bounded file I/O, stable-ID conflict rejection and unknown bundle extensions. Public `asset` import creates a Composition layer in one undoable transaction; contextual native load/save works on instances. Import does not execute author-time source. External scene/media/document references must be exposed as inputs; instance-specific edits remain in the project.

## Checks

36 focused tests passed across nine program tests, eleven graph/verification tests, fourteen application transport/asset tests and two native UI/source-lifecycle tests. Coverage includes exact timing, live geometry fitting, migration failure atomicity, native key identities, bundle persistence/conflicts/extensions, public export retry safety, one-step import undo, source preservation, cancellation and stale worker results. The keyframe test initially used a nonexistent circle node kind; its corrected ellipse fixture and per-instance stagger assertions pass. Public probe harness argument/response-shape errors were corrected before the successful recorded run.

Release desktop, CLI and profiling example build passed. [Executable hashes](release-build.json) identify the desktop/profile-tested build. The [final rebuild receipt](final-build.json) records the subsequent formatting/diagnostic-message cleanup, public catalog smoke check and exact saved-Motion reopen; no renderer or authoring semantics changed in that cleanup. Affected Motion/app Clippy completed with nonfatal style warnings ([log](clippy.log)); this is not a warning-free Clippy claim. Touched Rust formatting and diff whitespace review passed. Workspace-wide tests were not repeated.

The rebuilt release [public MCP probe](public_probe.py) verifies 5,000 copies / 15,002 evaluated elements / 30,000 path segments, complete fingerprints and a meaningful control response despite truncated element details. It exports/imports the asset, undoes the import, and creates the desktop fixture. [Result](public-result.json), [public transcript](public-transcript.json).

## Actual desktop

Orca automation remained unavailable. An isolated Xvfb X11 display, isolated runtime/config and disposable projects allowed testing the real release desktop using xdotool input and ffmpeg capture. The GPU was accessible outside the process sandbox: **NVIDIA GTX 1070, Vulkan, driver 580.173.02**, shared Fold GPU backend ([startup log](desktop-backend.log)). Earlier sandbox-only llvmpipe/no-device observations did not establish that the host lacked a GPU.

Verified through visible UI and public read-only state snapshots:

1. Select the generated asset and open its Rust editor; replace source and compile. Project revision 4 → 5.
2. Change Radius from 12 to 18 in the inspector. Revision 6.
3. Edit source and press Compile, then Cancel compile. Revision and complete inspected Motion data remained unchanged. Recovery compilation succeeded at revision 7 with the artist value retained.
4. Native Edit-menu Undo restored the exact precompile Motion data (revision 8); Redo restored the compiled result (revision 9).
5. Add a radius key with the native parameter affordance; see its track in Composition. Edit that key's value, then compile another source revision. Exact root graph, key identity/tangents and inputs survived (revision 12). Composition and source viewers have independent bound clocks; the recorded key edit is at source time zero, not a claimed second key at the scrubbed Composition time.
6. Native Save cleared dirty state. Reopening in a fresh public session reproduced the entire inspected Motion data exactly ([receipt](desktop-reopen.json)).

[Assertions](desktop-assertions.json) and `desktop-*.json` retain the state evidence. [UI captures](ui/) cover normal/narrow controls, source and load/save windows, plus full desktop source, key tracks, migration controls and artwork. Source-window migration controls were visually reviewed; automated migration behavior and public packaging cover the actions. This does not certify every native dialog gesture, Wayland behavior or every DPI setting.

## Rendering and playback

[Summary](render-summary.json) and raw logs distinguish three runs at the saved animation's native 500×888 size:

| Run | Frames | Mean ready | P95 ready | CPU parity |
|---|---:|---:|---:|---|
| llvmpipe Vulkan | 150 | 426.5 ms | 501.8 ms | maximum 1 code value |
| GTX 1070 with concurrent parity workload | 150 | 54.5 ms | 65.4 ms | maximum 1 code value |
| GTX 1070 sustained, no concurrent CPU parity | 300 | 28.7 ms | 33.7 ms | separate full-cycle run above |

The sustained run spans two six-second cycles and excludes viewer caching. Mean full wall time was 35.7 ms. GPU preparation/readiness fitting a 40 ms frame budget is not a blanket end-to-end desktop guarantee.

Actual desktop playback was also captured. [Cold playback](desktop-playback.mp4) advances about 29 frames from recording second 2 to second 5 (roughly 10 fps), while the cache fills. [Warm playback](desktop-warm-playback.mp4) advances from frame 51 to 126 over the same three-second interval: **25 fps** with a full visible cache bar. The displayed viewer is 140×250 fit-to-panel; the independent renderer timings above are full 500×888. These videos establish live movement and warm playback in this X11 virtual-display setup, not physical-monitor latency. Cold desktop playback remains a measured performance follow-up.

## Boundaries

No arbitrary Rust decompilation, source/manual-graph merge, coordinated multi-source interface migration, external-media archive packaging or universal performance guarantee is claimed. Native files and compiler limits remain documented in the [public asset guide](../../../../skills/fold/references/rust-assets.md). The user requested marking the phase complete and committing it on 2026-10-07. This acceptance does not expand the recorded hardware/Wayland/DPI coverage.
