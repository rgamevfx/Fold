# Phase 51 — Composition authoring

**One Composition type owns a shared layer stack. Each layer contains a MoGraph graph or a Compositing graph; footage and nested compositions also fit in that stack. Graph types are layer content, never alternative composition types.**

Status: complete. On 2026-10-07 the user accepted the corrected UI ("It looks good") and requested phase completion and commit. Earlier editing-model and saved-workspace failures are corrected; verification is recorded below.

## Direction and contracts

- Composition owns layer hierarchy, visibility, lock, affine transforms, opacity, ranges, source clocks, keys and curves. Independent Motion and Compositor packages own their graphs and evaluation; Timeline owns sequences.
- Retire standalone Animation and MoGraph windows. Network edits the selected layer graph. **Phase 51a supersedes the original exception:** all keyframe and curve editing belongs in Composition, including graph-internal parameters. Network remains a graph.
- Composition retains its own editor context, group badge, selection and local clock independently of Inspector locking and layer graph navigation. Explicit viewer clocks and ambiguity checks remain shared SDK behavior.
- Nested compositions have independent dimensions, duration and rational frame rate. Movement shifts source origin; trimming does not. Adjacent sibling layers/groups can be precomposed atomically. A group and an independently timed nested composition remain distinct.
- Project changes use transactions and undo; rendering uses immutable snapshots and registered output capabilities. Unknown extension data remains preserved. Feature packages do not parse each other's models.
- **User adjustment: no legacy-project shims, automatic adoption or conversion fallback. Update project files directly to the current model.** Opening a file must not silently rewrite it or make it dirty. Existing provider schema readers are outside this correction's scope.
- Light Wrap is excluded.

## Current artist workflow

1. **Project → New → Composition** creates the shared composition.
2. **Composition → Add MoGraph layer / Add Compositing layer** adds either kind of graph in one undo step. Both kinds use the same visibility, lock, timing and layer controls.
3. **Select a layer** to open its actual graph in Network while retaining the parent stack, selected layer and clock. Expand Badge directly in Composition to select Text or Background. The Inspector presents shared layer controls followed by native graph properties; Network’s Tools menu retains the native MoGraph tools.
4. **Layer → Add image processing graph** adds editable processing downstream of the source. Named image sockets connect independent graph providers; cycles are rejected atomically. Group outputs must be precomposed before connecting.
5. Place footage or another composition into the stack, or create/precompose a nested composition. Source navigation retains exact local time and parent return.

Internal graph documents are marked as layer content and hidden from the Project root listing. Default output selection prefers the common composition. Explicit output selections remain authoritative.

## Implementation and cleanup

- [x] Common composition package, layer state, mixed source rendering, nested clocks and transactional authoring.
- [x] Shared layer tracks and curve controls; remove obsolete Animation registrations, dispatch and duplicate editor state.
- [x] One composition creation workflow; atomic graph creation and named image input connections.
- [x] Preserve parent Composition while editing layer graphs.
- [x] Remove the automatic adoption module, conversion command and registration, background-open mutation and their compatibility-only tests.
- [x] Restore a Composition editor context when the saved layout lacks one; refresh restored bindings against the current project; explicit document opening creates a missing registered editor.
- [x] Update `fold-badge-live.fold` and `fold-badge-color-ramp.fold` directly. Original public composition IDs and sequence references remain stable; editable graphs, resources and object relationships remain intact in layer content. Live SDK bindings point to those source graphs.
- [x] Complete correction verification and native artist acceptance.
Public agent authoring for the common Composition/layer model remains a separate follow-up in `dev-plan.md` under Live agent access and reusable workflows. Existing live SDK examples build provider graphs; they do not reproduce the updated common stack.

## Badge Wave structure

Both demo files expose **Duplicator**, **Badge/badge** (hidden source), and **Background** as common root layers, with their original identities, visibility, lock and ranges. **Text** and the fitted **Background** are actual child layers under Badge in Composition. Selecting any row opens that row’s original graph, including the Duplicator’s Source → Duplicator → Transform network and its Oscillator/Color Ramp connections. Object references stay inside the original editable source graph. The finishing Composition contains a Compositing layer reading the main Composition; the Sequence reads that finishing Composition.

