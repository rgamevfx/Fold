# Phase 11 — reference evidence and cross-cutting contracts

## Subsequent playback refinement

The agreed [phase-15A plan](phase-15A.md) and playback decisions in [the active tracker](../dev-plan.md) supersede universal clock-driven viewer playback and ordinary pending-state image replacement: viewers will support Real-time and Every-frame policies with uninterrupted held-image presentation. Monitored audio/monotonic clock rules apply to Real-time; Every-frame advances with presentation and mutes audio. Historical baseline observations, contracts, and acceptance evidence below remain unchanged; they do not certify the new behavior. Phase 19 owns bounded cached-range preview.

## Goal, boundaries, tasks and acceptance

Establish reference evidence and explicit contracts **before** changing schemas,
rendering semantics or panel workflows. Preserve the CPU reference renderer and
existing regression fixtures. No production color, schema, UI or transport change
belongs to this phase; isolated technical probes are not implementation acceptance.

Tasks: inventory owners/behavior/budgets; capture release baseline stages; pin
ACES/config/OCIO and prove shader/LUT interoperability; specify feature-neutral
project/media/frame/viewer interfaces; reconcile phase 9; record risks, evidence
and remaining gates. Verification uses runnable probes and focused checks, with
native evidence explicitly separate.

Acceptance requires actionable dependency/policy selections, runnable fixtures
and results, migration risks and remaining decisions with owners. The separate
checkpoint requires CPU/GPU image evidence, packaging feasibility and approval
of project/viewer contracts.

**Status: complete, including the phase-11 checkpoint.** Inventory, reproducible
headless and native release baselines, CPU/GPU OCIO evidence and packaging
feasibility are recorded. The user's follow-up **“Proceed, finish this phase”**
after the policy-review recommendation authorizes the selected policies below;
this is approval of the contracts and implementation direction, not acceptance
of later production features. The native gate is closed by
[the measured desktop trace and inspected screenshots](phase-11-native.md).
Later-phase implementation/verification obligations remain explicitly owned.
Do not silently substitute conflicting semantics.

## 1. Inventory and ownership

Inspected baseline: `43eccebedf26b388fced27a459b3298882e5b808`. Read the product
architecture proposal and Fold UI skill; the active plan's ACES and independent
viewer decisions supersede the proposal's deferred OCIO/shared transport.

| Existing owner | Current behavior / assumption | Required change and phase |
| --- | --- | --- |
| `project/src/model.rs`, `coordinator.rs`, `persistence.rs` | Generic envelopes, assets, dependency records, immutable snapshots, atomic batches, global undo; JSON archive v1; unknown payload preservation | Generic item/bin records and archive migration, 12; color identity, 16; recovery, 21 |
| `media/src/lib.rs`, `video.rs` | P6 RGB8 sRGB fixture adapter; MP4 H.264 yuv420p only, limited BT.709 primaries/transfer/matrix, left chroma, progressive, square pixels, one video stream, no rotation, exact zero-origin CFR | Preserve native metadata/planes, 12/16/18; retain rejection of unverified profiles |
| `media/src/audio.rs`, `wave.rs`; `render/src/audio.rs` | WAVE and supported video audio adapters; prepared 48 kHz stereo float PCM, sample-addressed mixing, offline evaluation | Shared device/clock ownership, 15; bounded shared preparation, 19; long-run validation, 21 |
| `timeline/src/authoring.rs`, `app/src/media_workflow.rs` | Import probes and creates assets **and clips**; legacy one/two-layer workflow remains | Split asset-only ingest from placement, 12/13; retain selection inspectors |
| `platform/src/packages.rs`, `lib.rs` | Static registration, provider-owned payloads, cross-document compilation and explicit refs; output helpers assume video/default port, some first-document fallbacks | Metadata/output enumeration, 12–14; no feature enums in shell |
| `render/src/graph.rs`, `lib.rs` | CPU premultiplied linear-sRGB RGBA32F. Media/video/vector/solid, nearest affine transform, opacity, half-open crop, transparent-border fixed-divisor box blur, SDR-clamped RGB gain, alpha mask, Porter–Duff over | Preserve semantics as reference; explicit color/precision, 16; GPU parity per operator, 17 |
| `render/src/vector.rs`, `motion/src/geometry/` | Native paths/text/instances lower to immutable drawing work; rasterization uses 8-bit linear color/coverage before float expansion | Remove working-color precision bottleneck, 16/17; inherited motion completeness, 21 |
| `app/src/session*.rs`, `platform/src/desktop.rs` | Singleton viewer/navigation, latest preview mailbox and playback controller; document-keyed review ranges; `PreviewKey.view = 1`; transient preview edits separate from committed snapshots | Instance context, 14; request/transport isolation, 15; fair shared scheduling, 19 |
| `ui/src/sdk.rs`, `shell.rs`, `desktop.rs`, `preview.rs` | One panel per contribution, one viewer/transport, UI-owned device and preview host; RGBA8 LRU protects displayed/submitted textures | Runtime instances, 14; host render resource ownership, 17; shared multiconsumer cache, 19 |
| `app/src/media_workflow.rs`, `media/src/video.rs` | Persisted delivery selection already independent of navigation; export still evaluates via PreviewKey/to_display then RGB8 sRGB → limited BT.709 H.264; temporary/no-clobber destination | Explicit scene/output requests, 16/20; keep pinned committed snapshot and source safety |

