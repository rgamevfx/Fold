# Consolidation pass

The code-level consolidation and automated integration checkpoint are implemented.
**Native workflow acceptance and the product's real-time performance gates are
not complete.** Missing motion features remain phase-9 feature work rather than
being silently included in, or claimed complete by, this cleanup.

## State and editing

- Added coordinator-issued `CommittedSnapshot`; save, application export and
  playback require it. Read-only `Snapshot` is evaluation-only. Compile-fail
  tests prevent accidentally persisting/exporting a transient handle.
- Removed the parallel `Project::preview` mechanism. Desktop preview overlays
  use `EditSession`, immutable retained worker snapshots and stale-session checks.
  Final commands still commit one history entry.
- Extracted compositor/motion inspector lifecycle into shared `EditResponse`.
- Added stable `UiId` inspector scopes and `NumericProperty` metadata/controls
  with property identity independent of labels, units, step, optional limits and
  reset. Motion scalar/vector/color and compositor transform fields use them.
  Specialized controls retain the trusted ImGui escape hatch; no large widget
  framework or speculative public extension schema was introduced.

## Compilation, capabilities and delivery

- Replaced four overlapping video-provider compilation methods with one
  `VideoCompile` request. Registry convenience calls and cancellable job calls
  use that same provider interface.
- Propagated cancellation through dependency validation, provider dispatch,
  nested resolution, motion node/field evaluation and scene traversal. Scene
  traversal also has a finite work budget, including empty/nested groups.
- Moved timed video metadata out of mandatory document registration into the
  video capability. Audio validation no longer requires a video output.
- Registered legacy two-layer documents through the normal timeline capability
  registry and removed the application's parallel legacy compilation path.
- Added persisted, undoable delivery-output selection (`fold.output`). The Delivery
  panel can select the viewed document and displays its actual output range.
  Navigation no longer resets export bounds to a nested viewer's duration.
- Export queries the selected document's audio capability instead of assuming
  the first timeline owns delivery. An invalid explicit output never falls back;
  old projects retain their historical selection convention until explicitly set.
- Separated authored output validation from the verified MP4/H.264 restrictions.
  Procedural documents support odd raster sizes and durations beyond 18000 frames;
  media import and encoded ranges still enforce the supported profile. Output-time
  multiplication is checked; authored frame counts fit the current signed UI index.

## Reuse and measured optimization

- Motion document/video providers share bounded validated immutable roots and
  canonical content identities. Cache invalidation uses the complete document;
  eviction does not affect retained workers, and reference overrides never mutate
  a shared root. Parsing/evaluation does not hold the cache mutex.
- Audio playback retains its preparation decoder across seeks/restarts. Preflight
  releases PCM sources absent from the next plan before applying existing limits.
  Streams, presentation rings and generation checks remain per playback request.
- Added a reproducible 1080p two-layer/title/audio headless execution probe and a
  sampled Linux process-tree RSS/scratch-disk measurement tool.
- Measured conversion overhead justified exact input-transfer and output-quantization
  tables instead of per-pixel powers. Tests compare the original formulas, all
  RGB8 inputs, quantization boundaries and varied f32 values.
- See `consolidation-performance.md` for measurements, commands, environment,
  limitations and external artifact locations. No quality approximation or
  claim of sustained 1080p30 was introduced.

## Final automated checkpoint

- `cargo test --workspace --all-features --locked`: **124 passed, 1 ignored**.
  The ignored test opens a native audio device and remains intentionally manual.
  Tests include actual codec/export workflows, output selection/persistence/undo,
  stale preview rejection, one-entry gestures, optional video capabilities,
  cancellation inheritance, prepared-root invalidation/eviction and transfer
  equivalence. Synthetic ImGui tests cover property identity and ordinary drawing.
- `python3 scripts/check_boundaries.py`: passed, including headless UI isolation.
- Python regression suite: **12 passed**, including measurement timeout/output
  protection and resource sampling.
- `cargo fmt --all --check` and `git diff --check`: passed.
- Desktop all-target check: passed.
- Workspace/all-target/all-feature Clippy: completed with **9 existing motion
  style/type-complexity warnings**. No new warning remains from this pass.

The first workspace test attempt exhausted a 120-second timeout during compilation;
it was rerun with a longer timeout and completed. Desktop checks used the machine's
existing external ALSA development package; no machine-specific dependency paths
were added to repository configuration.

## Explicit limits and follow-up gates

1. Native interaction, focus, direct authoring, audible playback and long-run A/V
   drift still need coordinated user acceptance. No desktop input was performed.
2. The CPU/process fallback still misses the 1080p30 target. Retained/range-oriented
   video decoding and eventual GPU evaluation require further execution work;
   conversion improvements and audio reuse do not solve that bottleneck.
3. Per-module budgets are enforced and process-tree usage is now measurable, but
   a unified CPU/GPU/disk reservation manager is not implemented. The short probe
   does not certify repeated edit/seek/export memory behavior or UI P95 latency.
4. The prepared-root cache is not a persistent static-geometry cache. The minimal
   property controls are not the full public SDK or motion property-stack UI.
5. Complete the remaining motion features/acceptance in `phase-9.md` separately.

These are product/execution acceptance limits, not evidence that the entire
architecture or creative models should be rewritten. Keep the CPU reference
renderer and existing ownership seams while addressing measured bottlenecks.
