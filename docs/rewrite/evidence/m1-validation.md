# M1 cutover checkpoint — 8 October 2026

The removal gate passed: the old engine and transports are absent, the application
builds and runs without Motion, and unsupported payloads remain opaque. This is
not a claim that every inherited workspace test or warning is resolved.

## Verification

- `cargo check --workspace --all-targets --features fold-app/desktop --locked --offline`: passed.
- `cargo build -p fold-app --bins --features desktop --locked --offline`: passed.
- `cargo test --workspace --features fold-app/desktop --locked --offline --no-fail-fast -- --test-threads=1`: completed all 92 result groups, 425 passed, 4 failed, 71 ignored. Ignored checks include packaged OCIO, native GPU/video and release performance fixtures.
- The introduced placement-fixture mismatch (Solid fixture asserting Vector output) was corrected to a native compositor rectangle. `cargo test -p fold-app --features desktop --locked --offline --test project_placement --test common_composition -- --test-threads=1`: all 15 passed afterward. The earlier deleted-Motion layer fixture was also corrected; all 29 app unit tests passed in the full run.
- Cutover tests passed: old Motion payload/extension save and reopen, provider rejection, compositor rendering through undo/redo, removed agent CLI rejection, and retired graph-child metadata preservation.
- `cargo clippy --workspace --all-targets --features fold-app/desktop --locked --offline`: completed successfully with inherited warnings. The introduced one-element test loop was removed and Clippy rerun. The stricter optional `-- -D warnings` run failed on unchanged media warnings; no lint suppression was added.
- `cargo fmt --all -- --check`, `git diff --check`, package boundaries and all 16 Python tests passed.
- [Runtime evidence](m1-runtime.json) records headless and CPU/GPU desktop smoke, unsupported native opening, screenshots and binary hashes. [Physical GPU sampler check](m1-baked-physical.txt) passed the replacement sampler's ignored parity/cache test.

## Unresolved shared failures

These paths are outside the removed Motion engine. The listed implementations are
unchanged from baseline `66ebb2ece465189a2f8dd3e415b38f63a62bdd6a`; a complete baseline
workspace execution was not performed, so this record does not claim a measured
before/after baseline test run.

| Test | Observed result | Unchanged path evidence |
| --- | --- | --- |
| `media_sequence::input_output_transfer_roundtrips_every_rgb8_value` | RGB8 roundtrip passed, but the final assertion expected resized media to fail. The existing preparation path resizes it successfully. | `render/{preparation,sampling}.rs` and the relevant `render` entry point are unchanged; the test's only cutover change removes unused compile-context fields elsewhere. |
| `video_workflow::saturated_colors_and_explicit_profile_rejection` | Untagged video was accepted, contrary to the assertion. | Entire test file and `media/video_format.rs` are unchanged; the format validator explicitly accepts `unknown` tags. |
| `nle_workflow::multitrack_nle_edit_mix_render_save_reopen_and_export` | Export failed with `media process capacity exhausted (6 regular; 2 reserved for audio)`. An isolated repeat failed identically. | Entire test and media process/decoder implementation are unchanged; no Motion provider participates in this fixture. |

These three failures remain reported, not disabled or presented as passing. Normal
Timeline/Composition/Compositor tests and the cutover smoke passed. Physical screen
presentation, artist interaction and new Motion behavior are not M1 acceptance claims.

## Removal audit

Removed 155 old Motion Rust source files, v1 app automation/MCP/CLI, SDKs, Wrangles,
Solver, compiler/toolchain, candidate glue, creation actions, old demos and sole-use
render paths. The lockfile removes 61 packages with no added or upgraded versions.
Shared image sampling, compositor vectors and provider services have independently
traced consumers. The indirect audit also removed old Composition object-context
execution. Current source references to old package IDs occur only in negative
cutover tests; the generic project loader never decodes those payloads.

Logs: `/tmp/fold-m1-workspace-tests-final.log`,
`/tmp/fold-m1-corrected-fixtures.log`, `/tmp/fold-m1-clippy-final.log`,
`/tmp/fold-m1-final-check.log`, `/tmp/fold-m1-nle-repeat.log`.