Supported effects above describe actual IR, not all aspirational nodes/tools in
the proposal. Arbitrary VFR, 10-bit, HDR monitoring, broad codecs, general 3D,
simulations, multiple native windows and runtime plugins remain outside the
verified profile. NVDEC/NVENC availability in a driver is not Fold support.

### Current resource limits (not an aggregate reservation guarantee)

- Media: 4,194,304 pixels; video source ≤2 GiB, ≤18,000 frames; each decoder
  keeps two verified temporary source copies (up to 4 GiB scratch per decoder).
  Read-ahead ≤32 MiB/512 entries; batch ≤16 MiB/32 frames. Pinning rehashes and
  reprobes; source safety costs both full reads and temporary storage.
- Render: 4,096 graph nodes; 64 MiB embedded RGB8 source bytes; 128 MiB live
  working pixels with blur/vector scratch accounted locally. Caller-retained
  frames/display copies are separate, not charged to that graph limit.
- Vector: 16,384 drawings / 262,144 segments. Motion prepared roots: four entries,
  8 MiB payload budget; these are not static-geometry caches or total heap bounds.
- Audio: ten-minute source PCM limit, 512 MiB / 128-source prepared PCM budget.
- Viewer: 256 MiB RGBA8 presentation budget. A 1080p entry is 8,294,400 bytes;
  do not give every new viewer its own 256 MiB cache.
- Project history is entry-count bounded, not byte bounded; retained snapshots,
  feature roots, codec children, caches and scratch have no unified reservation
  manager. Phases 18/19 own aggregate source/CPU/GPU/disk accounting.

### Regression fixtures to retain

`project/tests/{project,serialization}.rs` (atomicity/unknown data),
`media/tests/output_profile.rs` (authored vs codec profile),
`render/tests/{graph,compositor_ops}.rs` (operator/alpha semantics),
`timeline/tests/{sequence,multitrack}.rs`, `motion/tests/evaluation.rs`,
`app/tests/{video_workflow,media_sequence,audio_workflow,motion_workflow,
compositor_workflow,delivery_selection,viewer_transport}.rs`, and the existing
`execution_probe` remain reference paths. Preserve UI gesture/cache tests too.
Passing old tests alone cannot certify ACES, native playback or multiple viewers.

## 2. Reproducible release baseline

Artifacts: `/home/rgame/Documents/fold-phase-11/`; final generated media run in
`reproducible/`, OCIO reference images/JSON in `ocio-final/`. Artifacts, virtualenv,
encoded media and scratch are outside the repository. The baseline runner writes
exact FFmpeg commands, source/output SHA-256, revision/working-tree status and
machine inventory into `manifest.json`; raw per-frame samples and sampled resource
reports are retained. This document records the numerical evidence in-repo.

