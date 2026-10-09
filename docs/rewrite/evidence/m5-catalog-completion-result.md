# M5 native catalog continuation — 8 October 2026

This record qualifies bounded native modes. It does **not** close M3–M5 gates.
The implementation resumes the previous handoff without restoring legacy paths.

## Implemented contracts

- Stroke retains the four centered combinations of butt/round caps and miter/round
  joins. Existing `solid_centered_butt_miter` constructions remain valid. Both
  Screen and Camera Plane use the evaluated choice.
- Transform has explicit translation, pivot, rotation and positive scale.
  `screen` accepts contours/Outline Text and transforms their path coordinates;
  Stroke width retains its canvas-unit value. `world_mesh` accepts metric meshes,
  uses quaternion rotation and inverse-transpose normals, and preserves UVs and
  element IDs. `world_instances` transforms the complete collection with uniform
  scale and Z rotation, retaining the prototype, distribution provenance, IDs and
  per-instance paint. Direct retained Text, straight_text solids and general
  affine/sheared instance transforms are not advertised by these modes.
- Tessellate `planar` and Extrude `closed_profile` accept closed planar contours
  or Outline Text. Fill rule and canvas-to-meter conversion are explicit. Output
  uses World meters with canvas Y flipped, +Z front caps and negative-Z extrusion.
  Generated normals, canvas-XY UVs, named front/back/sides regions and output
  vertex-to-cap-vertex correspondence are observable. A separate Material assigns
  spatial paint. Connectivity determines topology IDs; changing depth preserves
  IDs when segmentation/connectivity is unchanged. Outlined glyphs retain separate
  profile partitions, with no implicit Boolean union between glyphs. Conversion
  retains the source curves and packaged font provenance for standalone observation.
- The native mesh container now supports named regions and conversion provenance,
  rather than wrapping every result in the shared box-only preparation type.
- Field consumers use positions in their explicit upstream geometry. A preceding
  Transform changes that basis; the original distribution ID remains provenance.
  Word-group observations verify complete glyph membership, translated
  advance-center pivots and independent stagger from one retained layout.

## Automated and public evidence

The focused suite covers canonical Source round trips, native retained modes,
open-cap endpoint pixels, round-join pixels/planar meshes, rectangle caps/sides,
O/B/8 topology (three components, five holes), UV transfer, depth correspondence,
pivots, inverse-transpose normals, transformed field sampling, invalid controls,
cancellation and budget rejection. A private evaluator fixture distinguishes
same-winding nested contours under nonzero and even-odd rules and rejects open
profiles. These are native service/renderer tests, not physical interaction.

The public authoring script now runs eight additional Source trials through each
transport: three new Stroke modes, three Transform modes, Tessellate and Extrude
Profile. It discovers contracts, proposes, inspects, applies, observes and
restores baseline Source for each trial. Profile observations assert attribute
correspondence and named regions. GPU-enabled runs also request a public frame
for each trial. Existing baseline/wording/palette/grid variations, exact pixel
restoration, public save, native reopen and undo/redo checks remain.

Initial native-GPU catalog run passed both CLI and MCP (79.56 seconds total),
using NVIDIA GTX 1070, driver 580.173.02, Vulkan. Each report contains seven baseline
frames, eight catalog frames, one control-variation frame and one restored frame.
The contact sheet was reviewed: rotated Screen type, changed prototype dimensions
and transformed clone grid are visible; both profile conversions preserve holes.
The baseline still has 35 positioned operations. Public frame time remains
half-open, ending at **95/24**, with exact four-second CPU observations separate.

Initial reports: `/tmp/fold-m5-catalog-public/{d7589fff-bf0b-4870-a8fe-41f132dc3173,1ea78016-2c1d-4ea8-842f-a50f40431a1f}/report.json`.
The final checkpoint below supersedes affected checks after the Screen transform
stroke-width consistency correction and additional acceptance assertions.

## Reproduction

Use `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` for the following Cargo commands. Local protocol
sockets and native Vulkan require access outside this session's sandbox.

```sh
cargo test -p fold-motion --features ui --locked
cargo test -p fold-app --features desktop --test motion_spatial camera_plane_stroke --locked -- --ignored --nocapture
FOLD_M5_EVIDENCE_DIR=/tmp/fold-m5-evidence cargo test -p fold-app --features desktop --test agent_integrated gpu_authoring --locked -- --ignored --nocapture --test-threads=1
cargo check --workspace --all-targets --features fold-app/desktop --locked
cargo test --workspace --features fold-app/desktop --locked --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --features fold-app/desktop --locked
cargo fmt --all --check
python3 scripts/check_boundaries.py
git diff --check
```

## Artist acceptance and remaining implementation

`./demos/run-m5-demo.sh` builds the current desktop and opens an editable copy of
`demos/m5-integrated.fold`. The [hands-on guide](../../../demos/README.md) describes
scrubbing, controls, undo/redo and save/reopen. The shell script passes `bash -n`.

