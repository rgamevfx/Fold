# Current work: Motion, 3D and Compositing replacement

Status consolidated from the 2026-10-12 checkpoints, including the existing
uncommitted image-alpha, font-ownership and keyframe fixes. **All five replacement
slices remain open.** This checklist owns remaining scope and acceptance;
[architecture](architecture.md) owns contracts. No new runtime checks were run
for the documentation consolidation.

## Remaining work, in dependency order

| Slice | Implemented at selected seams | Still required |
| --- | --- | --- |
| 1. Shared 2D document | Schema-4 native shapes, text, paths, groups, images, paint and scripts; stable IDs, guarded edits, scalar/point/handle animation | Complete remaining path/animation and image/resource workflows; establish concrete missing cases before adding features |
| 2. fframes integration | Retained compiled scripts, shared native/script SVG/Skia rendering, floating-point GPU sharing and signed/HDR shader transport | Broader image/color input, full resource accounting, measured larger-scene performance and native package acceptance |
| 3. Artist and agent tools | Canvas, Pen, numeric point controls, hierarchy, graph navigation, script controls, conversion and immutable multi-pose frame review | Native keyframe, conversion and remaining vector/artist-agent workflow acceptance |
| 4. Context cutover | New Motion and independent 3D registered; old Motion crate/protocol paths removed; 3D hierarchy, resources, picking, transforms and animated parenting | Native 3D parenting/workflow acceptance, final deletion audit and current checkpoint validation |
| 5. Complete application slice | Mixed Motion/3D composition and Timeline save/reopen; candidate/Apply workflow; tested Float RGBA EXR parity | Other delivery/color cases, HDR display, sustained frame speed and end-to-end native artist/agent acceptance |

Work one concrete gap at a time: inspect current behavior, make a complete change,
run a relevant check, then offer an optimized demo. Preserve accepted work. Broad
categories above are not permission to invent new features or resume an old
session's goal mode.

## Accepted work and important limits

- **Accepted:** Motion layout at normal/narrow widths and the release canvas
  Bézier interaction (“okay. it feels good.”). Keep ImPlot input/guides. The
  separate animation/keyframe editor has not received native acceptance.
- **Images:** Motion accepts 8-bit sRGB stills without embedded ICC profiles,
  with default Straight or explicit Straight/Opaque alpha. Opaque retains source
  RGB even at zero alpha. Premultiplied reassignment, ICC and high-bit-depth/HDR
  input remain unsupported. Upstream image transport is premultiplied RGBA8;
  silently reducing precision is not a solution.
- **Memory:** known image/font ownership and Skia's budgeted cache have checks.
  Font parser metadata, text/drawing caches, unbudgeted GPU resources and driver
  allocations are not fully accounted for. Cache allowance is not total VRAM.
- **Subdivision:** Linear/Hold/HoldNext and aligned Bézier timing have geometry
  evidence. Incompatible timing rejects. Original/subdivided raster edges can
  differ; Bézier public/native/export qualification remains open.
- **Delivery:** tested mixed signed/HDR Float EXR matches linear preview in
  premultiplied and straight alpha. This does not qualify HDR image import,
  native HDR paint, other delivery formats or HDR monitor presentation.
- **Performance:** the application 10,000-dot baseline at 640×360 is 42.482 ms
  warm median / 45.123 ms p95; native warm median is 15.962 ms. These include graph
  evaluation and display output, not full UI FPS. Preview submission's 0.120 ms
  median excludes rendering and display. None is general performance acceptance.

## Evidence needed for the next decisions

Detailed historical results stay beside fixtures in
[resources/motion-context](../resources/motion-context), not in this checklist.