The original graph nodes, resources, artwork and color policy are retained. Badge’s overlapping group and Transform nodes are positioned separately. This is a direct artifact update, with no runtime format-conversion path.

## Verification

Earlier focused checks covered mixed rendering, visibility/animation, exact nested clocks, move/trim semantics, cycles, undo/redo, persistence, named provider inputs, Project placement, compositor media export with audio, and normal/narrow native controls. Those checks did not certify opening Badge Wave with the user's saved workspace.

Correction evidence is recorded below after running against the updated files. Full workspace tests and Clippy remain deferred. Live desktop interaction, mixed/nested GPU parity and sustained playback must not be inferred from headless or isolated UI tests.

### Earlier file/open correction results (superseded UI acceptance)

- The saved-workspace context regression failed before the shell fix and passes afterward. All 18 shell tests pass, including independent clocks and parent-context preservation.
- All 22 focused app tests pass: six common-composition tests, seven compositor-session tests, three Motion-workflow tests, two delivery-selection tests and four Project-placement tests. The demo test loads both checked-in files, verifies the common root layers and graph roles, renders their outputs, and checks that normal session opening preserves content without setting dirty state.
- A temporary comparison harness rendered original and updated Motion/main, compositor/Finish and Sequence outputs for both projects at 0, 1.25, 3.25 and 6.25 seconds (96×54 CPU frames). All 24 comparisons passed; maximum channel difference was 1.79e-7. The harness was removed after verification.
- Package boundaries/headless isolation, touched-file formatting and diff checks pass.
- The corrected group badge has automated context coverage; its live desktop appearance and interaction have not yet been visually rechecked. Earlier normal/narrow captures cover the shared controls, not this saved-workspace restoration case.
- Release desktop build passes (`cargo build -p fold-app --features desktop --bin fold-desktop --release`). The running app was not restarted or its open project overwritten; restart and reopen the updated demo to load the correction.


## Hierarchy and direct-graph correction

The wrapper-network implementation was rejected by the user: preserving rendered pixels while hiding Badge’s children and replacing per-layer networks was the wrong editing model.

- [x] Delete the common source-image wrapper Network and its position state. Remove its artist-facing terminology and command names.
- [x] Store Badge’s full parent/child hierarchy in the shared Composition. Preserve original graph/node IDs, text, fonts, modifiers and object references.
- [x] Selecting a layer requests its native graph and object scope without navigating the parent Composition away. Native graph selection resolves back to its owning layer. Graph properties and shared layer properties remain available together; inherited locks apply to either graph type.
- [x] Resolve shared layer hierarchy/state and exact source clocks through an explicit graph-evaluation capability. Providers validate the context contract; declared dependencies and cycle checks remain enforced.
- [x] Shared layer edits and MoGraph metadata changes commit atomically. Native MoGraph creation adds its layers and children to Composition in the same transaction. Node-only edits leave the shared hierarchy unchanged.
- [x] Retain native Tools and modifier-group navigation. Phase 51a moves scoped graph animation into Composition. Reset stale group scope when selecting another layer.
- [x] Update both Badge Wave files directly; no automatic project adoption or compatibility shim.
- [x] Finish current verification and native artist acceptance.

Common groups support mixed MoGraph, Compositing and nested-composition layers. A graph group such as Badge consumes geometry children in its own graph. Precomposition must include the other layers sharing that graph so native object references are not cut across composition boundaries; rejection is atomic. Ordinary independent layers and common groups keep their existing nesting workflow.

Verification must include the actual demo’s hierarchy, distinct Duplicator/Badge/Text graph scopes, child property propagation through object references, undo, native graph edits, native tool creation, mixed graph types and normal/narrow panel captures. Pixel parity is additional evidence, not a substitute for these checks.

### Current correction verification

