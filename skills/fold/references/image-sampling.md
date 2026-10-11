# Image Texture and Sample Image

Use **Image Texture** to select an imported asset or another document's image output.
Use **Sample Image Color** or **Sample Image Channel** to read pixels at UV coordinates.
Find these operations in the running host's catalog before authoring Source.

- `fold.motion.image_texture`, version 1: `asset` or `output` mode.
- `fold.motion.sample_image`, version 1: `color` or `scalar` mode.
- Source identities must be literal. Animate `time` to change the source frame.
- UV starts at the top left. Pixel centers are `(x + 0.5) / width`, `(y + 0.5) / height`.
- Filter: `nearest` or `bilinear`. Wrap: `clamp`, `repeat`, `mirror`, or `transparent`.
- Outside Range: `hold`, `loop`, `transparent`, or `error`.
- Color output is straight-alpha, linear Rec.709. Filtering uses premultiplied alpha.
- `clamp_negative` defaults to true for color output. Set false to retain signed RGB.
  Positive HDR values are retained. Scalar output is not clamped.
- Scalar channels: `luminance`, `red`, `green`, `blue`, or `alpha`.

Connect the result to ordinary math, Color Animator, or Attribute Create. UV can
use the current point or instance index. Each evaluation shares decoded source
frames. Image sources are sampled at their native size, independent of Viewer size.

Run the [public CLI example](../../../demos/image-sampling.py) against a Fold socket:

```sh
python3 demos/image-sampling.py target/debug/fold-cli /path/to/fold.sock /tmp/image-cloner.fold /tmp/image-report.json
```

Use new output paths. The script creates a two-color source and two clones, checks
colors and stable IDs at shuffled times, and saves a project copy. If frame
observation is available, it also saves a PNG next to the report. PNG capture needs
Python Pillow. The script uses only discovered public authoring and observation tools.

## Current limits

Sampling runs on the CPU. GPU-only source graphs return an error. No Viewer texture
readback is used. The per-evaluation limit is 128 MiB and 64 source frames.
The shared importer determines supported formats; this change does not add PNG or
JPEG decoding. Video uses its source rate. EXR sequences use 24 fps. Document
outputs retain exact fractional time. ACES projects require the packaged OCIO runtime.

This is a bounded addition to rewrite phase M7. It does not close M7 or establish
physical artist acceptance. See [evidence](../../../docs/rewrite/evidence/image-sampling/result.md).
