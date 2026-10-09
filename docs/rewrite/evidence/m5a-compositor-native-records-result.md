# M5a compositor native-record continuation — 9 October 2026

M5a remains **in progress**. This replaces compositor animation storage and image
input connections; it is not the full compositor/document/source cutover.

## Implemented

- Compositor animation persists shared `Parameter`, `Binding::Track`, `Track` and
  `Key` records. Scalar component curves are transient Composition/Inspector views,
  not persisted curve copies. Rendering uses the shared typed sampler, including
  Bézier handles and explicit Replace/Add/Multiply semantics.
- Curve edits retain parameter, track and surviving key identities and unknown
  extension data. Protected tracks reject changes/deletion through the curve editor,
  document command and provider authoring path. Runtime effect-instance values can
  shadow protected definition tracks in transient evaluation copies; the definition
  stays unchanged.
- Image inputs persist shared `Edge` records. Rewiring retains connection identity
  and extensions; invalid cycles restore the complete previous edge. Shader-port
  migration moves existing connections and native animation records. Duplication
  allocates new edge, parameter, track and key identities. Validation rejects aliases.
- Native compositor schema is 7. Readers/registration for schemas 1–6 are removed;
  unsupported project documents use generic opaque handling, not executable migration.
  Existing processing implementations, channel metadata and image semantics remain.

## Verification

Build environment: two Cargo jobs, incremental off, dev/test debug info off,
`FOLD_VIDEO_ROOT=/tmp/fold-codec-runtime`, offline dependencies.

- Compositor all-feature suite including the ignored offscreen capture: **36 passed**.
  Covers channel routing/masks, effects, source metadata, native animation sampling,
  persistence, protected edits, identity-preserving rewiring and duplication.
- App `common_composition`, `compositor_media`, `compositor_workflow`: **15 passed**.
  Includes shared curve edits, media export, nested rational source timing,
  cross-document undo, alpha, save/reopen and Composition scene behavior.
- Protected reusable-definition override regression: **5 reusable tests passed**.
- An additional save/reopen regression preserves the retired schema-6 payload byte
  for byte while provider support/execution remains disabled: **6 animation tests
  passed**, including that new regression. No migration or compatibility shim is used.
- Workspace/all-target/all-feature check passed. Package boundary check passed.
  Affected-crate Clippy completed with existing warnings; no new warnings arose
  in the native binding or connection implementation. The final non-capture
  compositor regression run passed after evaluation-copy cleanup.
- Read-only offscreen normal/narrow Shuffle Inspector captures were inspected:
  labels, two image routes and the channel disclosure remain readable. No live
  computer input was used. This does not establish physical artist acceptance.

Logs: `/tmp/fold-m5a-native-compositor-final.log`,
`/tmp/fold-m5a-compositor-app-tests.log`,
`/tmp/fold-m5a-protected-instance-tests.log`,
`/tmp/fold-m5a-compositor-final-workspace-check.log`,
`/tmp/fold-m5a-compositor-clippy.log`, `/tmp/fold-m5a-compositor-verified.log`,
`/tmp/fold-m5a-opaque-compositor-tests.log`.

## Still required

Static `Parameters` and the `Composite`/`Node` authoring structure remain to be
replaced with native versioned operations, complete typed parameter bindings,
scoped source and structured authoring. Effect-mask references and reusable
instance-control storage also require the final identity/typed-model audit.
The v2 public candidate transport is still Construction-specific. Scene/object
and atomic mixed-document authoring, the public two-scene/title/compositor demo,
complete workflow acceptance and the full workspace test gate remain open.
No deferrals are approved; M6 remains blocked.
