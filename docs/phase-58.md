# Phase 58 — Tiered playback caches and resource settings

Status: complete at the user’s request on 7 October 2026. Engineering checks pass; sustained real-project performance and manual application testing remain post-phase verification follow-ups, not claims of completed testing.

## Goal and decisions

Keep timeline editing responsive while compositions and subclips are inspected or
recached. Add a dedicated viewer RAM allowance and independent timeline/composition
disk allowances (including 20 GiB and larger user choices). Preserve clean package
boundaries, direct code, exact render identities, and the current fast GPU path.

- GPU presentation is the hot tier. A freshly rendered GPU display lease goes
  directly to the viewer, without CPU readback, disk I/O, or waiting for retention.
- Viewer RAM holds display-ready pixels, separate from CPU decoded/working image
  memory. The latter retains its existing one-third/two-thirds allocation.
- Disk retention is persistent and disposable, with separate timeline and
  composition stores, folders, byte limits and clear actions. Subclip/source and
  composition viewing share the composition allowance. Neither store evicts the
  other. Default disk caching is opt-in; no surprise multi-gigabyte writes.
- Viewport/ROI and requested resolution remain part of cache identity, together
  with content, document/output, exact time, channels and display policy. A
  changed view requests its own entry, never presents mismatched pixels as ready,
  and does not purge old views or reusable source/working caches. Fold's existing
  source-build fingerprint also enters content identity, preventing persistent
  reuse after evaluator implementation changes without hashing files at runtime.
- Initial disk entries retain those exact preview requests. They are not
  full-resolution delivery sources. Resizing cannot delete existing timeline
  work, but a new region/resolution may need rendering. Canonical full-frame
  timeline preparation and spatial reuse across different requests require an
  explicit subsequent slice; do not silently force full-frame renders or invent
  pixels outside a cached ROI. This distinguishes retention from render coverage.
- BC7 is the explicit lossy playback-retention format (user decision). Remove
  the previous quality-comparison pass and its status readback. Unsupported BC7
  devices keep direct GPU presentation and report retention unavailable.
- Export continues to evaluate scene data; SDR display caches never feed delivery.
- Retention is opportunistic during idle time. A bounded worker does GPU readback
  and disk I/O; no framebuffer reads or filesystem work on the UI thread. Pause
  retention during playback/foreground demand; never queue every rendered frame.
- Cache hits bypass graph evaluation, including lookahead. A busy/slow restore
  never launches a duplicate render. Only an actual missing/invalid entry allows
  normal rendering; explicit cached-only playback stops on eviction. Submitted
  work from an earlier demand may complete safely but no replacement work is queued.
- Cache only the final displayed output/channel visualization: RGB, alpha, depth,
  reflections, AO, etc. Never cache all underlying channels. Switching output or
  channel makes old entries ineligible and resets matching coverage; retain old
  variants for switching back until ordinary eviction. Alpha/depth visualizations
  are display images, not raw planes. BC7 never substitutes for data inspection
  precision or scene-linear delivery.
- Cold lower-tier restores are asynchronous and bounded. GPU cache hits retain
  their existing path. Lookahead may restore upcoming frames. Missing/corrupt
  disk entries fall back to rendering. Held/submitted GPU resources remain
  protected; retention never blocks eviction waiting for a write.
- The engine owns pixel transfer/format validation and GPU allocation accounting.
  Platform owns persistent cache policy; UI owns viewer requests and presentation.
  Feature packages remain independent. Classification comes through an explicit
  desktop service, not feature-specific models in the renderer.
- No new general cache framework, dependencies, or speculative task graph. Cache
  limits are bytes, not a hidden frame-count ceiling. Disk metadata has its own
  explicit bound. File names are opaque; validated keys and payload headers guard
  against collisions, stale formats and corrupt data. Clear deletes only owned
  cache files, never source media or unrelated files.
- Settings show GPU presentation, viewer RAM, CPU image processing, timeline disk
  and composition disk separately, with numeric entry and units. Missing settings fields take current defaults; no legacy cache formats,
  migration adapters or parallel old/new workers are retained. Automatic memory defaults do not override
  explicit disk choices. Startup-only changes are labelled honestly.

## Implementation slices

- [x] 58A: bounded renderer presentation transfer/restore, using BC7 for retained playback pixels and
  uncompressed RGBA8 for immediate display; completion-safe lifetimes and transfer accounting.
- [x] 58B: byte-budgeted viewer RAM and independent persistent disk stores;
  restart indexing, eviction, corruption recovery and scoped clear.
- [x] 58C: nonblocking viewer integration, demand-first restores, idle retention,
  exact-key publication, explicit timeline/composition classification.
- [x] 58D: settings, usage and clear controls; remove hidden frame cap and expose
  meaningful budgets without tying RAM/disk capacity to VRAM.
- [x] 58E: remove the quality-comparison GPU pass and status readback from the
  existing shared BC7 compressor as well as the new retention path. Completion
  callbacks protect resource readiness; export and working precision are unchanged.
- [x] 58F engineering checks: focused regression tests, native GPU round trips,
  normal/narrow settings review and affected desktop build.
