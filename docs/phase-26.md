# Phase 26 — Layer relationships and attributed path motion

Authorized scope: ordered 2D layers with visibility/lock controls, transform groups, preserve-placement reparenting, explicit follow constraints, local/referenced masks and group isolation. Deliver an editable shape/text path-motion project with stagger, secondary motion and path-color transfer.

## Contracts

- Topmost siblings render in front; group children occupy one contiguous stack position. Cross-hierarchy constraints do not alter stacking.
- Reparenting preserves the evaluated world transform at the current time by default, through an explicit offset. It does not bake the entire animated trajectory.
- Hidden objects remain valid sources. Locks protect authored edits, not evaluation. Dependencies reject cycles.
- Masks apply after object modifiers, in explicit local/reference coordinates. Isolation composites children before opacity and masks; ordinary group opacity distributes to children.
- Path attributes are typed, named samples over normalized arc length. Stagger is a per-copy time offset, not stateful simulation. Results must agree in arbitrary evaluation order.
- Scene/project edits retain transactional undo and extension data. Feature models remain within Motion.

## Work

- [x] Scene transforms, relationships, masks and isolated rendering.
- [x] Layer controls and Inspector authoring workflows.
- [x] Path sampling, named attributes, stagger and transfer nodes/presets.
- [x] Editable demo, regression checks and render evidence.

Native drag/visual acceptance remains user-owned on this machine. Record actual verification and limitations below.

## Implemented workflow

Motion schema 4 adds explicit object offsets, opacity/isolation, constraint stacks, mask stacks and scene-owned animation channels. Schemas 1–3 remain readable; unknown object, relationship, path-setting and attribute-sample extensions are preserved. Sibling order remains front-to-back and groups occupy contiguous positions. Reparenting uses the old/new parent world transforms at the edit time to preserve placement, including objects with constraints; singular destination transforms are rejected.

MoGraph rows have separate eye/lock controls and type badges. Inspector's Group picker and Object → Move into group preserve placement; Inspector also offers Keep local coordinates. Scene-owned opacity, mask opacity/feather and constraint influence/path progress use the shared keying controls and appear in the integrated animation hierarchy. Constraints and masks are ordered, selectable rows beneath their object. Normal grouped-object viewport handles now use the parent world transform and authored offset. Hidden paths remain editable through their handles.

Constraints include Follow Transform, Copy Position/Rotation/Scale, Aim and Follow Path. Follow Transform captures an offset by default; constraint targets do not change stacking. Partial full-transform influence interpolates rotation by the shortest arc, signed scale and shear rather than collapsing the matrix at half a turn. Constraint dependencies and content/mask dependencies are validated separately so ordinary group parenting is legal and cycles are rejected.

Masks can reference another object in scene-world coordinates or use a hidden local path created by New local mask. Local paths are editable with existing path point handles and Inspector segment controls. Ordered Intersect/Add/Subtract operations support inversion, opacity and feathering. Masks apply after the owner's modifier stack and object transform, before the parent combines its children. They isolate their owner. Unisolated group opacity distributes to children; isolated opacity applies once after their composition. Lowering uses existing Vector, Over, Mix, Merge, Gaussian and Mask operations; preview/export/headless share the same plan.

Path attributes are named, typed samples at normalized arc-length positions. Inspector supports Color, Number and Vector attributes, renaming, sample insertion/removal and color-managed color picking. Numeric attributes interpolate; discrete values use held samples. Path sampling supports a single continuous line/quadratic/cubic/closed contour, with bounded subdivision (64 steps per quadratic, 96 per cubic). It rejects multiple paths/contours and zero-length paths rather than choosing one silently.

Object → Add Path Motion creates an editable reusable modifier graph containing an owner-relative Object Reference, Path Motion and Transfer Attribute to Color. It exposes count, progress, speed, per-copy stagger in seconds, normal-direction wave amplitude/frequency, scale pulse, orientation, looping and text-only transfer. Path Motion samples named attributes onto each generated instance along with index, local time and path position. Color transfer reads the named color attribute and updates glyph fills while retaining the shape appearance. Owner-relative references let the same modifier definition work on objects inside transformed groups.

## Demo and review

