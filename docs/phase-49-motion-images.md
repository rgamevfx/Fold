# Phase 49: media in Motion

User-authorized scope: imported media and document images as Motion inputs; textured cards and deformable grids; collections and per-copy selection; image sampling for luminance-driven ASCII and procedural responses. Keep supported image work GPU-resident and share preview/export/headless semantics.

## Implementation

- Typed image references and exact source-time mapping; snapshot-owned resolution through platform contracts. Declare asset/document dependencies, validate cycles, preserve unknown data and transaction/undo behavior.
- Render-owned textured geometry and sampling operations, with bounded CPU reference and GPU implementations. UVs survive deformation; distinguish whole-image and per-cell mapping. Reuse source frames and immutable geometry.
- Motion nodes and artist controls for sources, collections, cards/grids, sampling and image-driven responses. Public catalog/authoring must expose the same construction.
- Sampling defines coordinate, filter, alpha and working-color semantics. No viewer-cache sampling or hidden full-image readback for CPU fields. Unsupported execution must report a useful error.

## Acceptance and verification

- Focused tests: dependency envelopes, invalid inputs, source-time mapping, save/load, placement/undo, grid UVs, source selection, sample/ASCII output, CPU/GPU parity, cancellation and resource bounds.
- Render representative textured-grid, carousel and ASCII scenes. Measure cold/warm completion, transfer counts and memory on the reference GPU; distinguish GPU-ready evaluation from output readback and continuous playback.
- Inspect source and effect controls at normal/narrow sizes; exercise artist source replacement and editable controls. Update Fold skill with verified public examples.
- During implementation run touched-file formatting and affected-crate checks; at completion review the diff and record limitations without claiming unmeasured frame rates.

## Progress

Implemented Image Source collections, Image Card, Image Cards on Points, Textured Grid, Sample Image, Image Response and Image to ASCII. Asset/document placement uses normal editable objects and transactions. Sources share the existing media pipeline and project dependency validation; schema 5 declares image dependencies while existing documents retain their previous schema.

Sources carry exact rational time, per-entry offsets and Hold/Loop/Transparent/Error range policies. Cards support Contain/Cover/Stretch; grids retain UVs through vertex deformation and support whole-image or repeated-cell mapping. Image Response supports displacement, scale, rotation, color and opacity on textured geometry and instanced native paths/glyphs. ASCII uses one font atlas with deferred glyph selection.

The renderer shares source evaluation, retains immutable RGB textures and meshes, batches adjacent compatible copies and pools its textured working surface. Native video remains in the existing GPU decode path. A bounded CPU oracle shares sampling and geometry semantics. GPU validation catches invalid transformed coordinates without reading image pixels back.

### Delivered examples and public workflow

[Editable four-document study](../demos/motion-image-inputs.md): warped texture, mixed image/video carousel, video ASCII and a nested Motion output. The [public-tool script](../skills/fold/scripts/image_inputs.py) creates/imports fixtures, authors all three constructions, places the nested output, renders three times and saves. The [Fold image guide](../skills/fold/references/image-inputs.md) is also exposed as `catalog guide="image-inputs"`.

### Verification

