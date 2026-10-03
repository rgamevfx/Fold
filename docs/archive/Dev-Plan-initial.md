# Development plan

- Before each phase, write a brief plan in `docs/phase-N.md`: goal, tasks, and acceptance criteria. Follow the product document `docs/Fold_Application_Product_Architecture.md` and `AGENTS.md`.
- During iteration, format touched files, check affected crates, and run focused tests for changed behavior. Run workspace checks and full suites at slice checkpoints.
- Mark a phase `[x]` only when its acceptance criteria pass; record verification and deferred work in its plan. Include the tracker update in the phase commit.
- Keep scope minimal: static packages, one supported media path, and one native window. Defer runtime plugins, 3D, and simulations.

## Foundation slice

- [x] 1. Scaffold the Rust workspace and enforce package boundaries.
- [x] 2. Implement project state, exact time, transactions, undo, and save/load.
- [x] Checkpoint: create, edit, undo, save/reopen, and render a generated image headlessly. See `docs/phase-2-checkpoint.md`.

## Media slice

- [x] 3. Build a thin Dear ImGui shell with docking, viewer, inspector, and timeline placeholders. User accepted; verification evidence and deferred native checks in `docs/phase-3.md`.
- [x] 4. Display an engine-rendered texture in the viewer. CPU-upload path and native resize/close verified; see `docs/phase-4.md`.
- [x] 5. Build the shared render graph with transforms, opacity, and compositing. Bounded CPU graph and shared provider contract verified; see `docs/phase-5.md`.
- [x] 6. Connect media import, decoding, scrubbing, and export through the shared engine. User accepted; MP4/H.264 path, desktop jobs/controls, range export, and GPU RGBA8 viewer cache implemented and automatically tested. See `docs/phase-6.md` and accepted cache decision `docs/adr/0001-viewer-cache.md`.
- [x] Checkpoint: preview and export two composited sources without blocking UI input. User accepted based on automated concurrency and native preview/seek verification; deferred native checks and measurement limits remain documented in `docs/phase-6-checkpoint.md`.

## Editing slice

- [x] 7. Add timeline move/trim/split and synchronized audio playback. Complete: registered first-party multi-track NLE package, direct canvas editing, linked A/V transactions, shared compositing/mixing and export, and synchronized transport. Workspace tests and checks pass; native interaction acceptance is user-verified. Scope, evidence, and hardening limits: `docs/phase-7.md`.
- [x] 8. Add basic compositor nodes, a static operator registry, and nested document references. User accepted the corrected spatial node canvas after native inspection and laptop navigation refinements. Includes typed sockets/wires, selected-node inspector, live gesture previews/undo, independently ordered authoring graphs, and exact nested-source navigation. Backend nesting and atomic Create Composition retain audio. Final workspace tests, Clippy, formatting, and boundary checks pass. Scope and remaining verification limits: `docs/phase-8.md` and `docs/phase-8-correction.md`.
- [x] Checkpoint: edit a short sequence with audio and a nested composite; verify cross-document undo. Complete by user acceptance. Workspace and focused nesting/audio/export/undo-redo checks pass; native audio-clock playback observed. Evidence and verification limits in `docs/phase-8-checkpoint.md`.

## Motion slice

- [x] 9. Add procedural shapes/text, repetition, keyframes, wave/falloff, and exposed controls.
- [x] Checkpoint: create an animated title through direct controls; verify out-of-order frame evaluation.
s
## Hardening slice

- [ ] 10. Fix correctness, crashes, recovery, and performance before expanding scope.
- [ ] Checkpoint: import → edit → composite → animate → save/reopen → export; pass workspace checks and full suites.
