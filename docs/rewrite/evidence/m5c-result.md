# M5c — parameter references, first Motion slice

9 October 2026. Baseline `cd5851c`; this record accompanies the M5c implementation checkpoint.
**M5c remains in progress. The user's request for universal references is not complete.**
M6 remains blocked by M5c and the earlier open gates.

## Implemented

- Native graph inputs appear on one Channel References node, including an empty
  container. Ports retain input identities through rename and reorder. Its layout
  is presentation data. The old per-input Control-node display is removed.
- The container node offers Add Parameter. Input Inspector menus provide name,
  type and Copy Parameter controls. Changing an incompatible input type still
  fails the normal semantic/transaction gate; it does not coerce its consumers.
- `fold-graph` owns escaped path normalization, owner-relative paths, typed component
  access and native parameter references. Motion resolves within one definition.
  `ch("width")`, `ch("../controller/width")`, container `ch("../Width")` and
  definition-absolute `/definition/node/parameter` paths resolve to stable IDs.
  Both `.5` and `0.5` syntax are accepted. Canonical source prints current paths.
- Parameter references and expressions reuse the existing type checker and CPU
  evaluator. Dependency cycles reject. Values are cached by parameter and instance
  path within a request; element/time sampling preserves existing cache isolation.
  Vector components keep distance units; color and quaternion components are scalar.
- Value parameters now accept native tracks directly. Animate no longer creates
  a separate keyframes node. Explicitly authored keyframes operations still have
  independently useful behavior and remain available.
- Motion node-parameter Inspector actions use shared SDK menu items: Copy Parameter,
  Paste Relative Reference, Edit Expression, Go to Source and Remove Reference.
  Copy retains identity, so a rename between Copy and Paste is safe. Expression
  drafts retain a base value and reject stale or invalid Apply. Normal protection
  and transactional edits apply. Locked controls can still be copied.
- Driven values are sampled on one cancellable worker per Inspector, using the
  existing prepared evaluator and exact selected occurrence clock. Results from a
  different revision/selection/time are discarded. Removing a reference requires
  a current sampled primitive value and retains that exact value.
- Public discovery describes the qualified scope and explicitly reports
  `cross_document: false`. Source proposals and native bindings use the same
  contracts. Fold skill guidance defaults to container controls, inline expressions
  and inline tracks rather than value-carrier primitive nodes.

## Verification

- Workspace all-target/all-feature check passed with the pinned native runtime.
- Graph and Motion suites passed before the final UI refinements. Focused tests
  then passed for renamed-path/source/persistence agreement, cycle rejection,
  inline animation and nested forward/reverse occurrence sampling.
- Real CLI and MCP transport test
  `cli_and_mcp_author_relative_parameter_expressions_without_primitive_nodes`
  passed: discovery, inspect, source proposal, candidate inspection, Apply,
  evaluated 200 × 0.25 = 50 canvas units, unchanged node count, one undo group
  and redo. Initial sandbox socket denial was an environment restriction; the
  permitted local-socket run passed.
- Existing animation tests were updated to inspect the track on its authored
  parameter instead of assuming a newly generated keyframes node. Existing
  graph rewire tests now select the input's stable port on Channel References.
- Package boundaries/headless isolation and whitespace checks passed.
- Final checkpoint and rendered capture results are recorded below.

