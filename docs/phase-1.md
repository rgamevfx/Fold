# Phase 1 — Workspace and package boundaries

## Goal

Scaffold the Rust workspace described in sections 3, 16, and 17 of
`Fold_Application_Product_Architecture.md`, without implementing later phases.

## Tasks

- Create foundation, project, media, render, platform, ui, timeline, compositor,
  motion, and app crates.
- Keep creative packages independent; project and execution infrastructure may
  depend only on shared infrastructure/contracts, never creative models or UI.
- Provide a default headless CLI scaffold and an opt-in desktop binary. Only app
  assembles concrete packages; future Dear ImGui integration stays behind ui.
- Document ownership and allowed dependency directions. Enforce them using Cargo
  metadata in CI, including transitive headless isolation.

## Acceptance criteria

- All ten crates compile and their scaffold tests pass.
- CLI builds/runs without default features and has no UI/ImGui dependency in its
  resolved graph; desktop scaffold builds/runs with `desktop` enabled.
- A boundary checker rejects forbidden direct dependencies, peer feature imports,
  and transitive UI dependencies in the headless graph; focused checker tests pass.
- CI runs boundary checks, formatting, workspace checks, and tests.

## Verification and deferred work

Acceptance criteria passed locally on Linux with Rust/Cargo 1.95.0:

- `cargo fmt --all --check` — passed.
- `cargo check --workspace --all-targets --all-features --locked` — all ten crates passed.
- `cargo test --workspace --all-features --locked` — passed; scaffold Rust targets
  have no behavioral tests yet.
- `python3 -m unittest discover -s scripts -p 'test_*.py'` — eight policy tests passed,
  covering allowed edges, infrastructure/peer violations, aliases and disabled
  declarations, transitive UI/ImGui, transitive peer imports, and new members.
- `python3 scripts/check_boundaries.py` — all-feature boundaries and headless
  transitive UI isolation passed against the real Cargo graph.
- CLI with `--no-default-features` and desktop with `--features desktop` — both
  built and ran, printing their explicit scaffold status.

CI is configured with these checks but has not been run remotely. The phase
tracker is marked complete. No native window or renderer is claimed. Stable IDs,
exact time, transactions, and persistence belong in phase 2. Dear ImGui/backend
selection and docking belong in phase 3. Runtime plugins, multiple windows, 3D,
and simulations remain deferred. The foundation slice's behavioral checkpoint
is not complete until phase 2.
