# M2 native document/language/reference gate — 8 October 2026

The M2 gate stated in the rewrite plan passes automated native-service acceptance.
This does **not** close the combined M2/M3 public-tool/native-artist acceptance or
claim application registration, rendering, protocol completion or performance.
Those remain required in the subsequent phases; the full `verify_slice.py` oracle
has not been satisfied.

## Requirement-to-evidence audit

| M2 requirement | Implementation and inspected evidence |
| --- | --- |
| Versioned native document and one registry | `model.rs` enforces the new package/type/schema. `registry.rs` owns operation/version/mode, defaults, typed ports, domains, costs, qualification flags, JSON schema, catalog entries and source signatures. Registry tests check unique contracts, defaults and schema-reference closure. |
| Stable native identity and unknown data | Definition/input/node/parameter/edge/instance/output/track/key UUIDs survive serialization and source printing. Model and reference tests preserve unknown fields, unresolved node/version parameters and wrapper metadata. Old package identity is rejected; generic old-document preservation was qualified in M1. |
| Typed language and source mappings | `source`, `expression_source` and the private declaration parser implement bounded native source, scoped names, exact literal parsing, diagnostics and byte spans. Canonical reparse compares the whole construction, not only rendered values. Mapping tests check remapped source spans refer to retained native IDs. |
| Asset-fragment reconciliation | `parse_fragment` resolves one asset within retained context; `prepare_fragment` replaces only that asset. Explicit maps are simultaneous, injective and kind-checked against the selected definition. Missing mappings do not infer identity from names. Tests reject outside/kind/duplicate mappings and verify protected controls/layout survive mapped edits. |
| Project transactions and protection | Private immutable candidates commit one named coordinator edit; stale revisions/documents fail. Source buffers retain rejected text without publishing it. Tests cover scoped rejection, protected parent/override/context/group cases, undo/redo and wrapper retention. Deletion cleanup removes only presentation references to deleted selected-asset records; locked deletion still fails. |
| Reusable assets and exact clocks | Reference tests evaluate original/variant instances in one request, with size/color overrides and offset/reverse/frozen clocks. The rich asset construction is source-round-tripped, saved/reopened and changed through one undo/redo group without changing the child definition. |
| CPU reference evaluation | `execution.rs` and `evaluation.rs` share bounded typed IR for time/element contexts. Tests cover checked time/integer arithmetic, unit/space checks, quaternion shortest arcs, explicit channel combinations, work/element limits, cancellation and node/asset cycles. Cycle errors are checked for participating IDs. |
| Deterministic element fields/random | A positive element-context test evaluates a World/Meters position field, rejects a space mismatch and proves random output is unchanged by seek time or element iteration index for the same integer element ID. Separate tests distinguish adjacent IDs above 2^60. |
| Animated-shape proof | `tests/reference.rs` builds the native graph with typed inputs, bounded width expression, Tween, Keyframes, placement and output. It checks frozen fixture times in forward/reverse/shuffled order, scalar tolerance 1e-9 and exact IDs. Reparsed source produces identical samples. No legacy source/compiler/runtime participates. |
| Persistence/recovery gate | Source and asset tests compare full native state through canonical parsing, project save/reopen, undo and redo, including tracks, overrides, graph positions, annotations and locked groups. Unresolved operation payloads are saved/reopened unchanged. Invalid source and locked proposals leave the last committed project revision untouched. |

## Commands and results

- `cargo test -p fold-motion -p fold-foundation --locked --offline`: **36 passed**
  (22 Motion unit, 10 Motion integration, 4 foundation), zero failures.
- `cargo clippy -p fold-motion --all-targets --locked --offline -- -D warnings`:
  passed.
- `python3 scripts/check_boundaries.py`: passed.
- Touched Rust formatting and `git diff --check`: passed.

Logs: `/tmp/fold-m2-gate-tests.log`, `/tmp/fold-m2-gate-clippy.log`. Tests execute the
current worktree, with the root Cargo lockfile; no dependency versions changed for
this gate. Motion enables serde_json's `float_roundtrip` feature because an exact
quaternion round-trip test demonstrated a one-ULP error without it. No tolerance
was relaxed. Temporary project archives are created and reopened by the tests.

Workspace-wide tests/Clippy remain scheduled at M3's vertical-slice checkpoint.
The shared M1 exceptions remain documented separately and are not reclassified as
fixed by these focused tests.

## Explicit limits and next phase

The app still has no new Motion provider or agent transport registration. Native
node/control/timeline interaction, explicit artist unlocking, disjoint rebasing,
expected-value conflicts, candidate generations, idempotency, expiry and observation
isolation are M3 work. M4/M5 rendering and the integrated scene remain unqualified.
GPU flags stay false. No artist acceptance or public-tool execution is inferred
from the Rust tests.

Unresolved records remain losslessly parseable/persistable but cannot enter the
execution-qualified apply service. External resource execution is unavailable until
ownership integration. Source is bounded to 64 KiB; scoped fragments avoid requiring
unrelated assets in a request. Native locks protect records/owning routes, not a
frozen raster result under arbitrary independent upstream changes.