```sh
cargo build -p fold-app --example execution_probe --release --no-default-features --locked
cargo build -p fold-media --example stage_probe --release --locked
python3 scripts/phase11_baseline.py /absolute/new-external-directory
```

The runner generates two one-second 1080p30 test-pattern H.264 sources, one hue
shifted, with 440/660 Hz stereo AAC; explicit limited BT.709 metadata. The existing
probe constructs two overlapping layers (foreground 0.5), a procedural glyph-wave
title and stereo audio. It evaluates ten frames, then repeats with the same
decoder. The separate stage probe measures inspect, cold decode, a same-frame
cache hit, encoding thirty synthetic gray frames, and cancellation of a new
source-preparation/decode task. Every subprocess is timed out; output directories
must be new. No user data or filesystem cache is deleted.

### Machine/build conditions, 2026-10-01

- Pop!_OS/Linux `7.1.5-76070105-generic`, x86_64; Intel i7-8750H, 6 cores / 12
  logical CPUs; 33,560,539,136 bytes RAM (~31.26 GiB).
- NVIDIA GeForce GTX 1070, 8,192 MiB VRAM, driver `580.173.02`. GPU spike explicitly
  selects Vulkan and refuses a non-GTX-1070 adapter; wgpu 27.0.1 / Naga 27.0.3.
- Samsung MZNTE512HMJH-000H1 root SSD behind encrypted LVM. Artifacts on the local
  filesystem; normal OS/browser/COSMIC and an existing debug Fold were active.
- Rust 1.95.0 (`59807616e`), release optimized build, baseline commit above plus
  the phase-11 examples/scripts. FFmpeg `6.1.1-3ubuntu5`, libx264 software codec.
- No filesystem-cache flush or system-load isolation. “Cold” means a fresh
  decoder/prepared cache, **not cold physical storage**. Single-run stage numbers,
  not P95/sustained performance. RSS sums shared pages and 50 ms nominal sampling
  can miss peaks; scratch is file lengths, not RAM.

### Generated-fixture results

| Stage | Initial | Repeat | Units / interpretation |
| --- | ---: | ---: | --- |
| Graph compile + motion evaluation, mean | 0.210 | 0.176 | ms/frame |
| Decode + CPU evaluation, mean | 366.516 | 309.902 | ms/frame; repeat exceeds decoder cache |
| Display conversion, mean | 46.007 | 44.874 | ms/frame |
| Audio preparation | 748.126 | 0.007647 | ms; two sources, 770,048 retained PCM bytes |
| Source inspect | 246.236 | — | ms; fingerprint/probe/fingerprint |
| Isolated decode frame zero | 453.645 | 0.001531 | ms; repeat is an actual same-frame cache hit |
| Encode thirty 1080p gray frames | 866.571 | — | ms total; includes conversion/startup/finalization, no audio |
| Cancel → worker joined | 0.457 | — | ms; task active at cancel, returned `job cancelled` |

Whole execution probe: 9.078 s; 116 samples; sampled process-tree RSS peak
228,704,256 bytes; scratch peak 17,671,092 bytes. Codec-stage probe: 1.579 s;
20 samples; RSS 277,897,216 bytes; scratch 13,463,190 bytes. Both exited zero.
Cancellation is one observation during fresh source preparation; it does not
establish cancellation bounds for every codec, blocked encoder or submitted GPU.

Also reran the **unchanged consolidation media** from
`/home/rgame/Documents/fold-consolidation-probe-PphM9Y/`: whole-process 7.880 s,
RSS 227,397,632 bytes, scratch 14,398,565 bytes. Raw samples are `timings.log` and
`baseline.json` in the phase-11 artifact root. Do not compare the generated
fixture's two audio sources with the older fixture's one as a regression test.

**Conclusion:** compilation is small; CPU evaluation/decode and display conversion
both miss the 33.3 ms budget. Select retained libavformat/libavcodec decoding as
the phase-18 adapter direction, preserving the process fallback until equivalent
seek/source-change/cancel tests pass. Exact FFmpeg build/distribution pin and
NVDEC interop are phase-18 gates; no zero-copy or hardware speed claim is made.

