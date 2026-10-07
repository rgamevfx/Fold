# Phase 51 — Composition authoring

**One Composition type owns a shared layer stack. Each layer contains a MoGraph graph or a Compositing graph; footage and nested compositions also fit in that stack. Graph types are layer content, never alternative composition types.**

Status: complete. On 2026-10-07 the user accepted the corrected UI ("It looks good") and requested phase completion and commit. Earlier editing-model and saved-workspace failures are corrected; verification is recorded below.

## Direction and contracts

- Composition owns layer hierarchy, visibility, lock, affine transforms, opacity, ranges, source clocks, keys and curves. Independent Motion and Compositor packages own their graphs and evaluation; Timeline owns sequences.
- Retire standalone Animation and MoGraph windows. Network edits the selected layer graph. Graph-internal parameter animation remains available through Network's Animation control; common layer animation remains in Composition.
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
- [x] Shared layers, dope sheet and curve controls; remove obsolete Animation registrations, dispatch and duplicate editor state.
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
- [x] Retain native Tools, modifier-group navigation and scoped graph animation. Reset stale group scope when selecting another layer.
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
