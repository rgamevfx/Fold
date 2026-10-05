# Phase 25 — Object-based Motion authoring

User-approved scope: Motion is an artist-facing scene, with an integrated compact hierarchy/dope sheet/curve surface. Node graphs remain the implementation of editable procedural objects and modifiers. Compositing remains graph-first. Timeline remains the sequence editor.

## Contracts

- Inspector serves objects, modifiers and nodes. Network is a contextual graph surface; feature packages retain their own models and evaluation.
- MoGraph lists static and animated objects. Hierarchy selection and animation share stable identities. Summary drags move all descendant keys, including collapsed channels, with one undo entry and cancellation.
- Scene order, visibility, locking, active ranges, parenting, and modifier order are explicit authored state. Hidden objects remain referenceable; dependency cycles fail clearly.
- Built-in modifier groups expose ordinary controls and their editable graphs. Custom procedural objects can reference scene objects; reusable definitions distinguish local edits from shared edits.
- Motion documents are editable compositor sources through provider-owned document references, with explicit time mapping and source navigation. No feature-model imports across packages.
- Preserve old graphs and unknown extension data. Render immutable snapshots with the same semantics in preview, export and CLI.

## Work and acceptance

- [x] Scene model, validation, persistence and deterministic evaluation; legacy graph preservation.
- [x] Node-free object creation, scene organization, modifier controls and procedural object/reference authoring.
- [x] Shared Inspector and contextual Network; integrated MoGraph animation surface; Timeline retained.
- [x] Compact hierarchical animation rows, summary retiming, selection, curves and undo/cancel.
- [x] Compositor MoGraph source, source navigation and exact-time rendering.
- [x] Reproducible text/shape/radial-modifier/compositor demo, focused tests and checkpoint checks.

Native pointer dragging is unavailable on the development machine. Automated interaction/model checks cover gestures where possible; the user will verify the demo. Record results and outstanding limits here without treating a prepared demo as native acceptance.

## Implementation

Motion schema 3 adds scene objects with stable graph references, front-to-back order, group parenting, visibility, locks, exact active ranges and an optional output object. Schema 1/2 graphs remain readable. Converting a legacy construction retains its original graph as one procedural scene object; new scene objects use the same content/fill/transform construction without duplicating a second evaluator. Scene and object extension data round-trip unchanged.

Radial Array is an ordinary reusable group containing group inputs, radial points, instancing and a content output. Custom modifiers start with a pass-through graph; procedural objects start with transparent content. Object Reference reads evaluated object content in its parent space (without ancestor transforms) independently of that object's direct visibility/range. Group children retain their own visibility/ranges and local transforms. Explicit dependency traversal rejects reference and parenting cycles. Changing a group definition preserves compatible input wiring and animation while dropping incompatible channels. Group editing is shared; Make unique clones a definition while retaining its stable parameter IDs. The Interface menu names groups and exposed controls and edits defaults for new instances. Reusable groups are currently stored in their Motion document.

MoGraph combines object/transform/appearance/modifier rows with the shared animation editor. Canvas buttons and Add create objects without graph wiring. Object actions expose visibility, locking, output targeting, grouping, sibling reordering and reusable modifiers. Inspector provides object properties, modifier controls/order and Edit Graph. Hierarchy rows and summary spans/diamonds support dragging; numeric offset and exact rational time scaling provide an alternative. Filled object strips occupy most of each compact row, with selected/hover outlines and edge grips. Dragging a strip body moves its active range and unlocked descendant keys together; dragging an edge trims only the range. Lighter summary strips span the first/last keys of modifier and property rows. Curves retain the hierarchy; the timing area can collapse and its divider can resize.

Network is a shell-owned contextual surface dispatching to provider-owned graph editors. Compositor's former standalone editor window is represented there; Motion supplies its current document/group graph. Timeline remains independent. Inspector retains its saved panel identity but uses its generic name and registration API. Network source selection and parent navigation preserve existing document identities and explicit local clocks. Network currently follows the shared Inspector group/lock; an independently pinned Network scope is not introduced. Edit Graph requests reveal Network; selection continues through the shared workspace routing.