The native follow-up is recorded in [phase-11-native.md](phase-11-native.md):
2,952 release-desktop redraw samples, eleven full-resolution uploads, ten cached
seeks, cancellation/replacement and inspected desktop screenshots. Mean host
upload was 2.501 ms; upload through shared upload/UI completion observation was
7.581 ms; cached seek to present-call return averaged 6.765 ms. Cancellation
removed the stale image by the next pending-state present (6.756 ms); screenshots
verified pending visibility within 70.281 ms and replacement visibility within
829.237 ms. These are baseline observations/bounds, not sustained playback or
physical scanout guarantees. Sustained/two-viewer playback and A/V drift remain
later-phase measurements; the OCIO probe is not a substitute for native evidence.

## 3. Selected color dependencies and integration evidence

Selected baseline:

- **OpenColorIO 2.4.2**, **ACES 1.3**, **Studio config 2.2.0**, built-in identity
  `studio-config-v2.2.0_aces-v1.3_ocio-v2.4`.
- New project working space: **ACEScg**. Initial SDR display: **sRGB - Display**;
  view: **ACES 1.0 - SDR Video**; no additional look, exposure zero.
- Serialized config SHA-256 in this pinned runtime:
  `d8b361f76750ebfbedf0ded0b5e4315b283eed5e095814486b9d7c416cfbbb4c`.
  Default display processor cache ID: `a60db0c1eecaf4a2582e5ba92b75959f`.
  Cache IDs complement, not replace, runtime/config/resource content identity.