- Eight image integration tests: dependency/save round trips, invalid envelopes, exact time, retained UVs, 512-card batching, atlas reuse for native shapes, deferred socket rejection, placement/undo/redo and document cycles.
- Six existing scene integration tests pass. Prepared-root cache regression updated for schema 5. Shared-still test covers identity reuse, changed-file rejection and cancellation.
- Five textured renderer tests cover premultiplied sampling, graph rebasing, UV coverage, CPU/GPU agreement, luminance/remapping, deformation/rotation, reflections/overlap, atlas selection, cropped preview, retained uploads, invalid coordinates and memory-failure recovery.
- Native normal and 280-pixel narrow inspector captures reviewed for Image Source, Textured Grid, Sample Image and ASCII. Controls fit without clipped labels. Captures: [source](media/motion-images-source-narrow.png), [grid](media/motion-images-grid-normal.png), [sample](media/motion-images-sample-narrow.png), [ASCII](media/motion-images-ascii-narrow.png). Live mouse/keyboard interaction remains unverified; transaction behavior is covered in integration tests.
- Desktop-feature compilation and release desktop/CLI/profiler builds pass. The saved revision 6 reopens with all four documents and both linked assets; all three 0.5-second PNGs matched byte-for-byte. The public `image-inputs` guide is served by the final catalog. Headless `agent serve <file>` seeds a new session from the file (its session path is initially unset); it is not a desktop clean-state check.
- Final scene parity checks exposed a precision difference: the CPU oracle initially retained f32 blending while the GPU used RGBA16F. Native adapters now use full-precision RGBA32F blending when supported; the portable fallback remains RGBA16F. Full-scene 640×360 grid/carousel/ASCII comparisons at two times now pass with a maximum difference of one display code. Five hardware renderer tests pass on the final path. No additional dependency was needed.

### Reference GPU measurements

[Raw frames and summaries](motion-image-inputs-profile.json): GTX 1070, Vulkan/NVIDIA 580.173.02, release build, 1920×1080 output, 320×180 linked inputs, 31 sequential frames at 30 fps time increments. Video cases use the matching native CUDA helper. Warm statistics exclude the first frame.

| Scene | Cold ready | Warm ready mean / p95 | Warm including readback | Warm upload / frame | Peak tracked GPU memory |
| --- | ---: | ---: | ---: | ---: | ---: |
| Animated textured grid | 120.03 ms | 13.10 / 14.85 ms | 26.58 ms | 20.8 KiB (animated mesh) | 151.80 MiB |
| Six mixed-media cards | 384.32 ms | 30.35 / 33.29 ms | 43.59 ms | 768 B | 254.80 MiB |
| Video ASCII | 376.34 ms | 16.07 / 18.42 ms | 28.84 ms | 128 B | 185.06 MiB |

All measured frames report zero scene pixel readback; the profiler separately requests final output readback. Video scenes report a native video node, and the still texture is retained after the first upload. The carousel does not meet a 16.67 ms ready target; ASCII also exceeds it at p95. These are measured constraints, not a universal 60 fps claim. Independent videos, source-time offsets and overlap require workload-specific measurement.

### Explicit boundaries

- Input profiles are Fold's existing MP4/H.264, OpenEXR/EXR sequences and P6 PPM; PNG/JPEG import was not added. Composition/Motion outputs are valid inputs and feedback cycles are rejected.
- `ImageField` is deferred image data for supported GPU consumers, not a general CPU Scalar/Color field. CPU Math, Format Text and Rust Wrangle cannot consume it. Multiple image responses must be combined into one node; an already image-driven ASCII result cannot take a second Image Response.
- RGB/luminance sampling unpremultiplies scene-linear color, uses working-space luminance coefficients and bilinear clamped filtering. ASCII samples cell centers, rather than area-averaged luminance. Images use retained UVs; native shape instances sample their centers in document coordinates.
- Textured raster coverage uses pixel-center triangles on a shared 1/16-pixel position grid, with RGBA32F blending on capable native adapters, an RGBA16F fallback, and RGBA32F graph output. The fallback can differ by several display codes near dark saturated colors; full-precision hardware is used for strict scene-parity acceptance. No textured MSAA is introduced. GPU-deformed picking uses the authored edit cage; accurate deformed bounds need rendered observation.
- Source collections are limited to 64, grid axes to 128 cells, ASCII/copy batches to 16,384, meshes to 65,536 vertices, and executed vertices per draw to two million. CPU fallback also bounds pixel work. Distinct video times and overlapping sources have measurable cost.
- Continuous desktop playback, full workspace tests and workspace Clippy are not claimed. Performance probes separate display-ready completion from final readback; the Instagram post was inaccessible, so the study demonstrates mechanics rather than reproducing that artwork.
