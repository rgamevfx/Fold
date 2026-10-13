# Motion craft with fframes

Use this workflow when making or changing Motion artwork in Fold. Use the
[public workflow](public-workflow.md) for connection, edits and Apply.

## Brief and design

1. Establish the output size, exact frame rate, duration, audience and assets.
   Inspect existing artwork before replacing it. For a supplied reference,
   record cuts in source frames and examine consecutive frames near fast effects.
2. Plan the visual hierarchy and reading time before scripting. Give each shot
   a clear focus. Use consistent alignment, generous margins and a small palette.
   Measure text where supported; inspect full-size pixels for box fit and glyphs.
3. Use native objects for shapes the artist needs to edit directly. Use one
   script object for procedural content. Expose useful controls such as count,
   spacing, color, timing and text. Preserve their IDs when updating the script.

## Script and motion

Fold supplies this script entry point:
`pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String>`.
Read exposed values with `input.number`, `input.text`, `input.boolean` and
`input.color`. Use `input.time` as the authoritative clock and `input.seconds()`
at a numerical animation boundary. Discover the script edit schema from Fold.

Keep static SVG content static. Move related content through a group transform
or opacity. Cache expensive preparation outside frame evaluation when the
script contract supports it. Choose upstream animation APIs supported by the
pinned runtime; the upstream skill may describe newer APIs.

Direct attention with one main movement. Offset related entrances and give text
time to settle and be read. Use eased motion or springs when they suit the brief.
Match supplied references before applying defaults. Camera motion and independent
object motion need separate models; a skewed 2D group does not supply 3D depth.

## Review loop

1. Inspect document structure and sample controls at exact times. Check duration,
   placement time mapping and stable IDs before reviewing pixels.
2. Request available render observations through discovered public tools. Review
   the start, settled state, exit and transition boundaries. Inspect consecutive
   frames around fast changes; sparse sampling can miss flashes.
3. Where supported, use a contact sheet to check flow, full-size frames to check
   details and onion frames to check paths and spacing. Report missing capabilities
   instead of issuing upstream CLI commands to Fold.
4. Fix unreadable text, unintended blank frames, missing assets and broken motion.
   A structural inspection alone does not qualify shaders or rendered output.
5. Review real-time playback in Fold when available. Export when requested, then
   verify output dimensions, frame rate, duration, alpha and audio as applicable.

For artwork, use render observations and playback as evidence. Engine changes
follow repository validation rules. State exactly which review was performed;
parameter samples and passing engine tests do not establish visual quality.

## Adoption boundary

Fold owns project storage, exact time, undo, controls, Timeline, Compositing and
export. fframes provides Motion scripting and drawing. Audio belongs in Fold's
audio workflow; upstream AudioMap and standalone project creation are not Fold
authoring instructions. Use the qualified GPU path.

The upstream skill supplies `timeline`, `inspect`, `strip`, `frame`, `onion`,
`preview` and audio analysis workflows. Their full equivalents are not yet
qualified through Fold's public tools. These are implementation requirements,
not commands this guide claims are available.

## Source and maintenance

Studied the [upstream skill](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/SKILL.md)
on 2026-10-11, and its design, API and audio references in the pinned checkout
`b644f22cf5be2333e7098caf7ec064c3d36aeb02`. This guide is an original Fold
adaptation. Compare future upstream changes against the runtime pin and verify
new examples through Fold's public tools before documenting them as supported.


For exact multi-pose review, use the discovered `observe_frame.review` field.
The [public workflow](public-workflow.md#frame-review) has a verified two-pose
contact-sheet/onion example and its limits. Choose useful poses, then inspect the
returned images. A passed pixel check does not establish motion-design quality.