Desktop automation was rechecked through the installed computer-use skill:
`orca-ide computer capabilities --json` returned `runtime_unavailable`;
`orca-ide open --json` returned `runtime_open_timeout`; the capability retry still
returned `runtime_unavailable`. No physical desktop action or normal/narrow graph
review is claimed.

M4 picking, aggregate Bevy allocation admission, bounded Boolean intermediate
memory/cancellation latency, general spatial transparency, rapid native edit
presentation, and the physical artist/agent handoff remain open. Current
`i_overlay` before/after checkpoints are still insufficient to close its resource
requirement. These obligations are not deferred to M6/M7 or waived by the demo.

## Final checkpoint

Final code includes the path-coordinate/Stroke-width consistency correction,
native starter tests, nested fill-rule tests, complete word-pivot assertions and
named box-region observation assertions. A final standalone default-title test
exposed nonconforming cap topology when all glyphs were tessellated together.
Conversion now tessellates each outlined glyph as a separate profile before
assembling the mesh. Both Tessellate and Extrude retain source curves/font
provenance even when no original Text placement is present. The new regression
uses the complete default multiword title, not only O/B/8.

| Check | Actual result |
| --- | --- |
| Motion, `--features ui` | **136 passed, 0 failed, 6 ignored**; `/tmp/fold-m5-delivery-motion.log` |
| Camera Plane Stroke GPU | **1 passed**, eight miter/round × fill/stroke-alpha cases; zero incorrect interior pixels; `/tmp/fold-m5-stroke-plane-final.log` |
| Public GPU authoring, desktop build | **2 passed**, actual MCP and CLI, 68.18 seconds; `/tmp/fold-m5-delivery-public.log` |
| Desktop binary | Passed; `/tmp/fold-m5-provenance-desktop.log` |
| All-target desktop workspace check | Passed; `/tmp/fold-m5-provenance-workspace-check.log` |
| Desktop workspace Clippy | Passed with existing shared warnings; `/tmp/fold-m5-provenance-clippy.log` |
| Desktop workspace tests | **673 passed, 4 failed, 109 ignored**; `/tmp/fold-m5-workspace-tests.log` |
| Corrected application observation target | **19 passed**, including standalone profile provenance; `/tmp/fold-m5-delivery-observe.log` |
| Corrected observation target Clippy | Passed with existing application warnings; `/tmp/fold-m5-observe-clippy-final.log` |
| Formatting, package boundaries, whitespace, launcher syntax, Python syntax | Passed |

The full run found an introduced stale expectation in
`box_observation_reports_mesh_uvs_and_transformed_bounds_without_text_provenance`:
it still expected six unnamed regions in an array. The updated test checks all
six named regions and their exact index ranges, plus empty conversion provenance
for a box. The target passed afterward and now passes all 19 tests including the added
profile regression. The complete workspace suite was not repeated after these
corrections. The final Motion, application observation, public GPU, workspace
check, Clippy and desktop build results above include the per-glyph correction.

Three inherited shared failures remain, matching the previous checkpoint:

- `media_sequence::input_output_transfer_roundtrips_every_rgb8_value`: expects a
  dimension-mismatched render to reject, while the shared renderer resizes it.
- `nle_workflow::multitrack_nle_edit_mix_render_save_reopen_and_export`: media
  process capacity exhausted, six regular slots and two reserved for audio.
- `video_workflow::saturated_colors_and_explicit_profile_rejection`: expects an
  untagged MP4 to reject, while shared video import accepts it.

No green full-workspace result is claimed. No unrelated media implementation was
changed. Initial sandbox GPU tests selected llvmpipe but failed on private Unix
socket creation (`Operation not permitted`); the authorized native reruns above
used the NVIDIA adapter and passed.

The Camera Plane oracle retains every original miter pixel check. New round
corners use an analytic radius-five boundary and omit only its half-pixel edge
band; all other pixels retain exact premultiplied color/alpha assertions. This is
within the frozen one-pixel contour-distance allowance, not a relaxation of the
existing miter oracle.

Final public captures:

- CLI: `/tmp/fold-m5-delivery-public/8963297a-749f-4c34-89ae-f6cfc1cd0e73/report.json`.
- MCP: `/tmp/fold-m5-delivery-public/ed4f0b87-3971-4664-b5a0-39e8a8d17fd1/report.json`.
- [Frame provenance and hashes](m5-catalog-public-summary.json) retain both runs.
- [Reviewed contact sheet](m5-catalog-contact.png) shows the restored baseline and
  all eight current catalog trials. [Baseline preview](m5-catalog-preview.png)
  comes from the exact restored revision 20 at one second.
- [Current canonical Source](m5-catalog-integrated.foldsource) accompanies the
  publicly saved CLI archive copied into `demos/m5-integrated.fold` after native
  save/reopen equality passed. Archive SHA-256:
  `a5639abb06a598b1e5e52c0e0048c74805530129963bf0f40aa0ef9f17a6f548`.

The regenerated demo has fresh document IDs. Historical captures and the older
`m5-integrated.foldsource` remain evidence for their own revisions; they are not
assigned to this archive. All test/build processes completed at this checkpoint.
M3–M5 gates remain open for the implementation/physical requirements above.