- Reproduced both reported editing regressions before fixing them: the demo hierarchy test failed because Badge children were absent, and the native graph-selection test failed because selecting Duplicator did not select its graph scope. Both now pass.
- All 26 focused app tests pass: nine common-composition tests, one native panel capture, seven compositor-session tests, three Motion-workflow tests, two delivery-selection tests and four Project-placement tests. Coverage includes the five-layer Badge hierarchy, child visibility/opacity/offset/range propagation into referenced copies, native graph edits, native tool creation, atomic nesting rejection, full-hierarchy precomposition, undo/redo and clean project opening.
- All 135 affected library tests pass: four common Composition, 36 Motion and 95 shared UI tests (five separately gated native probes ignored). The shared UI tests include all 18 shell tests. Package boundaries/headless isolation and diff checks pass.
- Normal/narrow captures use the actual registered Composition, MoGraph, Compositing and Inspector panels with the checked-in Badge Wave project. They assert native graph routing, retained parent selection, graph-to-layer ownership and no project mutation from viewing. Captures are in `/tmp/fold-badge-layer-ui/`.
- User acceptance of the corrected UI is recorded above. Agent-driven interaction in the running desktop and sustained playback were not performed; full workspace/Clippy checks remain deferred. The public common-composition authoring gap remains tracked separately.

- Fresh post-correction comparison: all 24 original/current main, Finish and Sequence frames pass across both demos at 0, 1.25, 3.25 and 6.25 seconds (96×54 CPU rendering; maximum channel difference 1.79e-7). The temporary comparison harness was removed. Native captures were regenerated after fixing Badge node overlap and hiding the internal object ID, then visually reviewed.
- Corrected release desktop build passes (`cargo build -p fold-app --features desktop --bin fold-desktop --release`). The running app and unsaved session were left untouched; restart and reopen the updated checked-in demo to load the corrected binary and project.

## Phase 51a — Complete animation consolidation (2026-10-07)

Implementation complete. Reopened after the user found that Network's Animation checkbox still opened the old authoring surface. The original phase-51 exception was the wrong requirement; the earlier completion did not certify this workflow.

- Composition has exactly **Track view** and **Curve Editor**. Track view replaces the Dope Sheet. Keyed shared-layer and graph parameters appear under their owning layer; both views edit the same channels.
- Remove both Network Animation toggles, their state, and the obsolete Compositing/MoGraph animation and scene-panel renderers. Retain provider animation data/evaluation and shared curve controls.
- Providers expose channel metadata and apply edits through the platform contract. Composition translates graph key times to its own clock for display, then maps edits back exactly. Modifier internals, constraints, masks and keyframe drivers retain provider ownership.
- Graph-key and common-layer changes commit together, with preview/cancel and one undo entry. Auto Key is shared with graph inspectors. View switching reveals the graph and fits the selected channels; channel discovery and channel selection refresh framing.

Verification (dev-plan item 51a):

- The new graph-channel regression first failed with zero channels for a keyed MoGraph Width parameter, then passed after provider discovery was connected.
- Six focused app integration checks pass: graph discovery, compositor/modifier-internal edits, shared Auto Key, atomic common-layer and graph-key edits, exact source time on key removal, evaluated output, locks, unknown-data preservation, undo/redo and save/reopen. Existing mixed-layer creation and native authoring cases remain covered.
- Common Composition: eight tests pass, including channel projection, exact source-clock round trips, last-key value retention and locking. Shared editor: 14 focused tests pass, including actual mode-button clicks, time/value key dragging, Bézier handle dragging, cancellation, one-shot commits, layer summaries, timing, locking and normal/narrow layout. MoGraph: 26 UI tests pass; Compositor: seven UI tests pass. The obsolete MoGraph settings/scene-panel capture cases were removed; Network stays a graph when the keyframe shortcut is pressed.
- The registered Composition panel's native renderer harness inserts a source-graph key, switches views, drags that key in the curve graph, verifies its exact source time and value, and undoes/redoes both edits. Normal/narrow Track view and Curve Editor captures are in `/tmp/fold-composition-animation-ui/`; images were visually inspected. A scripted input-sequence error in the expanded drag fixture was corrected before its successful run.
- Touched Rust formatting and diff checks pass. The release desktop build passes: `cargo build -p fold-app --features desktop --bin fold-desktop --release --offline`.
- `scripts/check_boundaries.py` reports the existing direct `fold-ui -> fold-media` and `fold-ui -> fold-project` dependencies. Both are present in unchanged HEAD's `crates/ui/Cargo.toml`; this change does not introduce them. Workspace-wide tests and Clippy remain deferred.
- Interaction evidence uses events injected directly into Dear ImGui, including the actual registered Composition panel and transaction host. It does **not** certify operating-system mouse dragging in the running desktop, which the user reports may be limited through computer use. The running app was not restarted; restart to load the rebuilt binary. No new artist acceptance is claimed.

