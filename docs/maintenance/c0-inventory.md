# C0 deletion inventory and ownership

Baseline: `6280fb628beddf1b72725f2b7849740eaeb146f9`. The [machine inventory](evidence/c0/inventory.json) records 1,403 matching references in eight groups and parses all 15 tracked `.fold` archives without parse failures. Every group and reference role has a phase owner and disposition. Matches are review obligations, not whole-file deletion instructions; for example, a `Transform` match in Timeline is not the obsolete Compositor operation.

Reproduce with `python3 scripts/maintenance_inventory.py /tmp/fold-c0-inventory.json`. The scan uses tracked source; compare its baseline commit and fixture hashes before reusing results. This semantic closure supplements lexical matches, including aliases, dynamic registration, callers of retained mixed-purpose modules, and serialized fixture contents.

## C1 removal scope

| Surface | Registration and callers | Replacement and deletion boundary |
| --- | --- | --- |
| Two-layer Timeline | `timeline/src/video.rs`; exports in `timeline/src/lib.rs`; document and video registrations in `timeline/src/package.rs` | Delete the model, validation, compiler and both registrations. Current Sequence and Composition own supported assembly. Old `fold.timeline.video-layers` payloads remain opaque. |
| Two-layer app workflow | `app/src/media_workflow.rs::{import,active_document,active}`; `session.rs` import/opacity arms and refresh; `platform/src/desktop.rs::{DesktopCommand,DesktopState}` | Delete import, opacity command/state and legacy discovery. Retain generic content identity, output selection, preview request, scene evaluation and export infrastructure; remove their legacy-only branches. Output fallback callers must be retested after removing the provider. |
| CLI/desktop entry points | Desktop `--layers`; CLI `import-video` branch calls `media_workflow::import`; `session.rs::background` calls it through the `workflow` alias | Remove the legacy entry points and success wording, not ordinary asset import or current Sequence CLI commands. CLI generic render/export callers remain supported. |
| Test aliases | `video_workflow.rs`, `audio_workflow.rs`, `native_playback.rs`, `project_ingest.rs`; `document_placement.rs` also matches `timeline_workflow::active` | Re-author meaningful media/preview/export/history tests with current fixtures. `timeline_workflow::active` resolves current Sequence and is retained; its lexical match is not a deletion. Keep unsupported-provider preservation tests. |
| Timeline schema 2/3/4 | `sequence.rs::from_document`, `package.rs::Documents::supports_schema`; schema constant consumed by authoring/lib | Keep schema 5 and its current writer. Remove version-specific gates for nested sources, images, effects and adjustment clips. Generic current validation stays. |
| Composition schema 1/2 | `model.rs::{document,from_document}` and `package.rs` schema registration | Write schema 3 consistently, including ordinary compositions; execute only schema 3. All four current Composition demo archives presently write schema 1 and must be re-authored deliberately. No automatic payload migration. |
| Grade/Merge/Transform/Blur v1 | `compositor/src/registry.rs` and `kernel.rs`; projection in `operation.rs`; catalog/defaults in `automation.rs` and `ui/graph.rs`; validation, animation, channel inference and lowering in `model.rs`, `animation.rs`, `channels.rs`, `graph.rs`, `compile*.rs`; inspector matches | Keep the corresponding v2 contracts. Re-author producer call sites, then delete v1 variants and match arms. Reconcile controls/defaults without adding UI labels or feedback. A box blur and Gaussian blur are different algorithms: remove the former as an unsupported old contract, or specify a genuine current box-blur mode before retaining it. Never claim pixel equivalence by changing expected output silently. |
| Workspace versions and retired panel names | `platform/src/workspace.rs::decode` → `workspace_migration.rs::upgrade` → `upgrade_legacy`; `ui/src/shell.rs::sync_instances` → `composition.rs::migrate_workspace`; old window-label replacements in `sync_instances` | Keep v4 data, current linked viewer/editor behavior, and generic unavailable-contribution handling. Remove upgrade functions and known retired-label rewrites. Unsupported sidecars use current defaults without editing artwork. Preserve existing error presentation; add no notification. |
| Archive v1/v2 interpretation | `project/src/persistence.rs::load`, version range, v1 `discover_items`; organization migration regression | Support archive v3. Reject unsupported containers without writing them. Keep unknown envelope/extension data within the supported archive. Keep `ProjectState::discover_items` and its call in `coordinator::stage`: current creation commands independently rely on organization discovery. Delete only its legacy-load use and migration-specific tests/comments. |

### Shared render kernels are not automatically obsolete