Build environment: `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, offline Cargo.
The old handoff's `/tmp/fold-codec-runtime` and `/tmp/fold-ocio-package` were absent.
The workspace check used
`FOLD_VIDEO_ROOT=/home/rgame/Documents/fold-native-pipeline/native-worker/runtime`
and `FOLD_COLOR_ROOT=FOLD_TEST_COLOR_ROOT=/home/rgame/Documents/fold-native-pipeline/release-package`.
The initial checks with missing runtime paths failed before compilation; they are
not code regressions. No development application was restarted or modified.

## Required continuation — not approved deferrals

1. Complete the shared project-wide parameter binding and evaluation
   contract across provider-owned documents. Read-only discovery and sampling
   are now implemented for the qualified adapters below. Existing Compositor configuration
   readers still require literals; Composition layer channels and Timeline
   controls still have separate provider-owned storage. Replace their assumptions
   through explicit provider interfaces, not arbitrary JSON mutation or a second
   expression engine.
2. Resolve absolute composition/scene/object/graph paths and relative paths across
   those scopes. Bind exact occurrences for reused documents/assets and map one
   composition instant through source clocks. Qualify cross-provider cycles,
   changes of owner, duplicate remapping and dependencies after save/reopen.
3. Finish every parameter Inspector, component destination, instance override and
   container binding workflow. Current menus cover Motion node parameters and
   copying container inputs; they do not establish universal parameter UI support.
4. Complete public parameter catalog/navigation/edit/sample tools and end-to-end
   cross-provider agent examples. Current examples qualify definition-local Motion
   only; they must not silently expand that claim.
5. Complete physical artist continuation, invalid/error presentation, rapid editing,
   cancellation, preview/headless/export agreement and the full M5c gate. Native
   offscreen captures and automated actions are not physical artist acceptance.

## Final checkpoint results

- Workspace `cargo check --workspace --all-targets --all-features --offline`: passed.
- Workspace `cargo clippy --workspace --all-targets --all-features --offline`:
  completed with existing warnings. New path-helper and evaluator lint findings
  were corrected. Final Graph/Motion all-target/all-feature Clippy completed with
  existing warnings only.
- Workspace `cargo test --workspace --all-targets --all-features --offline
  --no-fail-fast -- --test-threads=1`: **725 passed, 20 failed, 118 ignored**.
  The 19 `agent_observe` failures shared one obsolete fixture assumption that
  Animate creates a separate keyframes node. The fixture now edits the track on
  `height`; its final complete suite passes **19/19**. The remaining failure was
  the new `.5` expression test running against the earlier compiled parser during
  this continuation. The final parser accepts leading-decimal literals and the
  complete parameter-reference suite passes **3/3**. The whole workspace suite
  was not repeated after these focused corrections; this is not a single green
  full-workspace run.
- Final Graph/Motion suite: **170 passed, 9 ignored**. Two additional focused
  native regressions then passed: Add Parameter retains node count and uses one
  undo transaction; deselect/reselect during cancelled inspection starts a fresh
  request. The last complete Motion library plus parameter-reference run passed
  **45 tests, 9 ignored**. These overlap earlier counts and must not be added as
  independent test coverage.
- Real CLI/MCP source-authoring regression: **1 passed**, covering both transports.
- Native graph capture: **1 passed**. Reviewed [normal/narrow output](m5c-network.png):
  Channel References and its Width port are readable at both widths; ordinary
  canvas panning retains the rest of the graph. The capture fixture now uses
  independent editor instances for the two widths. This establishes offscreen
  graph presentation, not Inspector-menu interaction or physical acceptance.
- Formatting, package boundaries/headless isolation and `git diff --check`: passed.

Raw session logs remain under `/tmp/fold-m5c-*.log`; the tests and capture fixture
are durable, reproducible evidence. No earlier gate is closed by these results.

## Continuation — shared parameter inspection and sampling

The next implementation adds a provider-owned read service in `fold-platform`.
Stable targets contain document, optional object, nested occurrence IDs, parameter
ID and optional component. Paths are current name projections; project discovery
prefixes the document name and does not treat organizational bins as evaluation
scopes. Ambiguous names reject resolution. Unsupported document schemas remain
opaque and are skipped during project discovery.

Motion adapters enumerate reachable graph occurrences, including container inputs
and typed components. They use the existing semantic checker and Prepared evaluator.
Scene-object sampling applies ancestor clocks before graph clocks, in the same order
as rendering. Protected instance overrides report a locked effective input. Native
Compositor controls and animation channels are discoverable; primitive sampling
uses `evaluated_parameters`, the same configuration sampler used by compilation.
Structured configuration, lists and ramps remain discoverable but are not primitive
samples. No alternate expression engine or JSON-pointer editing was added.

Public `inspect_parameters`, `resolve_parameter` and `sample_parameter` commands
run on the existing supervised CPU workers against an immutable revision. Inspection
has pages of at most 256 entries; project and occurrence expansion are bounded.
Stale revision/epoch, missing targets, invalid components and ambiguity reject.
Discovery distinguishes these project inspection paths from the still-local Motion
`ch()` binding scope. The Fold public workflow now documents the verified calls.

Verification for this continuation:

- Motion parameter-service tests: **4 passed**, covering rename/reopen, escaped
  paths, document identity, ambiguity, cancellation, root overrides/protection,
  component units and reversed scene-object clocks.
- Existing parameter-reference suite: **3 passed**, now also comparing both reused
  forward/reverse occurrences through the shared provider service at random seeks.
- Complete Compositor native-authoring suite: **11 passed**, including a new test
  that reads the same animated opacity as render configuration and retains its
  native target after rename.
- Extended real CLI/MCP regression: **1 passed**, covering the three new methods,
  exact agreement with the native sample and stale-revision rejection. Sandbox
  socket creation failed first; the permitted local-socket run passed.
- App all-target/all-feature check passed during implementation. Final static checks
  are recorded below. No UI change or physical desktop acceptance occurred in this
  continuation.

Remaining required work is unchanged in scope: cross-provider expression evaluation
and dependency/cycle handling; common-composition-time mapping across documents;
Composition and Timeline storage/adapters; full Compositor bindings; unused graph
definitions and scene placement controls; every Inspector and component destination;
transactional public parameter editing; preview/export/native acceptance. The new
read service does not establish any of those gates. M5c remains in progress.

Logs: `/tmp/fold-m5c-adapters-final.log`,
`/tmp/fold-m5c-service-transport.log`, `/tmp/fold-m5c-public-service-check.log`.

Final continuation checks: App all-target/all-feature Clippy completed with existing
warnings only (`/tmp/fold-m5c-service-clippy.log`). The strengthened scene test now
also places the object under a parent with a 2× clock and verifies the composed
parent/child time and hierarchy path; the final service suite remains **4/4**
(`/tmp/fold-m5c-service-parent-final.log`). Formatting, whitespace, package boundaries
and headless UI isolation passed. The workspace-wide suite was not repeated in this
continuation; the earlier checkpoint record above still applies.

## Continuation — feature controls and placed-source clocks

The shared target now distinguishes native graph IDs from fixed provider property
keys. Property keys are matched by the owning feature, never interpreted as JSON
paths. The stable owner ID survives rename and reparent; public clients retain the
complete target from discovery. Existing project payloads were not rewritten.

Implemented read adapters now include:

- Composition layer name, visibility, lock, exact range/source times, opacity,
  adjustment state and six local affine components. Animated values use the same
  layer sampler and matrix as compilation; translation components carry Canvas units.
- Timeline track name/enabled/solo/lock and clip timing/level controls. Video clips
  also expose position, scale and rotation, including vector components. Clip paths
  contain the full native clip ID because clips have no authored name. Audio controls
  do not acquire unused video transforms.
- Motion scene placement position/rotation/scale, name/visibility/lock, clock offset
  and exact rational rate components. Placement tracks use the same native sampler
  as scene evaluation, after parent/object clocks.

Provider-declared source placements now connect Composition layers and Timeline
clips to source documents. A target can retain a root document and the complete
ordered owner-ID chain. Discovery follows declared dependencies, checks available
source outputs and carries inherited locks. Sampling maps the root instant through
each source origin/start and then the source provider's authored-time conversion;
Motion canvas starts are included exactly once. Repeated uses of one document stay
separate. Missing owners, changed source documents, missing outputs, cycles, excessive
depth/expansion and cancellation reject. Direct document sampling refuses a placed
target because it lacks the immutable project snapshot needed for time mapping.

This is parameter inspection, not video coverage: a control can be sampled outside
a visible layer/clip range. No held/black-frame value is substituted. Reusable-effect
occurrences, cross-document expression evaluation and writing those references are
still required. The source-clock service is isolated in
`fold-platform/src/packages/parameter_service.rs`; providers own all field access.

Automated evidence during this continuation:

- Composition property test and complete Timeline sequence suite passed: **9 tests**.
  They verify animated transform/opacity values, exact times, typed components,
  rename/reparent/track moves, inherited locks, persistence and rejection of arbitrary
  property paths.
- Motion reference/service and Compositor native-authoring suites passed after the
  target-key change: **18 tests**. The scene test was then strengthened to check
  animated placement components under a reversed object and a 2× parent clock.
- Complete CLI/MCP handoff suite passed: **5 passed, 1 ignored**. The new public
  regression reads Composition and Timeline property targets, resolves their paths,
  and samples a Motion source through a Composition placement. The ignored test
  requires native Vulkan frame acceptance and remains outside this read-service gate.
- Native placed-source test passed through Timeline → Composition → Composition →
  Motion. It checks separate source offsets, a nonzero Motion canvas start, source
  lock propagation and stale-owner rejection. The final run also checks missing
  source-output rejection.

Final focused checks and logs are recorded below. These results do not close M5c,
M6, the existing desktop-freeze investigation, or physical artist/export acceptance.

Final continuation checks:

- Combined focused suites: **28 passed**, covering all five affected feature/host
  packages (`/tmp/fold-m5c-controls-final.log`). This includes the strengthened
  scene-placement clock test and missing source-output rejection.
- CLI/MCP handoff plus initial placed-source run: **6 passed, 1 ignored**
  (`/tmp/fold-m5c-placements-public-final.log`). The placed-source test overlaps the
  combined 28-test run; do not add it twice as independent coverage.
- All-target/all-feature Clippy for App, Motion, Compositor, Composition and Timeline
  completed with existing warnings only (`/tmp/fold-m5c-controls-clippy.log`).
- Formatting, whitespace checks and package boundaries/headless isolation passed.
- No live application restart, native UI edit, physical acceptance, full workspace
  test rerun or export qualification was performed in this continuation.

## C4 ownership continuation — 9 October 2026

[Maintenance C4](../../maintenance/evidence/c4/result.md) replaces App's concrete
proposal parsing/reconciliation with the provider-owned capability. Source,
protected controls, stable identities, parameter references and mixed-document
publication retain their owning models and existing project transaction authority.
Provider schemas/catalogs are discovered under `scoped_proposals[].requests`;
public clients and the Fold skill follow that contract. The C4 record separates
implementation, automated CLI/MCP verification and unperformed native acceptance.
This continuation closes no remaining M3/M5a/M5c gate and does not begin M6.

C4 automated checkpoint: 772 workspace tests, 5 explicit native frame/CLI/MCP
checks and 16 Python tests passed, along with workspace/headless checks, Clippy
and dependency/ownership checks. The ordinary suite ignored 117 tests; the five
native checks were run separately. Physical artist acceptance remains open.