- Primary upstream references: [OCIO v2.4.2](https://github.com/AcademySoftwareFoundation/OpenColorIO/tree/v2.4.2),
  [ACES config releases](https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/releases).
  Available config/display/view names were queried from the pinned runtime,
  not guessed from globally installed OCIO (which is 2.1.3 on this machine).

Run `scripts/phase11-ocio/README.md` commands. The isolated Python generator uses
OCIO 2.4.2/NumPy 2.2.6; Rust runs generated GLSL through wgpu/Naga → Vulkan,
producing CPU/GPU 64×16 PPM images and comparing float pixels before quantization.
Premultiplied gray/colored exposure ramps include negative/above-one values,
black/white and alpha 0/0.01/0.5/1. Zero-alpha handling and unpremultiply/repremultiply
are explicit. This is actual OCIO-generated processing, not an imitation curve.

| Fixture | Maximum absolute float channel error | Gate | Result |
| --- | ---: | ---: | --- |
| Selected ACEScg display transform, no LUT required | 0.00007534 | ≤0.0002 | pass |
| Supplemental asymmetric 33³ LUT + same display | 0.00042759 | ≤0.001 | pass |

The supplemental LUT is not a modification of the selected bundled config.
It proves extraction/order/upload/sampler binding independently of the analytic
ACES view. Preliminary failures were useful: Naga rejected write-only storage
and nested swizzle stores; OCIO combined samplers needed separate bindings and
compute sampling needed explicit LOD. A 2³ hardware-interpolated LUT had 0.02298
maximum error; 33³ reduced it to 0.000428. Hardware interpolation precision cannot
be assumed equivalent to CPU interpolation. The LUT tolerance is below a quarter
of an 8-bit code value, not an authorization to relax every external config.

Final run host upload/allocation times: 4.128/4.940 ms; dispatch + synchronized
readback: 0.562/0.447 ms for analytic/LUT respectively. These are 1,024-pixel
host wall timings, **not GPU timestamps or native presentation**. Production
needs independent RGBA16F, external config, all LUT interpolation modes, dynamic
uniform and shader-resource-limit fixtures. Unsupported configurations must
report an error or an explicit CPU fallback, never silently omit processing.

### Runtime packaging selection and limits

Ship a private OCIO 2.4.2 C++ runtime behind a small Rust-owned C ABI adapter
(exception-safe handles, owned errors, explicit processor/resource lifetimes).
Build desktop and CLI against that same pinned build. Bundle serialized config,
resource hashes, runtime and third-party notices under installation resources;
resolve from installation/executable paths, never CWD or implicit `$OCIO`.
External configs are explicit project choices, fully validated with their LUT
closure and content identity; missing resources fail, not substitute silently.

Feasibility evidence: the isolated Linux wheel contains `libOpenColorIO.so` and
`ociocheck`; the tool uses `$ORIGIN:$ORIGIN/..` RUNPATH. `ldd` shows the private
runtime needs only system C/C++/math runtime libraries, not the machine's OCIO.
`ociocheck --iconfig <absolute-generated-config>` passed from `/tmp`. Thus pinned
runtime/config loading independent of CWD works on this machine. Python is a
**spike dependency only**, not the chosen application deployment strategy.
Phase 16 still owns a reproducible C++ build/toolchain pin, license/resource
manifest, actual Rust bridge and clean-install desktop/CLI packaging tests.

### Authored color, legacy and output policy

- New authored RGB is straight scene-linear ACEScg with separate alpha. The
  ordinary color picker presents labelled sRGB input and converts to ACEScg;
  numeric working-space values permit finite negative/above-one RGB. Rendering
  premultiplies. Reject nonfinite values and invalid alpha explicitly.
- Nonlinear processors operate on straight RGB; repremultiply afterward.
  Alpha-zero produces zero RGB, not division by zero or hidden unbounded values.
  Preserve valid signed/HDR RGB in working images; clamp/quantize only at explicit
  output boundaries. Grades/vector colors must not retain the old SDR clamp.
- Archive v1 or missing color contract means **Legacy linear-sRGB**, never ACEScg.
  Phase 12 organization migration must not change color. Phase 16 retains the
  legacy scene/effect/display interpretation, including old authored values, so
  old fixtures retain appearance. Opening/saving is not color migration.
- No automatic payload rewrite or one-click migration of unknown providers.
  Conversion to an ACES project requires provider-supported conversion with
  before/after fixtures, or an explicitly declared legacy-rendered source whose
  *scene-linear* linear-sRGB output is converted at the boundary. Never transform
  an already display-baked legacy result as if it were scene-linear.
- Viewer exposure/look/display are workspace settings. Delivery stores its own
  transform/profile with explicit output reference. Selected SDR video default:
  ACES view for `Rec.1886 Rec.709 - Display`, then verified RGB→limited BT.709 YUV
  encoder conversion/metadata. Phase 16/20 must fixture the exact signal transfer;
  do not feed this output through the legacy sRGB conversion again. Matched
  preview/export settings must agree before encoding; their defaults need not.

## 4. Minimum contracts for implementation

These are conceptual records/operations, not a speculative new trait hierarchy.
Extend existing modules at demonstrated seams. Project/render must not acquire
clips, motion nodes, or compositor payload schemas.

### Project items, ingest and media (phases 12/13)

- `ItemId`, `BinId` stable identities; item = display name + parent bin + exactly
  one `AssetId` or `DocumentId` reference. Root is explicit; one location per item,
  no aliases/smart bins. Bin membership is organization, not render dependency.
- Project core validates uniqueness, parent existence, cycles, dangling targets
  and atomic operations. Names may duplicate; UI disambiguates with ancestry/type,
  never treats labels or row indices as identities. Persist ordering explicitly.
- `ImportRequest` contains destination bin, paths and expected revision.
  Media workers return bounded verified asset/stream metadata plus a proposed
  edit; coordinator commits one transaction. Cancellation/stale proposals create
  no partial project state. No timeline/composition/delivery/viewer retargeting.
- Media-owned stream metadata: stable stream selector, codec/profile, dimensions,
  aspect/orientation, exact time base/start/duration/PTS policy, audio rate/layout,
  native pixel format/bit depth/planes, primaries/transfer/matrix/range/chroma,
  alpha, raw metadata and interpretation provenance. Unknown ≠ guessed sRGB.
  Explicit user overrides remain separate from probed facts and are visible.
- Reuse an existing asset/item for the same canonical file location **and verified
  fingerprint**; do not move it to the chosen bin silently. Same bytes at another
  location remain distinct linked assets. Changed bytes at the same location
  require explicit relink, not silent replacement or automatic duplicate merge.
- Relink preserves AssetId, stages verified fingerprint/metadata atomically and
  validates affected uses via providers. Fail incompatible duration/streams rather
  than silently trimming. Invalidate dependent content. Already pinned exports
  retain original source handles; no opportunistic switch mid-job.
- Delete empty bins only; a nonempty bin must first be emptied/moved explicitly.
  Delete an unreferenced item and its target atomically; reject authored references
  (including delivery output), with a usage report. No cascading clip deletion.
  If an unknown provider prevents proving safety, reject destructive deletion.
  No command deletes the linked source file. Undo restores organization and IDs.
- Existing assets/documents without organization migrate into root items with
  deterministic IDs/names; preserve unknown data/dependencies/resources verbatim.
  Archive version validation and migration run on a copy; no live partial load.
- Placement is another undoable command: typed asset/stream or document/output
  reference plus explicit destination, track/node context, exact time, duration,
  source range and mappings. Destination provider validates locks/type/cycles.

### Output enumeration and frame interface (phases 14/16/17)

- Registered providers enumerate lightweight `OutputDescriptor` records for every
  document, without decoding: stable port ID, label, capability (video/audio),
  exact bounds/rate or sample format, dimensions/aspect and availability/error.
  Browser/editor lists are not restricted to open documents. Missing providers
  remain visible unavailable items; invalid refs never pick the first document.
- Evaluation request = immutable snapshot (committed for export), `DocumentRef`
  including port, exact time/range, dimensions/region, quality, color intent and
  cancellation. Viewer ID/generation are consumer routing, **not content identity**.
  Separate scene identity from presentation identity so display changes reuse
  scene evaluation. Export cannot evaluate through viewer PreviewKey.
- Frame descriptor = size/aspect, plane format/strides/bit depth, timestamp/duration,
  primaries/transfer/matrix/range/chroma/color processor identity, alpha convention,
  CPU/GPU/external storage and an owned lease plus readiness completion token.
  A CPU borrow requires readiness; a GPU consumer waits by dependency, never on
  the UI. Neither a raw pointer nor Arc alone proves GPU completion or safe reuse.
- Host render module owns device/queue/pools and transfer accounting, not panels.
  Display-ready RGBA8 handles go to the texture registry; overlays draw afterward.
  Decoded, geometry, working and display caches have separate globally accounted
  budgets. Every displayed/referenced/in-flight resource is protected; reserve
  before allocation and return pending/backpressure rather than blocking UI.
- Presentation identity includes output port, exact time, content/implementation
  identities, dimensions/quality, config/resources/processor, display/view/look,
  baked adjustments. Bin/name/layout changes do not flush scene content. Media
  interpretation/working-space changes do; display-only changes do not.

### Instances, transport and workspace (phases 14/15/19)

- Distinguish registered contribution ID from stable workspace `PanelInstanceId`.
  Editor instance binds an explicit document plus nested source-context/time map;
  viewer instance binds a stable output reference or an explicitly owned source.
  **User-approved phase-14 refinement:** normal viewer/editor/inspector linking
  uses groups A–D via a small letter dropdown (A by default), not an exhaustive
  viewer target list. One explicitly chosen editor supplies each group's source;
  window focus never changes that owner. Pinned outputs and source-port choice
  remain secondary context actions. Group linking does not link playback clocks;
  closing/moving a source freezes old followers rather than guessing a replacement.
  Earlier `Follow(EditorInstanceId)` sidecars migrate to groups or stable pins.
- A viewer request/result carries viewer ID and monotonically increasing consumer
  generation. Retarget, seek and close retire that consumer's old generation;
  accept a result only if binding/content/time/quality still match. Shared work
  may continue for other consumers. Export has its own pinned job lifetime.
- Each viewer owns rational playhead, half-open review range and loop flag. UI
  Out is inclusive, converted once to exclusive range end. New viewer starts
  paused at zero with full range; duplicate-viewer action copies state **paused**.
- Source switch pauses that viewer, preserves absolute rational local time then
  clamps to the new valid range (not relative duration percentage). Reset marks
  to full new range. No implicit play restart; another viewer/export is untouched.
- Explicit editor→viewer association supplies time for timeline seeks, keyframes,
  Open Source, overlays and shortcuts. Focus only selects command context; it
  does not retarget outputs or choose whichever viewer most recently updated.
  With ambiguous/missing association, require selection or disable time-dependent
  editing. Direct overlay edits carry the exact viewer/document/local-time context.
- Follow-editor closure retains the last valid output **and switches to Pinned**,
  visibly; it never follows a different editor automatically. Deleted/unavailable
  output yields a paused missing-target state retaining its reference for recovery.
- One explicitly monitored viewer at a time. First created viewer is the initial
  monitor; creating/focusing/playing another does not steal it. Closing the monitor
  clears monitoring (no automatic audible handover). Explicit monitor handover
  flushes/retags old audio, primes the new viewer at its own time, and never seeks
  other viewers. Callback does no allocation/I/O/locks/compilation.
- Monitored playback uses latency-compensated device time when audio is active;
  otherwise its own monotonic clock. Unmonitored viewers always have independent
  monotonic clocks. No drift correction uses another viewer's transport. Seek or
  handover rebases clock mapping explicitly; underrun emits silence/diagnostics.
- Versioned workspace sidecar outside project content stores layout, instance IDs,
  bindings, associations, viewer presentation choices/playheads/ranges, browser
  expansion/filter and edit-context state. Restore paused; monitoring restores
  only to a valid instance, otherwise none. Tolerate missing contributions/documents
  with placeholders; do not rewrite project refs or produce project undo entries.
  Bind sidecar to project identity, not document names; project Save As creates a
  separate workspace association. Project delivery/color/organization stay authored.

## 5. Phase-9 reconciliation — obligations, not retroactive acceptance

The archived tracker checkbox conflicts with `phase-9.md` and `consolidation.md`.
Treat it as historical progress, **not acceptance**. Consolidation's 124 passed /
1 ignored checkpoint is historical evidence, not a fresh run and not proof that
all motion tools or native workflows exist. No inherited obligation is waived.

| Unresolved obligation | Owner / required evidence |
| --- | --- |
| Field-driven Fill/Stroke, independently driven color/opacity, random-vector controls, full domain/space rules and all node combinations | Phase 21 / motion; focused semantic fixtures, including shuffled-time requests |
| Property-stack explanation/evaluated-result UI, direct object/layer selection, full transform interactions | Phase 21 / motion UI; direct-title and radial-pattern workflow acceptance |
| Native curve/group/binding/per-reference controls and exact group camera/selection restoration | Phase 21 / shared UI + motion; native gestures, cancel and one-entry undo |
| Motion-to-sequence placement and undo | Phase 13 regression plus phase 21 acceptance; preserve references/overrides and one transaction |
| Actual motion encoded delivery, desktop/CLI pre-encode equivalence | Phase 20; pinned identical settings plus encoded timing/alpha/output fixtures |
| Broader shaping/font-environment and missing glyph/font coverage | Phase 21 / motion resources; pinned environment and failure fixtures |
| Vector 8-bit working-color bottleneck | Phases 16/17 / render; float authored color/alpha and CPU/GPU comparisons |
| Large repeated constructions, geometry/cache budgets and cancellation | Phases 17/19 / motion + render; measured bounded stress and transfer accounting |
| Persistent/static geometry reuse proposal | Phase 19; measure first, no assumption prepared-root cache solves it |
| Native audio, monitor handover, ten-minute drift | Phases 15/21; initial one-frame latency-compensated offset and no accumulating drift |
| Full creative workflow acceptance, narrow/DPI/focus/IME/accessibility limits, device loss/OOM/recovery and aggregate stress | Phase 21 checkpoint; actual native and automated evidence, explicit unsupported behavior |

## 6. Verification, migration risks and remaining gates

Performed:

- Release build/run of existing `fold-app` execution probe and new `fold-media`
  stage probe, using generated verified-profile media: passed.
- `cargo test -p fold-media --locked`: four tests passed (see artifact `media-tests.log`).
- Measurement-tool tests: three passed initially; full Python script suite at
  closure: twelve passed. Product render/schema/transport semantics are unchanged.
  Native diagnostics are opt-in behind the `native-probe` feature and report
  environment variable; normal desktop/headless builds exclude the hooks.
- Isolated OCIO/wgpu release build, analytic image and supplemental LUT comparison:
  passed on actual GTX 1070/Vulkan. `ociocheck` from another CWD: passed.
- Touched Rust formatting, Python syntax compilation, existing-directory refusal
  and final diff/whitespace checks passed. Config hash validation and both GPU
  comparisons were rerun after pinning the serialized identity; errors unchanged.
  Inspected the generated GPU reference image (not a desktop screenshot).
- Native release run, screenshots and trace invariants passed: full-size uploads,
  no reupload on cache revisits, no cancelled/stale frame presentation. See the
  separate native evidence document for procedure, numbers and limitations.
- `cargo test -p fold-ui --features native-probe --locked`: 21 passed, including
  probe schedule/cancellation/no-clobber/incomplete-report coverage.
- Full checkpoint: `cargo test --workspace --all-features --locked -- --test-threads=1`:
  **127 passed, 1 ignored** (native audio-device test). Headless app and ordinary
  desktop checks, package-boundary/headless-isolation checks and workspace
  formatting passed. The release diagnostic desktop also built and ran.
- `cargo clippy --workspace --all-targets --all-features --locked` completed with
  **10 existing motion warnings**, no warning in changed diagnostic code.
- The first workspace build exceeded a 300-second timeout. The longer retry
  reached tests and exposed an inherited timeline test concurrency problem:
  `panel_toolbar_fits_wide_and_narrow_windows_without_editing` and the canvas test
  independently create Dear ImGui contexts, causing `ContextThreadConflict` in
  the parallel runner. Neither timeline test was changed here. Serial execution
  passed the complete suite; phase 21 owns shared test-context serialization.
  This is not concealed as an unconditional parallel-suite pass.

Migration risks: old SDR clamps/vector quantization cannot just be relabelled
ACEScg; archive v1 has no color identity; unknown-provider data cannot be migrated
semantically; OCIO CPU/GPU approximations/resource layouts vary; source pinning
multiplies disk use; global singleton time/request assumptions must be removed
end-to-end, not hidden by duplicating widgets. Preserve regression paths until
new fixtures cover these cases.

| Remaining gate | Owner | Required next evidence / decision |
| --- | --- | --- |
| Native baseline upload/presentation and cancellation-to-visible-state timing | Phase 11 / app + UI | **Closed:** release GTX-1070/Vulkan trace plus inspected pending/replacement screenshots; distinguish present call, observed completion and visible-state bounds. |
| Project/viewer policies, monitor selection, source switching, closure fallback and duplicate/delete/relink rules | Product owner + phase-11 checkpoint | **Approved to proceed:** user's follow-up authorizes completion with these recorded selections. Amend this record explicitly if requirements change. |
| Exact ACES/config/runtime selection and packaging direction | Product owner + phase-11 checkpoint | **Approved to proceed:** pinned choice and legacy policy retained; production implementation/packaging tests remain phase 16. |
| Parallel ImGui test-context conflict in unchanged timeline tests | Phase 21 / UI test infrastructure | Serialize native context ownership consistently; serial full-suite pass is recorded, not a fix for parallel execution. |
| Production Rust OCIO bridge / relocatable shipping artifacts | Phase 16 | Source/toolchain/license manifest, resource hashing, clean-machine desktop/headless test; wheel feasibility is not shipping acceptance |
| Retained decoder version/FFI and codec distribution review, NVDEC interop | Phase 18 | Measured correctness/copies; keep software fallback until verified |
| Aggregate budgets, GPU precision and hardware encode policy | Phases 17–20 | Differential fixtures and full reference workload measurements, not inferred from this small spike |

No later phase should invent another policy without amending this record. The
phase checkbox records baseline/contract acceptance only. It does not certify
production ACES integration, GPU evaluation, real-time playback, inherited motion
completion or the final creative workflow. Phase 12 is the next implementation
phase; the remaining rows above do not retroactively block phase 11.