`ImageOp::Over`, affine transforms and shared image operations have independent consumers in Composition, Timeline, render tests and current Motion lowering. Removing Compositor v1 registrations does not justify deleting these kernels. The machine inventory assigns render matches to shared-consumer review. `render_profile.rs` already uses v2 Compositor contracts and remains a valid profiling tool. Current `Grade`/`Transform` types are not aliases for old contracts merely because their names coincide.

### Fixture and public-material closure

- `demos/Make-it-move.fold`, `m5-integrated.fold`, and `m5a-mixed-scene.fold` contain Grid/Cloner v1 records already unsupported by M5b. C1 must replace current launch/authoring references and re-author or retire these shipped fixtures. Do not add old-operation execution to make them load.
- `demos/m5b-background-spheres.fold`, `m5b-wrangle.fold`, and `m5b-integrated.fold` supply the current baseline. The first two contain Composition schema 1 and Compositor v1 operations requiring C1 fixture replacement. Baseline originals/hashes are retained as evidence; do not overwrite user copies.
- `demos/README.md`, `run-m5-demo.sh`, `make-it-move.py`, `m5-integrated.py`, `m5a-mixed-scene.py`, `m5b-background-spheres.py`, `skills/fold/SKILL.md`, and `skills/fold/references/public-workflow.md` are explicit public-material owners. Verify the supported recipes through discovery and public authoring when replacing them. Follow the installed Fold skill link rather than creating a divergent copy.
- Existing `docs/evidence/phase-47/**` archives contain the retired Motion provider. Keep dated historical evidence; no runtime reader is authorized. Existing M5 v1 construction archives under `docs/rewrite/evidence/` are also historical. Tests with active references must use re-authored current copies (several already use `m5b-controls-*`).
- Reconcile current claims in `docs/architecture.md`, the shared product specification, and relevant rewrite records. Historical phase bodies remain dated evidence under the retirement register. Do not resurrect `dev-plan.md`.

## C3 projection replacement

The inventory lists 264 references across 56 tracked files. `compositor/src/operation.rs` owns record/config conversion; graph validation, compilation, animation, parameter sampling, authoring, presets, reusable effects and inspector consumers all cross it. External producers include `app/src/{composition_layers,read_source,timeline_transition,preview_metadata}.rs`, CLI fixtures, app/compositor examples and shared test helpers.

Replace the projection as a complete provider-owned change; do not leave old `configuration()` callers behind a compatibility wrapper. Retain native records, stable parameter/edge identities, kernel behavior, and unknown-operation preservation. `OperationConfig` being a disposable kernel input is legitimate; repeated JSON conversion and unenforced validation assumptions are the problem. C3 determines the final private representation.

## C7 vector replacement

`gpu/mod.rs` registers `vector_legacy`; `gpu/vector.rs` owns the old pipeline and invokes it when `Sources::prepare` reports `needs_cpu_transform`. The trigger is transformed bounds exceeding ±65,536 after whole-instance culling in `render/src/vector_instances.rs`, not a missing GPU or a Motion-version fallback.

Owner: Render, C7. Qualify large/offscreen intersecting transformed bounds in the retained path, preserve stroke/coverage/alpha behavior, then delete `vector_legacy.rs`, its WGSL, pipeline ownership, clear/encode calls and obsolete counters. Existing `gpu/vector_tests.rs` explicitly expects a fallback in one case and must become retained-path behavior coverage. `app/tests/motion_video.rs` checks counters and must be updated consistently. Historical JSON statistics are evidence, not runtime callers. Shared vector geometry remains required by current Compositor and Motion.

## Dependencies and remaining rewrites

No workspace crate or third-party dependency is proven exclusive to these removal groups. Keep `fold-timeline`, `fold-composition`, `fold-compositor`, `fold-graph`, image/vector kernels, Serde, media/color support and GPU dependencies. C1/C3/C7 must inspect resolved consumer graphs before deleting a manifest entry; update Cargo.lock only when a real dependency is removed.

C2 capability/default replacement is owned by Platform and provider registrations. C4 dispatch replacement is owned jointly by App and the affected provider; generic candidate/retry/job infrastructure remains. C5/C6/C7 rewrite implementation files incrementally behind the current behavior tests, deleting their old orchestration within the completed phase. None is a blanket whole-crate deletion. Source, test, tool, demo and documentation references inherit their group's owner; there are no unassigned inventory groups.

## Explicit exclusions

Keep rational time; project transactions and opaque envelopes; current native Motion, Rhai Wrangle, shared graph semantics, CPU reference evaluation, color interpretations and independent media adapters. ImGui's third-party `render_legacy()` naming is not a Fold compatibility engine. A documented CPU/color/reference path is not removed by a keyword search. The prior audit's native-video `error.contains("lost")` match is in a test assertion; confirmed production error-text scheduling decisions are in preview, export and thumbnails.
