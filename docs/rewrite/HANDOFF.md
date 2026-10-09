# Motion rewrite checkpoint — 8 October 2026

## Resume here

The user requested completion of **M0–M5**, then explicitly paused implementation
to continue in a new session. This commit preserves work in progress; it does not
claim those phases complete. Resume implementation when the user authorizes it.
No build/test process was left running at the checkpoint.

Read, in order:

1. [Rewrite plan](../Motion_System_Rewrite_Plan.md): phase authority and evidence ledger.
2. [Completion audit](m0-m5-completion-audit.md): remaining requirements, including the
   frozen catalog obligations that the working demo does not satisfy.
3. [Catalog](catalog.md) and [acceptance specification](acceptance.md), then relevant
   sections of the [proposal](../Fold_Motion_Architecture_and_Node_Set_Proposal.md).
4. Root `AGENTS.md`; `.agents/skills/fold-ui/SKILL.md` before UI work. Public artwork
   workflows use `skills/fold/SKILL.md` and must not depend on reading Rust crates.

M0 specification, M1 removal and M2 automated gates are recorded passed. M3 native
services/protocol and much of M4/M5 work, but their complete gates remain open.
The checkpoint contains the entire rewrite since `66ebb2ece465189a2f8dd3e415b38f63a62bdd6a`.
Legacy deletions are intentional: restore historical source through Git only,
never compatibility engines, SDK aliases or hidden fallback paths.

## First implementation task: native Stroke choices

Only inspection was performed before the pause; **no Stroke changes were started**.
Current native mode is `solid_centered_butt_miter`. The shared renderer already
supports and GPU-tests round/butt caps and round/miter joins. Reuse it.

Relevant seams:

- `crates/motion/src/registry/style.rs`: operation mode/parameter contracts.
- `crates/motion/src/evaluation/style.rs`: retained `StrokeMode`/appearance;
  `evaluation.rs` calls `style::apply` with the builtin and arguments.
- `crates/motion/src/rendering/{screen,plane}.rs`: both currently hardcode Butt/Miter.
- `crates/motion/src/ui/graph_templates.rs`: registry-driven Stroke creation.
- `crates/motion/tests/style.rs`, `tests/text.rs`, shared `text_gpu.rs`, and app
  `motion_spatial.rs`: existing behavior and pixel oracles.

Choose one native contract and carry it consistently through Source, discovery,
evaluation/observations, UI and both rendering routes. Keep existing authored
butt/miter behavior valid; this is a current operation, not a legacy engine alias.
Qualify actual cap/join differences and persistence rather than only catalog counts.

Then address Transform with explicit pivot, public Tessellate, and general-profile
Extrude with attribute transfer. Finish group-pivot/sampling-basis acceptance.
The audit also retains M4 picking, resource/recovery, general transparency,
rapid-edit presentation and physical artist gates. Do not redefine M5 around
only the already-working integrated demo or silently move catalog duties to M7.

## Recent results and important limitations

- Camera Plane text double coverage is fixed for the tested fixture by replacing
  triangle subtraction with pinned `i_overlay` 4.4.1 contour extraction followed
  by the shared Lyon tessellator. Old subtraction/BVH code was removed.
- Typography Text/Outline equality, retained GPU frames, rectangle paint, World
  occlusion, translated cross-camera views and shared oblique/near-clipping tests
  pass. Follow linked evidence for exact coverage and tolerances; oblique depth
  uses a documented subpixel ray interval, not the constant-depth epsilon alone.
- `i_overlay` lacks cooperative cancellation inside synchronous graph operations.
  Input/output/cache caps and before/after checks do not prove intermediate-memory
  or worst-case cancellation-latency bounds. This remains a real implementation gap.
- Default Motion conversion budget is 2,000,000 checkpoints per frame, explicitly
  raised with user authorization. Geometry-size limits were not raised. The new
  library's internal work is not represented by those checkpoints.
- `demos/m5-integrated.py` uses actual public CLI/MCP discovery, proposals,
  inspection, Apply, CPU observations and public saving. All 35 operations now
  have authored graph positions. GPU-enabled runs add seven baseline frames and
  variation/restoration frames; exact restored pixels are asserted.
- Public frame time is half-open: the last demo frame is explicitly **95/24**,
  while CPU observations and native image tests retain exact time **4**. Do not
  claim a public frame was obtained at the excluded endpoint.
- Native service handoff tests cover artist ramp/key/lock/layout edits and public
  field/timing rebase, conflict, undo/redo and save/reopen. Physical artist
  interaction and normal/narrow graph review are not proved by those tests.

Current saved demo: `demos/m5-integrated.fold`; canonical public Source:
`docs/rewrite/evidence/m5-integrated.foldsource`. These have the new layout.
Latest public-frame reports use fresh IDs and have their own reviewed PNGs:
`m5-public-baseline.png` / `m5-public-variation.png` in the evidence directory.
Older captures must not be assigned to a newer artifact without verification.
Large raw reports/logs live under `/tmp` and may disappear in a new session;
committed evidence summaries and executable fixtures describe how to regenerate.

## Validation and environment

Use this profile to avoid huge debug builds:

```sh
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

Focused checks that recently passed (run those affected by the next change):

```sh
cargo test -p fold-motion --test style --test text --test spatial_controls --locked
cargo test -p fold-render --test planar_mesh --test spatial --locked
cargo test -p fold-render --features spatial --test text_gpu --locked -- --ignored --nocapture
cargo test -p fold-app --features spatial --test motion_spatial camera_plane --locked -- --ignored --nocapture
cargo test -p fold-app --test agent_integrated --locked -- --nocapture
cargo test -p fold-app --features spatial --test agent_integrated gpu_authoring --locked -- --ignored --nocapture
```

Consult individual evidence files for original executed combinations/results;
these commands are a focused starting set, not a claim of one final combined run.
Native Vulkan and private Unix sockets required sandbox escalation in this session.
Hardware: NVIDIA GTX 1070, driver 580.173.02, Vulkan; sandbox-only GPU may be llvmpipe.
Rust 1.95.0. Approximately 33 GiB disk space remained during recent checks.

The last full desktop workspace run: **649 passed, 4 failed, 101 ignored**.
One introduced stale cutover assertion was fixed and its five-test target passed
afterward. Three prior shared failures remain: media sequence resize expectation,
NLE export process capacity, and untagged MP4 profile expectation. See
`evidence/workspace-checkpoint-result.txt`. The complete suite was not rerun after
that correction or subsequent work. There is no final green workspace checkpoint.

At the final gate, run the prescribed full checks and classify failures honestly.
Do not claim phase completion from narrow tests. Update the plan with fresh evidence.
Physical desktop automation previously returned `runtime_unavailable` even after
the prescribed startup attempt; see `evidence/desktop-automation-recheck.txt`.
Recheck the environment when needed, but continue independent implementation if
that route remains unavailable. No subagents are authorized by default.
