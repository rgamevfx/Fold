# Motion craft

Use the [public workflow](public-workflow.md) for authoring and render review.

## Design and script

1. Establish dimensions, exact frame rate, duration, audience and assets. Inspect
   existing artwork. For references, record cuts in source frames and inspect
   consecutive frames near fast effects.
2. Plan hierarchy and reading time: one focus per shot, consistent alignment,
   margins and palette. Check text fit and glyphs in full-size pixels.
3. Use native objects for direct editing; one script for procedural content.
   Expose useful count, spacing, color, timing, text or image controls and retain
   their IDs when changing source.

Fold's script entry point is
`pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String>`.
Read controls through the supported `input` accessors (number, text, Boolean,
color). Use `input.time` as the clock and `input.seconds()` only at a numerical
animation boundary. Discover the current script edit schema.

Keep static SVG content static; move related content through group transforms
or opacity. Cache preparation only where the script contract supports it. Use
APIs supported by the pinned runtime, not assumed from upstream main.
Direct attention with one main movement, stagger related entrances and let text
settle. Match references before imposing defaults. A skewed 2D group is not 3D.

## Review

- Inspect structure and sample controls at exact times, including placement time mapping.
- Review opening, settled state, exit and transitions. Sample consecutive frames
  near fast changes; sparse poses can miss flashes.
- Use discovered contact-sheet/onion review for flow and trajectories, then
  full-size frames for detail. Fix unreadable text, blank frames and missing assets.
- Review real-time playback where available. After requested export, verify
  dimensions, frame rate, duration, alpha and audio as applicable.
- State which review was performed. Structural samples and engine tests do not
  establish visual quality; renderer captures do not establish native interaction.

## Upstream boundary

Adapted from the [fframes skill](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/SKILL.md)
and design/API/audio references studied on 2026-10-11 at runtime pin
`b644f22cf5be2333e7098caf7ec064c3d36aeb02`. Fold owns project storage, exact time,
Undo, Timeline, compositing, audio and export; fframes supplies Motion scripting
and drawing. Upstream standalone CLI/AudioMap workflows are not Fold APIs.
Discover available review tools and report gaps. Verify new examples through
Fold's public tools before documenting them as supported.