### Follow-up audit and Rust Orbit acceptance fixture

The audit found a shared-key ownership defect and incorrectly treated an obsolete scene-opacity authoring path as a compatibility requirement. Review scope was the working implementation against HEAD, excluding the user's pre-existing Timeline demo changes; the original user request is the specification.

#### Standards

One material finding: projected graph keys reused their provider IDs across layers. Because editor selection is global by key ID, moving one of two shared-source tracks also shifted the other projected keys and could retime the shared source. This breached explicit identity/ownership. Corrected with stable layer/property-scoped UI identities mapped back to provider IDs, plus canonicalization of equivalent shared-source proposals. Unedited aliases refresh from the committed source; retaining the actual editing owner during previews avoids stale gesture baselines and incorrect source-clock sampling when removing the last key. A regression covers range moves, multiple drag frames, returning to the original value and last-key removal.

#### Spec

The shared-key collision violated reliable Composition editing and is corrected. The added whole-scene opacity discovery/edit fallback was the wrong solution: the user explicitly rejected compatibility for retired designs. That fallback, its dedicated graph edit path, the separate graph-inspector opacity widget and public object-opacity setter are removed. Composition owns layer opacity and keys. Shared layer state still feeds graph evaluation; it is not exposed as graph-authored animation. Network's old Animation toggles/renderers remain removed; both Composition views use the same channels.

Rust Orbit is now a Composition with an owned Orbit dots graph layer. Radius and Speed have flat Bézier keys at 0 and 4 seconds, retaining the original values and artwork. Adoption marks the bound graph as layer-owned so selecting its layer opens Network while preserving Composition. Existing graph definitions, source code and IDs remain intact. All artifact edits and saves used public agent transactions.

Verification adds two channel-projection regressions, a provider ownership regression, and the actual saved Orbit native UI test. The latter inserts at one second in Track view, switches to Curve Editor, drags the inserted keys, checks source time/value and changed rendered pixels, verifies one commit per edit, undo/redo, and edited-project save/reopen. Both views were captured and visually inspected at 900px and 280px panel widths. Source and migrated Composition renders match at 0, 1 and 4 seconds at 640×360. The checked-in demo retains its original appearance; test edits are confined to temporary copies. See `demos/fold-rust-orbit.md` for the artist workflow.

Review disposition: Standards key-identity finding corrected; Spec key editing corrected, and the proposed scene-opacity compatibility requirement rejected and removed. OS-level desktop dragging and artist acceptance remain unverified; the native interaction harness injects Dear ImGui input.

Final follow-up checks: 14 app Composition integration tests, two native Composition UI tests (including saved Rust Orbit), two channel-projection tests and one whole-scene provider test pass. Final release desktop/CLI build, touched-Rust formatting and diff checks pass. Existing boundary failures and deferred workspace/Clippy scope remain as recorded above.

User clarification: update only `demos/fold-rust-orbit.fold`; no other demo migration. Orbit already has the current Composition/layer structure and Radius/Speed keys. The additional ownership regression edits opacity through Composition, checks evaluated opacity and graph-channel ownership, and undoes/redoes the transaction without changing the saved demo. The native Orbit Track view/Curve Editor regression remains the project-level acceptance test.

Post-clarification verification: the graph-provider ownership regression, saved-Orbit Composition opacity/undo regression, and saved-Orbit native Track view/Curve Editor regression all pass. Release desktop build, touched-file formatting and diff checks pass. Demo-file hashes are unchanged during this correction; Orbit was already updated, and no other demos were edited.

### Completion and user acceptance

On 2026-10-07 the user reviewed the corrected implementation and Orbit scope, said "looks good", and requested commit and completion in the dev-plan. Item 51a is complete and accepted. The recorded automated/native verification and OS-level dragging limitation remain unchanged.
