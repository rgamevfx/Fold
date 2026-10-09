# M0–M5 completion audit — 8 October 2026

This is a current requirement-to-evidence map, not gate acceptance. The rewrite
plan retains authority. M0–M2 have recorded gates; M3–M5 are incomplete. Historical
passes below are not a fresh full-workspace verification of the current tree.

## Earlier gates

| Requirement | Evidence inspected | Remaining for final completion |
| --- | --- | --- |
| M0 removal dispositions, ownership, contracts, catalog homes, protocol and dependency experiments | Plan M0 record; contracts, catalog, acceptance specification and linked inventories | Reconcile new dependency decisions, including contour Boolean resource limits, with final evidence |
| M1 old engine/protocol removal, opaque preservation, non-motion cutover | Plan M1 record and workspace checkpoint record | Fresh final cutover/dependency audit and checkpoint; do not treat the three inherited media failures as green |
| M2 document/registry/language/reference semantics | Registry and evaluator entry points; full combined M3 slice record referenced by acceptance.md | Keep earlier automated gate; rerun applicable final checks after remaining native modes land |

## M3 native editing and protocol

| Required behavior | Current evidence | Status |
| --- | --- | --- |
| One construction across Source, graph, controls and timeline | Combined slice; native capture, typed-key and graph/service tests | Automated evidence exists; physical artist continuation open |
| Scoped source/patch validation, rebase/conflict, immutable candidates, locks and exact retry | Combined full oracle; actual agent_handoff CLI/MCP cases | Automated evidence exists; regenerate final capture after remaining implementation |
| Observe without changing artist time/selection, exact inspected Apply generation and one undo group | Combined capture and integrated handoff | Automated evidence exists |
| New public tools, matching schemas and old-version rejection | CLI/MCP tests, Fold public workflow reference | Implemented; current public integrated authoring/frame/save runs pass |
| Verified public examples without implementation access | demos/m5-integrated.py and public frame reports | Implemented through discovery/Source only; harness internals remain host setup and verification |
| Rendered native artist/agent handoff | Native-service ramp/key/lock/layout capture plus separate rendered demo | Incomplete: separate runs do not prove one physical artist continuation |

## M4 text and rendering

| Required behavior | Current evidence/source | Status |
| --- | --- | --- |
| One retained Parley layout feeds Text, Outline and spatial geometry | Native text tests, integrated observations, Camera Plane Text/Outline GPU equality | Implemented and tested for packaged Noto Sans fixture |
| Vector holes/fill rules, caps/joins, clipping, thin strokes and zoom | text_gpu.rs contains 30 fixture/zoom combinations; fresh current-GPU run below | Shared renderer cases pass; this does not expose native Stroke modes or qualify arbitrary path clipping |
| Retained Bevy adapter and explicit Fold time | gpu/spatial adapter; reuse/held-frame tests | Implemented; full packed-instance and aggregate allocation qualification still open |
| Device ownership/submission/completion/readback/depth/picking/budgets/recovery | GPU Host/adapter tests; spatial module explicitly labels picking/internal allocation accounting unqualified | Incomplete: depth is not object picking; no complete resource/recovery acceptance claim |
| Screen/Camera Plane/World depth, ordered paint, lighting, linear color/alpha and display conversion | Native GPU paint/World/typography, cross-camera and mixed-scene evidence | Tested subsets pass; general overlapping paint/transparency remains open |
| Revision/time/camera frames to viewer, agent and headless | Desktop preview service evidence; public CLI/MCP integrated frames | Shared service routes tested; physical desktop agreement and rapid native edit presentation open |
| History/reset/shutter policy | Adapter declares fixed no-history profile | No-history behavior exists; final policy/evidence audit needed |
| Bounded geometric preparation | planar overlay input/output/cache caps; checkpoints before/after i_overlay | Incomplete: synchronous Boolean intermediate memory and cancellation latency unqualified |

## M5 native modes and integrated proof

The catalog is an explicit part of M0's freeze. A successful demo does not excuse
missing modes assigned to M5 there.

| Required behavior | Current source/evidence | Status |
| --- | --- | --- |
| Text, word grouping/stagger, Fill, Material, Cloner/distribution, Animator, Field/Ramp/Random, clocks, Place/Scene, cameras/light | Registry, retained text, integrated public/GPU evidence | Implemented bounded modes; group-pivot/selection semantics still need complete acceptance |
| Transform with translation/rotation/scale and explicit pivot | Catalog M5 row; registry Builtin and operation modules lack Transform | Missing native operation; Place transforms do not satisfy it |
| Stroke round/butt caps and round/miter joins | Catalog M5 row; registry/style.rs advertises only solid_centered_butt_miter; rendering hardcodes that style | Missing native round choices despite existing renderer support |
| Public Tessellate, closed planar nonzero/even-odd profiles, explicit transfer | Catalog M5 row; registry lacks Tessellate | Shared tessellation is a cache service, not the required native operation |
| Extrude closed profiles with holes, caps/sides | Only straight_text registered; native evaluation requires outlined text | Partial: text holes qualified, general supported planar profiles missing |
| Stable random/instances, shared geometry, group pivots and explicit sampling basis | Integrated clone checks and retained word records; catalog assigns full Deform to M7 | Sharing/identity/stagger tested; finish explicit pivot and sampling-basis contracts without prematurely adding full deformation modes |
| Exact integrated scene, exposed controls, multiple times and control variations | Public authoring script, saved demo, seven native GPU times and nine public frames per transport | Implemented and tested; public frame endpoint is half-open (95/24, not 4 seconds) |
| Ramp/key/lock/layout → agent field direction/timing → save/reopen/continue | m5-ramp-field-handoff record and saved construction | Native-service/public-transport proof exists; physical/rendered combined handoff still open |
| Readable graph and verified Fold examples | 35 positioned nodes, six annotations; public layout/source/save checks | Data is verified; normal/narrow native graph review still open |

## Scope and next implementation

Next native work: round Stroke choices, then Transform/pivot and public planar
Tessellate/Extrude. Keep M4 resource bounds, transparency and presentation work
visible alongside these; physical automation unavailability does not block this
independent implementation. Finish with native artist acceptance and the full
workspace checkpoint, reporting inherited failures separately.

Full Deform modes, broad production assets/images, contact sheets/clips and
performance percentile targets belong to M6/M7; USD production import/export is
M8. This does not defer M5's explicit deformation sampling-basis requirement or
M4's resource/picking obligations. No original M0–M5 requirement is waived here.

## Fresh vector audit check

`cargo test -p fold-render --features spatial --test text_gpu --locked -- --ignored --nocapture`
passes both GPU tests (2.92 seconds), including all 30 quality fixture/zoom
combinations, retained text/outline comparison and RGBA16F checks. Log:
`/tmp/fold-audit-vector-gpu.log`. CARGO_BUILD_JOBS=2, CARGO_INCREMENTAL=0,
CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0. Native NVIDIA GTX 1070,
driver 580.173.02, Vulkan. Text half-float error is at most 0.00048828125 and
display byte error at most one. This freshly resolves shared vector fixture
uncertainty; it does not qualify native cap/join authoring or physical presentation.
Whitespace checks pass. No final workspace audit or phase completion is claimed.
