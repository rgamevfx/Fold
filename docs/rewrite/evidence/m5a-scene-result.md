> Later implementation: [9 October animation/parenting continuation](m5a-animation-parenting-result.md). The limitations below describe this earlier checkpoint.

# M5a scene hierarchy continuation — 8 October 2026

This is partial implementation evidence, not M5a acceptance. The full compositor
replacement and public scene-authoring protocol remain unfinished.

## Implemented

- Native scene document with independent object graphs, camera/light objects,
  authoritative parent hierarchy, visibility, inherited locks, active camera,
  exact object clocks, source round trips and opaque extension preservation.
- Shared provider-neutral Outliner and projected Composition object rows. Scene
  rows are qualified by layer occurrence; selecting a row opens its actual object
  graph in Network. A graph node selection retains its owning object scope.
- Transactional scene/object/graph edits and trusted local scoped unlock. Source
  drafts are isolated by document/object. Scoped edits preserve neighboring graphs.
- Scalar/distance/vector/color curve projection, coupled typed keys and exact
  forward-clock scene mapping. Key identities and unknown metadata survive edits.
- Scene image registration, transparent output, Composition insertion and shared
  rendering adapter use. No authored scene-assembly graph is needed.

## Focused verification

- Motion UI library: 35 passed, 8 GPU captures ignored in the ordinary run.
- Native Motion reference suite: 28 passed. Scene tests: 5 passed, covering graph
  separation, hierarchy cycles, transform composition, visibility, camera failure,
  cancellation/budgets, inherited locks, source round trips, nested rational times,
  coupled vector keys, provider registration, named outputs, local unlock and undo.
- Composition UI library: 12 passed, 1 GPU capture ignored. Includes repeated scene
  occurrence identity and curve-edit routing without adding image layers.
- Desktop application `common_composition`: 9 passed; `motion_cutover`: 5 passed.
  Scene insertion, provider animation through Composition, neighboring scenes,
  undo/redo and archive save/reopen are exercised. Coupled keys edited through
  multiple scene occurrences are validated together and conflicting moves fail
  atomically.
- Explicit Vulkan test
  `hierarchy_scene_outputs_composite_independently_and_preserve_depth`: passed.
  It renders two independent scenes with a compositing layer between. The
  compositor explicitly reads the background scene’s named output. It compares the
  result against premultiplied image-over arithmetic and verifies that reordering
  overlapping objects within a scene preserves depth. This fixture uses boxes,
  not the required final public-authored title demo.
- Explicit offscreen Dear ImGui/WGPU scene capture: passed with no document
  mutation. [Normal/narrow Outliner, Network and Inspector](m5a-scene-panels.png)
  reviewed: readable labels, selected object visible, narrow Network overflow,
  ordinary graph clipping/panning and Inspector scrolling. It is not a screenshot
  of the complete desktop shell or expanded Composition rows.
- Crate boundary check and its 9 policy tests passed.

## Broad verification

- `cargo check --offline --workspace --all-targets --all-features` passed using
  the existing pinned video package at `FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`.
  The initial attempt without that variable failed at the helper build requirement;
  no system FFmpeg substitution was used.
- Workspace/all-target/all-feature Clippy completed with warnings. The final
  application library Clippy run also passed after the multi-occurrence animation
  transaction change. Warnings remain; this is not a warning-free claim.
- Offline workspace tests with desktop features and `--test-threads=1`, local
  socket access and `--no-fail-fast`: **682 passed, 6 failed, 112 ignored** across
  128 reported groups (including doc tests). The default sandbox/parallel attempt
  had additionally hit local-socket denial and an ImGui context conflict; the
  serial run with socket access passed all 39 application unit tests.
- Two failures were introduced by the filled-rectangle starter: the existing
  public demo asserted an exact three-node starter. Its guard now requires the
  new four-node starter while continuing to refuse other artwork. The focused
  CLI/MCP rerun passed both tests; two GPU variants stayed ignored. Updated the
  Fold skill/public workflow to distinguish current scene UI from the still
  unavailable public scene candidate protocol.
- The handoff file-work deadline failed in the broad runs. The exact compiled
  handoff test passed alone in 2.29 seconds without a code change. Do not count
  that isolated pass as a clean broad run.
- Remaining broad failures are in unchanged media/color paths:
  `media_sequence::input_output_transfer_roundtrips_every_rgb8_value` (profile
  rejection assertion), `nle_workflow::multitrack_nle_edit_mix_render_save_reopen_and_export`
  (media process capacity exhausted), and
  `video_workflow::saturated_colors_and_explicit_profile_rejection` (untagged
  profile rejection assertion). They were not modified or waived in this change.
- Formatting and diff whitespace checks passed. A completely green workspace
  checkpoint and the full M5a gate are **not** claimed.

No computer-use clicks, typing or dragging were performed. Captures and native
service tests do not establish physical artist acceptance.

## Remaining qualification and implementation

- Compositor still executes its existing schema-6 model. Shared socket/graph types
  do not constitute the full compositor migration; the inventory remains binding.
- Scene provider inspection/author operations exist, but the active version-2
  candidate transport remains Construction-specific. No public scene authoring,
  mixed-document candidate or final demo acceptance is claimed.
- Scene parents currently use uniform scalar scale, with explicit rejection of
  unsupported affine/shear composition. Camera and directional light graphs use
  the qualified native operation set.
- Shared curves expose direct root-definition node tracks and scene position/scale
  tracks. Spherical rotation, reusable definition occurrences/overrides, paused or
  reverse clocks, Bezier semantics and complete auto-key remain unfinished.
  Non-forward clock projection fails explicitly; no approximate retiming is used.
- Remaining typed Inspector key tables still exist. Full shared animation workflow
  and their removal require completion of the preceding projection work.
- The full mixed-scene source/protection/rebase/public/artist workflow and final
  M5a removal audit remain open. M6 must not proceed on this evidence alone.
