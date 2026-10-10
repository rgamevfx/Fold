# Compositor Roto — 10 October 2026

Implemented under the user-directed compositor work. This does not close a Motion rewrite phase or constitute physical artist acceptance.

## Scope

Roto v1 has an optional image input and named-channel output (alpha by default). Compositor owns the document model; Render receives immutable native vector drawings. Root/group/shape/point IDs and extension metadata survive document round trips. Scalar animation uses existing native tracks and exact rational time. Shutter samples evaluate shape deformation and group transforms, not just a post-render transform.

The inspector follows `crates/compositor/prototypes/nodes-reference.html`: channels, premultiply, a layer/shape tree, opacity, signed feather, falloff, invert, combine operations, transform, lifetime and motion blur. Clip to format is visibly fixed because the compositor has no unbounded data window. Invalid requests to disable it are rejected.

Selecting one Roto node activates the viewport toolbar. Tools: Select, Rectangle, Ellipse, Bézier and B-spline. Bézier clicks create corners; drags create linked tangents. Enter or clicking the first point closes a path. Escape cancels. Select supports point/marquee selection, curve insertion, tangent linking/breaking, feather handles and deleting points. Selected-point bounds provide translation, scale, rotation and Ctrl-corner-pin. Shift constrains creation/scale/rotation. Numeric point controls remain available in the inspector. Each drag previews a transaction and commits one undo entry; cancellation restores the prior document.

## Rendering and maintenance

Feather is a signed coverage ribbon around a hard contour, tessellated once into the shared vector geometry. CPU and WGSL integrate linear coverage over clipped pixel polygons. Signed contributions support inward and outward feather. Falloff uses bounded adaptive subdivision. No bitmap-mask upload, separate rasterizer or Motion graph is stored in Roto.

Geometry uses the existing bounded cache, instance hierarchy, resource accounting and cancellation checks. Soft contours require a single closed path with corresponding segments. The shared triangle ABI grows from 24 to 48 bytes for coverage weights/alignment; existing consumers explicitly use `feather: None`. The existing GPU reuse test retains its original geometry allowance plus the exact added bytes per unique triangle, not per instance.

## Verification

All Cargo invocations use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`, `--offline -j 2` after the user-authorized Cargo clean freed about 196 GiB.

- `cargo check -p fold-motion -p fold-compositor --features fold-motion/ui,fold-compositor/ui`: passed.
- `cargo check -p fold-compositor`: passed without UI/GPU features.
- `cargo test -p fold-compositor --features ui --test roto -- --include-ignored --nocapture --test-threads=1`: 3 passed, including GTX 1070 parity/reuse.
- `cargo test -p fold-compositor --features ui --lib roto_ -- --include-ignored --test-threads=1`: point/key identity, corner-pin geometry, actual ImGui drag/cancel/commit/undo and native normal/narrow captures.
- `cargo test -p fold-render --features gpu --lib vector`: 18 passed; native tests separately qualified below.
- `cargo test -p fold-render --features gpu --lib gpu::vector::tests -- --ignored --test-threads=1`: all 8 native GPU regressions passed.
- `cargo check -p fold-app --features desktop`: passed.
- Touched-file formatting and `git diff --check`: passed.

The drag test exposed a release-frame movement loss; the handler now applies that movement before committing. Native captures exposed an unsupported tree glyph; the inspector now uses the shared tree widget. Normal and narrow panels have no horizontal overflow. A final clipping experiment hid the overlay; it was removed. The native capture test now also requires visible shape-control pixels, and the final captures were reviewed again. Captures use the real Dear ImGui/WGPU renderer offscreen, not browser mockups. They show the inspector and overlay controls; they do not establish a physical artist session over footage.

### Native captures

[Inspector](inspector-normal.png), [narrow inspector](inspector-narrow.png), [viewport tools](viewport.png), [narrow viewport](viewport-narrow.png). Logs are retained beside this record.

## GPU evidence

Vulkan, NVIDIA GeForce GTX 1070, driver 580.173.02. Fixture: outward-feathered ellipse plus inward-feathered rectangle, two native vector instances. At 128², CPU/GPU maximum channel error was 0.000002027. Evaluation had zero CPU adapter nodes and zero image readback before the explicit comparison. Warm geometry and instance upload bytes were both zero; 720 unique/executed triangles were retained.

Debug-build timings are single samples, not production benchmarks. Compile is outside these timings. Warm-ready includes evaluation, submission, completion and 1 ms polling; readback is measured separately. Vector preparation was 0.50–0.63 ms.

| Output | Warm ready | Warm including explicit readback |
| --- | ---: | ---: |
| 512 × 512 | 15.36 ms | 99.97 ms |
| 1080 × 1080 | 52.68 ms | 488.94 ms |
| 2160 × 2160 | 80.61 ms | 1439.69 ms |

Preview retains GPU output and does not use the explicit pixel readback in these tests. These figures do not establish real-time performance for large animated roto stacks.

## Qualification limits

- Output remains bounded by composition format.
- Feather tangents follow the main Bézier tangents; points have independent feather offsets. Crossed/self-intersecting feather contours and dense production shapes are not artist-qualified.
- Selection transforms operate on points of one shape. Multi-shape transform selection and independently keyed layer order are not implemented.
- No public-agent live authoring/save/reopen session or physical desktop artist acceptance was performed. Serialized model round-trip, shared compilation and native offscreen interaction are tested.
- Full workspace suites and release benchmarks were not run for this focused change. Existing unused `format`/`parameters` warnings in `render/gpu/yuv.rs` remain unrelated.