- [ ] Post-phase performance verification: sustained real-project before/after measurements
  across the scenarios below; short synthetic probes do not close this gate.
- [x] Phase completion accepted by the user on 7 October 2026 (“Mark as complete and commit”).
- [ ] Post-phase manual application testing.

## Implementation details and operating limits

The engine transfers BC7 blocks directly; readback and upload preserve the encoded
bytes and padded texture dimensions. A reusable staging buffer is bounded to
64 MiB, plus at most one temporary CPU payload of that size. Those are separate
from retained RAM bytes. All large pixel work and disk access are worker-owned.
A hash index replaces linear GPU lookups; eviction remains LRU among eligible
entries. Lower-tier metadata and explicit preparation are bounded to 131,072
frames, rather than the former hidden 512-frame ceiling. Reviews share GPU/RAM
allowances, but divide disk capacity only with reviews using the same store. Preparation estimates
BC7 bytes plus header allowance and retains actual coverage/eviction state.

GPU, RAM, timeline disk and composition disk have separate clear operations.
Clear affects only owned cache files; unrelated files are preserved. Corrupt or
truncated payloads fail identity/length/checksum checks and fall back to a real
miss. Startup indexes validated headers; checksum validation occurs on read.
Disposable writes use temporary files and atomic rename, without expensive
per-frame durability sync. An OS file lock permits one writer per disk quota;
startup removes owned abandoned temporary files and can be cancelled. Disabled
stores can still be explicitly cleared. One worker limits outstanding memory/I/O. GPU-only
mode performs compression with zero pixel readbacks. Unsupported BC7 devices
keep direct presentation and report retention unavailable; CPU-only preview
has no new lower-tier retention path.

Ordinary cache strips still show GPU-ready coverage. Explicit review status
includes RAM/disk coverage, and cache settings show separate usage. Full-frame
canonical timeline caching and distinct per-tier strip colors remain separate
follow-ups; this implementation never silently changes the requested ROI.

## Performance and correctness gates

Compare caching disabled/enabled for cold rendering, warm GPU playback, RAM/disk
restores, seek/zoom/resize, two viewers, review preparation, export contention and
memory pressure. Record P50/P95 readiness and UI pacing, dropped/admitted frames,
readback/upload bytes, RAM/VRAM/disk peaks, and retention skips. A direct GPU hit
must introduce no pixel transfers. Cold presentation must not await retention.
Idle copying must stop accepting work when playback starts; submitted work still
finishes safely. No sustained-throughput claim from round-trip unit tests.

Test exact key distinctions, budgets and eviction, disabled tiers, old settings,
malformed/truncated files, restart reuse, separate stores, cancellation/clear
races, held-frame safety, and byte-equivalent GPU restoration. Full workspace
checks and Clippy are deferred to the vertical-slice checkpoint. Report unavailable
native/performance evidence rather than claiming acceptance.


## Engineering checkpoint — 7 October 2026

The native GTX 1070 tests cover BC7 RAM/disk retention, restart reuse, scoped
clear, GPU-only compression without pixel readback, held-resource pressure, and
16-frame 1080p replay beyond the GPU allowance. Replay submitted **zero graph
requests**, with P50 1.639 ms and P95 5.030 ms readiness in that short probe.
It transferred 33,177,600 readback bytes during preparation and 31,104,000 upload
bytes during replay. These are synthetic readiness measurements, not sustained
UI pacing or cold/warm application qualification. See [native cache log](evidence/phase-58/native-cache.log).

The shared compressor's two native tests pass with no quality-comparison pass or
status readback ([BC7 log](evidence/phase-58/native-bc7.log)). Renderer transfer
round trips preserve RGBA8 and BC7 bytes at small, non-block-aligned and 1080p
sizes. Platform storage tests cover budget eviction, independent stores, restart,
corruption/truncation, file locking, abandoned temporary files and clearing a
disabled store ([storage log](evidence/phase-58/storage-tests.log)). Focused
preferences, scheduler, review-budget and app content-identity tests also pass.
The affected UI suite passes 109 tests (six tests ignored in the default
run; targeted native checks run separately), and the desktop build passes; workspace tests and Clippy were
not repeated. Native GPU checks required hardware access outside the sandbox;
the sandbox software adapter timed out and does not qualify hardware behavior.

Settings were rendered and visually reviewed at [normal width](evidence/phase-58/settings-normal.png),
[narrow width](evidence/phase-58/settings-narrow.png) and
[the narrow lower section](evidence/phase-58/settings-narrow-bottom.png).

### Application testing

Reopen the rebuilt desktop. In Settings → Application, set the timeline disk
limit to 20 GiB (or another desired capacity), choose independent disk folders
and composition capacity, and set the viewer RAM budget. Disk limits default to
zero. Apply and restart for resource settings to take effect.

Prepare a marked range that exceeds the GPU allowance, then use cached playback.
Test channel switches and return to the prior channel, zoom/resize, separate
composition viewing, reopening the project, and each scoped clear. Old view
variants should survive until eviction; changed views must never show mismatched
cached pixels. The current ordinary strips report GPU coverage while prepared
range status includes all tiers. Sustained performance and manual application
testing remain post-phase follow-ups; phase completion was accepted by the user.