The generator creates **Chroma Parade — MoGraph**, **Chroma Parade — Composite** and **Chroma Parade — Sequence**. The source scene contains a figure-eight Spectrum path with five color samples, a shape/text Badge group with ten staggered instances, animated wave amplitude, an animated Follow Path marker, and a hidden soft mask. Titles and the path guide are ordinary editable objects. Colors transfer only to the text.

```sh
cargo run -p fold-app --example path_parade -- /tmp/fold-chroma-parade.fold
# Optional second argument: a new directory for 120 preview PPM frames at 12 fps.
target/debug/fold-desktop --project /tmp/fold-chroma-parade.fold
```

The generator refuses to overwrite its project or preview directory. A preview directory can be encoded with:

```sh
ffmpeg -framerate 12 -i /tmp/fold-chroma-parade-frames/%04d.ppm -c:v libx264 -pix_fmt yuv420p /tmp/fold-chroma-parade.mp4
```

Review the scene by expanding Traveling badges, selecting Path Motion and adjusting Count/Stagger/Wave/Pulse. Select Spectrum path and edit Path attributes or path points; its eye can remain off. Select a badge child to edit its text or rounded shape. Inspect the group's mask and isolation, then the marker's Follow Path progress keys. Reparent objects through Group and undo. Open the Composite in Network and the Sequence in Timeline to review the same editable output.

## Limits and verification to record

- Stagger offsets the path-motion clock per copy; it does not time-shift arbitrary upstream source animation. Attributes are sampled along one continuous path, not a general mesh proximity-transfer system.
- Reparenting preserves placement at the current time, not an entire animated trajectory. Constrained objects use Inspector for base edits; direct constrained-transform gizmos remain unavailable. Ordinary grouped and hidden-path handles are supported.
- Object/member stack order is changed by row dragging. Changing group membership uses the explicit Group picker/menu, not an ambiguous drop gesture. Group and constraint links are shown in Inspector, not as permanent wide columns.
- Native screenshots, DPI and physical-pointer acceptance remain unverified on this machine. Automated ImGui and image/evaluation checks do not substitute for that review.


### Render evidence

The 960×540 demo renders on both CPU and GPU. All 120 preview frames compare byte-for-byte identically between CPU and GPU after output conversion. The rendered frame was inspected to correct title spacing and center the text on its cards. This is artwork/output verification, not native UI acceptance or a real-time playback benchmark.


The review artifacts are `/tmp/fold-chroma-parade-final.fold` (editable project), `/tmp/fold-chroma-parade.mp4` (10 seconds, 960×540, 12 fps), and `/tmp/fold-chroma-parade.png` (inspected frame). The source animation is authored at 24 fps; the preview samples every other frame.

The integration checkpoint exposed an older add-content command that appended only to the root graph of a scene document. It now creates a scene object; the project-placement regression requires nonempty native drawings rather than merely the existence of a vector operation.

### Final regression checkpoint

The desktop-feature workspace suite ran all targets with `--no-fail-fast`. All targets except two Motion UI group-extraction tests passed (opt-in tests remain ignored). Those failures exposed extraction replacing the identity referenced by scene objects. Extraction now preserves the external node identity and its animation, and rejects structural scene-child nodes. A new rendered regression verifies animated scene content and object references before/after extraction. The complete affected Motion suite then passed: 6 unit, 17 evaluation, 10 relationship and 6 scene tests (39 total). The full workspace was not repeated after that focused correction.

Shared UI checks passed (63 tests; 2 ignored), including separate eye/lock actions at normal and narrow widths. Package boundaries, touched-file formatting and `git diff --check` pass. Clippy completes with existing repository warnings. Native interface/gesture acceptance remains pending user review.

### User review follow-up

The user reviewed the interface and reported that it looks good, but the demo renders too slowly. Playback performance is not accepted. The supplied desktop executable was a debug build; desktop defaults to the shared GPU renderer unless `FOLD_RENDER_BACKEND=cpu` is set. Vector coverage and paint blending execute on the GPU, while motion evaluation, tessellation, transformed geometry preparation and tile binning execute on the CPU. Reusable geometry is cached, but changing drawings invalidates complete uploaded packets. Release-build profiling and any optimization remain follow-up work; no performance fix is claimed in this phase. Native drag/DPI coverage remains unverified.
