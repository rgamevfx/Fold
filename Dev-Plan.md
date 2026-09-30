# Development plan

- Before each phase, write a brief plan in `docs/phase-N.md`: goal, tasks, and acceptance criteria. Follow the product document `docs/Fold_Application_Product_Architecture.docx` and `AGENTS.md`.
- During iteration, format touched files, check affected crates, and run focused tests for changed behavior. Run workspace checks and full suites at slice checkpoints.
- Mark a phase `[x]` only when its acceptance criteria pass; record verification and deferred work in its plan. Include the tracker update in the phase commit.
- Keep scope minimal: static packages, one supported media path, and one native window. Defer runtime plugins, 3D, and simulations.

## Foundation slice

- [ ] 1. Scaffold the Rust workspace and enforce package boundaries.
- [ ] 2. Implement project state, exact time, transactions, undo, and save/load.
- [ ] Checkpoint: create, edit, undo, save/reopen, and render a generated image headlessly.

## Media slice

- [ ] 3. Build a thin Dear ImGui shell with docking, viewer, inspector, and timeline placeholders.
- [ ] 4. Display an engine-rendered texture in the viewer.
- [ ] 5. Build the shared render graph with transforms, opacity, and compositing.
- [ ] 6. Connect media import, decoding, scrubbing, and export through the shared engine.
- [ ] Checkpoint: preview and export two composited sources without blocking UI input.

## Editing slice

- [ ] 7. Add timeline move/trim/split and synchronized audio playback.
- [ ] 8. Add basic compositor nodes, a static operator registry, and nested document references.
- [ ] Checkpoint: edit a short sequence with audio and a nested composite; verify cross-document undo.

## Motion slice

- [ ] 9. Add procedural shapes/text, repetition, keyframes, wave/falloff, and exposed controls.
- [ ] Checkpoint: create an animated title through direct controls; verify out-of-order frame evaluation.

## Hardening slice

- [ ] 10. Fix correctness, crashes, recovery, and performance before expanding scope.
- [ ] Checkpoint: import → edit → composite → animate → save/reopen → export; pass workspace checks and full suites.
