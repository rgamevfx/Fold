# Reusable particle attraction

> **Pre-rewrite reference — 8 October 2026.** These examples describe the old build. For new motion/MCP/agent work, use the [Motion System Rewrite Plan](../../../docs/Motion_System_Rewrite_Plan.md); this guide supplies no compatibility requirement. Requalify examples against the replacement public contract before publishing them as supported.

Use this branch for particles gathering into editable text or a filled shape. Discover `catalog(type="fold.motion.document", query="particle")` first. The shipped **Attract Particles** scene action is exposed as the `attract_particles` edit operation. It creates an editable layer, a separate Scatter Points source, a Random Palette, and native Attraction animation on the layer. It hides the attractor's direct rendering while retaining its content and object reference.

## Construct and inspect

Create the target with `object` (text creation embeds its font), set its content and transform, then render it before adding particles. Text placement follows the actual glyph geometry; do not assume the transform is a baseline or vertical center.

Inside one revision-guarded batch, allocate `particles` with `ids` and submit:

```json
{"op":"edit","document":"MOTION_UUID","edits":[
  {"op":"attract_particles","id":{"$ref":"particles"},
   "attractor":"TARGET_OBJECT_UUID","name":"Attract Particles"}
]}
```

Poll the job and inspect its outcome. Inspect the particle node and its `settings.group`, then follow its `points`, `attractor`, and `color` input links. These are live editable sources, not baked data. Set instance overrides with `set`; inspect a group's complete port list before changing its definition.

| Artist edit | Public construction |
| --- | --- |
| Change “3” | Set the original text object's `text` input. Arbitrary filled paths/text are supported; holes respect their fill rule. |
| Replace target | Set the linked Object Reference node's `settings.object` to another observed scene object. Keep world/local settings explicit. |
| Initial distribution | Edit the linked Scatter Points group: `count`, `center`, `size`, `perimeter`, `jitter`, `seed`. Spread (`jitter`) adds bounded independent-axis position offsets. Replace the particle layer's `points` connection with another Points output for a different distribution. |
| Particle appearance | Set the particle layer's `size` or edit the underlying ellipse/Copy to Points graph for custom geometry. |
| Palette | Follow `color` to Random Palette. Its `seed` changes color assignments independently of motion. `settings.group` and `settings.ramp_node` identify the ordinary inner Color Ramp; edit its `stops` and `interpolation`. |
| Timing | Inspect the layer node's `fold.motion.animation`. Animate literal `attraction.0` and `turbulence.0` channels with exact key times. These appear in the MoGraph layer track. Preserve existing key IDs/tangents when retiming. |
| Motion | `frequency` is response in Hz; `damping` controls settling; `variation` varies response by stable point identity. `acceleration`, `turbulence`, and motion `seed` are independent of palette randomization. |

For an artist handoff, give the interface path as well as the API input: select **Attract Particles → Attractor → Edit**, then change the source's **Text** field. The Attractor picker replaces the object. The source is hidden from direct rendering, so its text can be hard to discover from the resulting particles alone. Inspect the current text before describing it: the original reference used “3,” but the saved demo may contain the artist's replacement.

The default stepped palette uses positions 0, .25, .5, .75 with a random coordinate in [0,1). Each stop owns the interval until the next stop; the last owns the remainder to 1. This gives equal probability to four colors. A final stop at exactly 1 has zero probability under this driver. Use unequal interval widths for weighted colors, or Linear/Smooth interpolation for gradients. Values are in the working color space; judge them through a render.

## Refine the motion from renders

When the first render feels too regular, identify which independent dimension needs variation. In this exercise, uniform arrival called for **Speed variation**; an overly straight initial border called for **Spread** on the point source. Color changes belong to the palette's seed. Keeping these controls separate lets the artist adjust one aspect without reshuffling the others.

Use the source object and initial Points connection as replaceable inputs. Expose the few controls that explain the result; keep the Solver and copy construction inside ordinary editable groups. If extending Fold is part of the task, first identify the missing reusable operation, then expose it through the same public authoring contract and undoable scene action. Return to public tools to build the artwork and verify that a different target and point source work. Track a missing capability instead of teaching the artwork workflow to depend on private implementation details.

Tune gathering and settling at several times, not just the final silhouette. Native Attraction and Turbulence keys give the artist timing handles; force values and response control the motion between those moments. Record exact key times from the current layer, rather than treating the reference timing as a required preset.

## Solver contract

- **Scatter Points** creates deterministic flat points, within a rectangle or on its perimeter, with stable identities independent of the position seed; optional Spread adds bounded offsets.
- **Particle Targets** assigns a random point inside transformed filled geometry to each input point. Place it before Solver's `initial` input. It stores `particle.target`, preserving velocity and other attributes.
- **Particle Step** reads `particle.target` and `velocity`, advances a damped spring with bounded implicit integration, and retains point identity and other attributes. In a Solver update, wire Previous State's `state` and `delta_time` to its `points` and `delta_time` inputs.
- The shipped Solver uses exact 1/25-second steps, holds between steps, and samples animated force controls each step. Changing authored initial points, target geometry, or seeds invalidates its replay state.
- Targets are sampled at Solver start. Animated target transforms after that instant are **not followed**. This tool has no particle collisions, continuous emission, death, or obstacle handling. Filled geometry is sampled; stroke width and rendered effects do not define attraction area.
- Work is bounded: at most 16,384 points, bounded shape sampling/replay, cancellation, and explicit errors for empty or excessively thin targets. A budget error is not an empty artistic result. Reduce point count or simplify the target when needed.

## Verify reuse

Render the initial spread, gathering, and settled state; compare cold and backward seeks. In a reversible probe, change target text, point count/distribution, palette seed, and an Attraction key time. Check that palette edits retain motion and that force keys alter gathering. Restore only the probe's changes using freshly inspected state. Save, reopen in the release desktop, rediscover the session, inspect retained sources/tracks, and render again.

The reference for this exercise was acquired with yt-dlp after Instagram page access failed. When a supplied video page cannot be inspected and a download is available, use the downloaded clip as visual evidence; verify its source and inspect spread, gathering, and settled frames before choosing primitives. The delivered study is a procedural interpretation, with an editable font and particle trajectories rather than a pixel-identical reconstruction.

The reusable artifact is [the saved project](../../../demos/particle-attraction.fold), with [an artist guide](../../../demos/particle-attraction.md). Its source text and keys remain editable; those example values are not requirements for new setups. Preview timings and performance measurements refer to the revision tested. A successful still render or short render profile does not establish sustained desktop playback performance.