| Decision | Evidence |
| --- | --- |
| Image input and retained font bytes | [Alpha/font ownership](../resources/motion-context/alpha-font-ownership-evidence.json) |
| GPU cache versus full accounting | [Ownership](../resources/motion-context/gpu-cache-ownership-evidence.json), [pressure/recovery](../resources/motion-context/gpu-cache-pressure-evidence.json) |
| Geometry versus raster parity | [Bézier geometry](../resources/motion-context/bezier-subdivision-evidence.json), [subdivision pixels](../resources/motion-context/subdivision-render-evidence.json) |
| Signed/HDR delivery | [Mixed EXR](../resources/motion-context/mixed-hdr-export-evidence.json), [Skia precision](../resources/motion-context/color-precision-evidence.json) |
| Frame cost | [Application profile](../resources/motion-context/application-frame-profile.json) |
| Installed package | [Relocated precision package](../resources/motion-context/installed-precision-evidence.json) — predates later source changes |
| Public multi-pose and downstream review | [Animation](../resources/motion-context/animation-review-evidence.json), [mixed candidate](../resources/motion-context/mixed-candidate-evidence.json) |

The latest focused checks reported 11 shared animation-editor tests, four Motion
context tests and 15 media tests passing, plus the real GPU alpha/font example.
Real-Session regressions live in `crates/app/tests/motion_gesture.rs` and
`composition_gesture.rs`; the latter needs `fold-app/native-probe`. The canvas
release fix commits the matching active preview; the keyframe fix cancels a drag
on seek/playback. Widget-only checks previously missed these lifecycle failures.

The earlier workspace run had 575 passes, two failures and 70 ignored tests.
After stale alpha/count assertions were corrected, all 16 tests in those two
groups passed. The all-target desktop check and normal-policy Clippy passed with
warnings. These results predate the latest focused changes; they are not a new
full-suite pass. The updated SDK and release desktop built, but the installed
package also predates the latest fixes.

## Compositor performance work

The [development plan](../resources/render/dev-plan.md) owns the requested phase
checklist. The [HTML report](../resources/render/report.html) contains before/after
timings, test evidence, rejected experiments and open work. The original
[audit baseline](../resources/render/compositor-audit.json) remains historical.

Implemented: bounded status-readback reuse, single-pass validated color output,
a graph parameter arena, retained blur kernels, masked pointwise/shuffle fusion,
request-local solid/filter reuse, and incremental cache-pressure eviction.
On GTX 1070 / NVIDIA 580.173.02, the 16-solid-layer release probe fell from
17.672 to 8.904 ms median at 1080p and from 50.052 to 33.155 ms at 4K. The
single-solid probe fell from 6.726 to 0.732 ms. These are synthetic serial
renderer/display timings, not desktop FPS. Equal solid sources favor reuse;
distinct media layers require their own benchmark. Clocks/load were not controlled.

**The full optimization plan is not complete.** General binding/merge reuse,
large-filter algorithm changes, prepared-document and cross-frame caches,
per-node tiled regions, format-aware working storage and pooled native ingress
remain open. Two blur layouts were rejected after measurement. A cheap-opacity
reuse policy was also rejected because it increased live-image memory. FP32,
validation and the native ingress ownership copy remain intact.

Focused validation passed: 66 normal render tests, 49 explicit native GPU tests,
and 17 compositor tests. Feature checks and focused Clippy passed with existing
warnings. See [commands and limits](../resources/render/validation.json).
Workspace, spatial adapter, installed package and native artist acceptance were
not run for this change set. Existing uncommitted media/Motion/UI work was preserved.

## Final acceptance gate

- Complete the remaining slices and removal audit, retaining generic opaque
  storage and independent Compositor/Roto services.
- Run the [integration checks](development.md#validation) and relevant opt-in
  native/GPU tests; record failures and unverified paths explicitly.
- Refresh and relocate the complete package; verify native/script/3D/video
  output, compilation without checkout access, save/reopen, Undo and public
  candidate/Apply flows. Compare color/alpha with explicit matching transforms.
- Review editable Bézier strokes, typography, 10,000 scripted dots, a 3D clone
  scene and a mixed composition. Record hardware, quality, warm frame and
  compile/update latency, and memory scope. Obtain native artist feedback
  separately from backend tests and renderer captures.

Update this file in place when a gate changes. Keep local project backups and
running process/socket details outside repository documentation; discover and
save live artwork before restarting any review instance.
