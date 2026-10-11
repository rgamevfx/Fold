# Common still-image import — 11 October 2026

User-directed shared-media addition. No Motion phase gate is closed or advanced.

## Implemented

- JPEG (`jpg`, `jpeg`), PNG, WebP, TIFF (`tif`, `tiff`), GIF and BMP import.
  File signatures select the decoder; extension matching is case insensitive.
- Typed source metadata records oriented dimensions, sample depth, alpha,
  TIFF alpha association and embedded ICC profile identity. EXIF orientation is
  applied. GIF, animated WebP/APNG and multipage TIFF use the first image only.
- The existing immutable still cache serves inspection and rendering. Fingerprint,
  cancellation, pixel/source-size limits and aggregate memory accounting remain.
  Cache accounting includes float storage; GPU retention uses stable still identities.
- Shared placement/source resolution covers Timeline, Composition (editable
  Compositor Read graph), Compositor and Motion image selection/sampling.
  Existing import transactions, undo and project persistence carry the new profile.
- Alpha and 16-bit PNG/TIFF samples survive decoding. Associated TIFF alpha is
  converted to straight samples before sRGB transfer conversion. CPU and GPU use
  the same sRGB interpretation, including the existing ACES input transform.
- Project import and Compositor Read file filters include the new formats.
  No panel layout or controls were added.

## Limits

- This adds still-image import, not animated image playback or image export.
- The default/available still interpretation is sRGB. Embedded ICC profiles are
  identified but not automatically converted; PNG gamma/chromaticity metadata is
  not a separate color-management path. Import provenance states the policy.
- Decoder format subsets apply (for example, not every TIFF photometric layout
  is supported). Unsupported encodings return decoder errors.
- The initial small-image size limits were superseded by the
  [large-image follow-up](../large-images/result.md). The shared geometry guard
  now supports 8192 × 8192; admission follows configured memory and GPU capability.
- Native file-dialog interaction, physical artist acceptance, production file
  collections, and all-platform packaging are not qualified here.

## Automated verification

- `cargo test -p fold-media --test raster`: 5 passed. All six formats and suffix
  aliases, alpha, 16-bit precision, TIFF alpha association, JPEG EXIF rotation,
  first GIF frame, thumbnails, cancellation, malformed and changed files.
- `cargo test -p fold-media --lib --test raster --test thumbnails`: final run
  passed all 22 tests, including PPM identity/invalidation, video signal references
  and image/audio browser previews.
- `FOLD_TEST_COLOR_ROOT="$PWD/target" cargo test -p fold-render --features gpu
  --test gpu_color raster_alpha_srgb_cpu_gpu_and_retained_source_agree -- --ignored
  --exact`: passed on the native Vulkan host. CPU/GPU premultiplied pixels agree
  within 0.0001 in linear sRGB and ACES, including repeated cached renders.
- `cargo test -p fold-app --test project_placement
  common_image_import_placement_render_and_reopen`: passed. Independently encoded
  fixtures for all six formats import, place in Timeline/Composition, render
  through editable Compositor Read graphs at native document resolution, and
  survive undo/redo and save/reopen.
- `cargo check -p fold-app --features desktop`: passed on the final code.
- Touched Rust files pass formatting checks; `git diff --check` passes.

The GPU-only build reports existing dead-code warnings for `gpu/yuv.rs` fields
and methods used by native-video configurations. No workspace-wide checkpoint,
full suite or Clippy gate is claimed by this bounded media change.