The compositor's existing document Read service already accepts Motion outputs. Its catalog and node title now use provider metadata (MoGraph), and both double-click navigation and Open Source map to the same exact local time. Project placement retains the existing reference service. No Motion model is imported by compositor, project or render.

## Demo

Generate a fresh project:

```sh
cargo run -p fold-app --example mograph_demo -- /tmp/fold-mograph-demo.fold
cargo run -p fold-app --features desktop --bin fold-desktop -- --project /tmp/fold-mograph-demo.fold
```

The generator refuses to overwrite an existing file. It creates **Radial Badge Design**, **Badge Composite**, and **Demo Sequence**, and evaluates the composite at frames 0, 48, 24, 96 and 0. Fonts and procedural content are self-contained.

Suggested review:

1. Open Radial Badge Design from Project. Text and Rectangle are hidden source objects; Radial badges is the visible procedural result. Choose that source in a viewer when editing its canvas.
2. Expand Radial badges and Radial Array. Inspect Count, Radius and Orientation. Radius has keys; drag its parent summary and undo, or use Retime selected keys in the animation menu.
3. Edit Graph on Radial Array to inspect its construction. Make unique before changing only that instance. Expose another numeric input through its Inspector parameter menu; name it in Interface.
4. Edit the procedural object's graph to see both Object References. Editing the hidden source text still changes the visible badge.
5. Open Badge Composite. Network shows a MoGraph source over a background. Open Source returns to editable Motion content with mapped time; the composite remains an editable document reference.
6. Open Demo Sequence to verify Timeline remains the sequence editor. Review normal/narrow panels and native drag/cancel/reorder behavior.

## Verification

- Full workspace tests with desktop features passed: `CARGO_INCREMENTAL=0 cargo test --workspace --features fold-app/desktop -j 2 --quiet -- --test-threads=1`. Existing opt-in native/GPU/OCIO tests remain ignored by that command.
- Workspace Clippy with desktop features and all targets completed successfully. Existing warnings remain; new collapsible-condition warnings were cleaned up afterward.
- Final desktop build passes. After cleanup, focused UI/Motion tests passed (89 tests, two existing UI tests ignored); the subsequent modifier reassignment fix passed all 29 Motion tests. Focused Clippy also completes with existing warnings.
- `/tmp/fold-mograph-demo.fold` was generated and evaluated at five times; reopening the saved project with `fold-cli render` produced a 640×360 sequence frame. The headless render was inspected; this does not establish native UI acceptance.
- Touched-file formatting and `git diff --check` pass. Package boundary checks pass. Focused coverage includes scene persistence/unknown extensions, hidden references/cycles, grouping/ranges/modifier removal, legacy conversion, exposed defaults, transactional undo/cancel, collapsed summary drags, exact key scaling, static range cancellation and Network clock routing.
- The first broad build exceeded available build-cache space. Rebuildable incremental caches and abandoned linker temporary files were removed, then the checkpoint was rerun successfully with two build jobs and incremental compilation disabled.

## Verification limits

The user will perform native gesture acceptance. Orca computer-use could not connect (`runtime_unavailable`); starting Orca returned `runtime_open_timeout`, and the retry remained unavailable. No native screenshot, click or drag acceptance is claimed. Automated ImGui tests exercise compact animation widths, summary dragging, static range cancellation and shell routing. Existing grouped-object viewport handles are suppressed rather than presenting incorrect parent-space controls; group/child transforms remain editable numerically in Inspector. Cross-document modifier libraries and tracked screen replacement are not introduced by this slice.

### Track strip refinement

- Replaced thin active-range and summary lines with compact filled strips and larger hit targets. Single-key summaries remain grabbable.
- Object-body drags preview range and descendant-key changes together; edge trims retain key times. Escape restores both. Locked channels remain unchanged.
- Validation: all 62 UI unit tests pass (two existing GPU tests ignored), including static and animated strip gestures, collapsed descendants, trim-only key preservation, cancellation and one commit on release. Frame-all includes scene ranges; left-edge trims take precedence over the hierarchy divider. Formatting, diff checks and package boundaries pass. Native visual/gesture review remains pending on the user machine.
