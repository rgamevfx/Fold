# Image inputs in Motion

> **Pre-rewrite reference — 8 October 2026.** These examples describe the old build. For new motion/MCP/agent work, use the [Motion System Rewrite Plan](../../../docs/Motion_System_Rewrite_Plan.md); this guide supplies no compatibility requirement. Requalify examples against the replacement public contract before publishing them as supported.

Use for image/video carousels, deforming textures, ASCII, or images driving geometry. Discover the `Image` catalog category first; these nodes require a phase-49 build. Supported imported profiles are Fold's MP4/H.264, OpenEXR (including sequences), and P6 PPM. Other image formats need a supported-source conversion before import. Composition and Motion video outputs work through the same image socket; project dependency validation rejects feedback cycles.

## Source and placement

Import through `batch` (`{"op":"import","paths":["/absolute/source.mp4"]}`), then read asset IDs from `state`. `place_asset` into Motion creates an editable Image Card with a connected Image Source and normal object transform. `place` does the same for a document output. The card's **Edit image source** opens its source collection; replacement, frame offsets and Hold/Loop/Transparent/Error are there. Card fit is Contain, Cover or Stretch.

For graph construction, `fold.motion.image` has an `image` output of type `Image`. Example settings for a verified source:

```json
{
  "sources": [{
    "source": {"Asset": "<imported asset ID>"},
    "info": {"width": 320, "height": 180, "rate": [30, 1], "frames": 90},
    "offset": {"numerator": 0, "denominator": 1}
  }],
  "outside": "Loop"
}
```

A document entry replaces `source` with `{"Output":{"document":"<document ID>","output":"video"}}` and uses that output's inspected metadata. Keep time rational in settings. The animated `time_offset` socket adds seconds to document time; each collection entry adds its own rational offset. EXR sequence local zero maps to its first numbered frame; gaps are errors.

## Choose the consuming construction

- **Image Card** (`image_card`): one image, uniform index and size. **Image Cards on Points** (`image_cards`): point distribution, per-point index and time offset, size and fit. Connect an Index field for cycling through the source collection. Output elements are instances, so normal per-copy position, rotation, scale and opacity responses apply. Repeated sources at the same time reuse media work; independent video times cost additional frames.
- **Textured Grid** (`image_grid`): size, rows/columns, index, and an offset Vector field evaluated at each vertex. Vertex index/position refer to the undeformed grid; shared corners have matching identities. UVs remain attached after deformation. `per_cell=false` spans one image over the grid; `true` repeats it in each cell. The source geometry is centered on its origin.
- **Image to ASCII** (`image_ascii`): image, source index, cell counts, size, dark-to-light character ramp, gain/bias and color amount. Use an embedded font. Creating it with the public `object` operation supplies the normal built-in font. Glyph selection samples each cell center; it does not average the whole cell. The rasterizer uses a shared atlas and GPU glyph selection, not individual per-frame text shaping.
- **Sample Image → Image Response**: `sample_image` outputs an `ImageField` from RGB, luminance or alpha with gain/bias. Connect it to `image_response.sample`, and connect geometry to `content`. Displacement, rotation, scale, color and opacity execute on the GPU. Images/grids use retained UVs; native paths/glyphs use their instance centers in document coordinates and shared raster atlases. Apply a response after instancing points. Combine response amounts in one node.

ImageField is deferred render data, distinct from CPU Scalar/Color fields. It cannot be connected to CPU Math, Format Text or Rust Wrangle; the graph rejects such connections rather than silently reading pixels back. Use the supported Image Response or ASCII consumers. RGB/luminance sampling unpremultiplies the scene-linear working image; alpha stays coverage. Gain/bias retain HDR values. Sampling is bilinear with clamped edges.

## Verification

Run [image_inputs.py](../scripts/image_inputs.py) only when adding three studies and linked fixtures to the chosen session is authorized. It uses public import, node/object authoring, source placement, rendering and save operations. It renders 0, 0.5 and 1.5 seconds and proves nested document input. Pass a new project path; inspect its PNGs and the saved state before handoff.

For performance, follow [the performance workflow](performance.md). Check both compilation and GPU-ready time; final PNG/readback timing is a different measure. Standard images use retained textures, meshes retain topology when unchanged, and compatible adjacent copies batch. Many unique videos, independent source times, large overlapping cards and dense grids remain bounded workloads, not unlimited real-time guarantees. GPU-deformed picking uses the authored edit cage; pixel-accurate deformation bounds require rendered observation.

## Prepare textures for instancing

Phase 50 adds **Image Bake** (`fold.motion.image_bake`), an Image → Image node. Connect Image Source → Image Bake → cards/grid/ASCII/response. Before baking it passes the original through. Select a resolution (longest edge), start and exclusive end frame; zero end uses the document end. Bake/Rebuild runs on a worker, with progress and cancellation. Clear restores the original input. Applying or clearing a bake uses normal project undo; clearing retains files needed by undo.

- **Auto** uses lossy BC7 for bounded SDR and Float16 for HDR/negative values. **Float (HDR)** retains those values with half-float precision and costs more storage. **Compact (SDR)** rejects out-of-range input unless **Clamp to SDR** is explicitly enabled; that option clips negative and above-white RGB.
- Every stored frame has linear-premultiplied mipmaps. Compatible adjacent cards share a texture array and instanced draw, retaining native source dimensions. Many copies at the same time share pixels; different times require different baked frames.
- Bake up to 600 document frames, 10,000 unique source requests and 2 GiB per bake (8 GiB aggregate). Request times must match the baked document-rate samples; downstream time offsets may require a wider range or another bake. Changed options/source references, missing files and uncached times report errors; rebuild or clear them.
- Cache files are disposable local `.foldtex` files under the media scratch root's `image-bakes-v1`, with manifest/hash validation. They are not embedded project assets or KTX2 interchange. Moving a project to another computer requires rebuilding. Originals and graph connections remain authoritative. Auto does not make a BC7 cache portable to GPUs without BC support; rebuild as Float there.

Public authoring uses the regular node patch for settings, e.g. `{"resolution":1024,"start":0,"end":31,"format":"Bc7","clamp_sdr":true}`. Then submit this operation in a revision-guarded `batch` and poll its job:

```json
{"op":"command","id":"fold.app.image-bake","arguments":{"document":"<motion document>","node":"<Image Bake node>"}}
```

Inspect with `{"document":"<motion document>","query":{"mode":"bake_status","node":"<Image Bake node>"}}`; the result includes progress, selected cache and changed-settings state. `cancel {job}` cancels the worker. Node settings `{"bake":null}` clear the reference. For group graphs, provide `group` in both command and status query.

[video_textures.py](../scripts/video_textures.py) creates ten distinct 1080p30 synthetic clips and six public-tool benchmark documents in an authorized disposable session. Use a new `--directory` and explicit `--session`; it saves `benchmark.fold` plus `cases.json`. Add `--bake --compact-sdr --resolution 2048 --frames 31` for the full-resolution BC7 trial, or `--resolution 512` for card-sized textures. `--compact-sdr` explicitly permits clamping/lossy compression. This is synthetic H.264 evidence, not a guarantee for arbitrary footage or codecs. The engine profiler command and measurements are in [the phase-50 report](../../../docs/phase-50-image-bake.md).

Verified workflow lessons: inspect returned image dimensions—public preview output is capped, so do not compare it with a native-size export as if they were equal. Native-only video decode requires the GPU service; use the GPU profiler for that measurement, or explicitly choose the software decoder for CPU/headless live rendering. Skip canonical paths already linked in `state` before an import-only batch; a fully duplicate import can produce `EmptyEdit`.
