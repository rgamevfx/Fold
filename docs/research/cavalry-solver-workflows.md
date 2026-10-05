# Cavalry and Houdini references for reusable motion solvers

Inspected: 2026-10-05. Status: primary-source research and design recommendations;
not a completed recreation or an implementation specification.

The requested [reference clip](https://www.youtube.com/shorts/x6O9nzOF-S8),
“16th Form Motion Design | Typography Animation in Cavalry” by tifukelison,
was inspected from a downloaded 360×640, 25 fps, 10.72-second video: frames
sampled throughout at 2 fps, plus denser opening and mid-shot samples.
Nothing below establishes which nodes its author actually used. The source
project and exact fonts were not available. Cavalry's former documentation host redirects
to `cavalry.studio/docs`; these links use the current destination.

## Cavalry's composable workflow

| Capability | Verified behavior | Implication for Fold |
| --- | --- | --- |
| Distribution and repetition | Duplicator distributes input shapes and exposes per-copy position, rotation, scale, visibility, opacity, shape selection, and animation time offset. Stagger can drive that offset. [Duplicator](https://cavalry.studio/docs/nodes/shapes/duplicator/) | Keep source geometry, placement, per-element properties, and time mapping independently connectable. |
| Per-element context | Duplicator supplies indices upstream; a connected Random behavior uses those indices when producing per-copy values. Nested duplicators have context controls. [Context](https://cavalry.studio/docs/getting-started/key-concepts/context/) | Make element context inspectable, but use persistent element identity for history rather than relying on current array position. |
| Spatial influence | Falloff samples circle, rectangle, linear, sweep, or custom-shape influence; its curve shapes the response. [Falloff](https://cavalry.studio/docs/nodes/utilities/falloff/) | Share fields between direct deformation, selection, and simulation inputs. |
| Combining influence | Behaviors support ordered falloff lists with operations including addition, subtraction, multiplication, minimum, and maximum. [Common behavior attributes](https://cavalry.studio/docs/nodes/behaviours/common-attributes-behaviours/) | Reusable field composition is preferable to a different influence implementation inside each effect. |
| Geometry feedback | Mesh Solver connects deformers for iterative effects, explicitly including drawing machines and differential growth. It exposes start/reset frame, pre-roll, hierarchy level, and retention of previous meshes for trails; retention is restricted to Internal Feedback mode. [Mesh Solver](https://cavalry.studio/docs/nodes/behaviours/mesh-solver/) | A solver should execute user-composed geometry operations, with explicit temporal feedback. The inspected page does not fully specify feedback modes or cache invalidation. |
| Numeric memory | Value Solver retains prior-frame values for accumulation, extrema, or fading, exposes start time, and supports a file cache and cache offset. Its documented example drives duplicated polygons using a falloff. [Value Solver](https://cavalry.studio/docs/nodes/behaviours/value-solver/) | Numeric state is useful independently of physics: persistent activation, counters, decay, and remembered influence. |
| Trace geometry | Trails creates lines or Bézier curves from moving shapes, with a start frame, optional length limit, per-trail context, and particle-color inheritance. The page marks it experimental. [Trails](https://cavalry.studio/docs/nodes/shapes/trails/) | Keep recording motion and drawing recorded paths separate from the operation that moves the source. |
| Across-element accumulation | Accumulator adds a value per evaluated mesh to construct stacks/layouts; it is not documented as previous-frame memory. [Accumulator](https://cavalry.studio/docs/nodes/utilities/accumulator/) | Distinguish a spatial fold over elements from temporal feedback in names and contracts. |

### Where Forge Dynamics fits

Forge is a Box2D-based 2D physics system with bodies, fields, constraints, and
collision events. Its body choices include dynamic, still, kinematic, and
kinematic-hybrid; settings include collision geometry, friction, bounce,
density, damping, and initial velocities. Its documentation specifies a step
frequency, simulation caching to `.sdcache`, and cache offset for retiming;
file caching is required for its motion blur. The same page documents limits:
kinematic motion supports keyframes but not behaviors, and geometry changes
during simulation are unsupported. These are current documented restrictions,
not assumptions Fold should inherit. [Forge Dynamics](https://cavalry.studio/docs/nodes/shapes/forge-dynamics/forge-dynamics-shape/)

**Recommendation:** consider Forge when the observed shot requires physical
collision or constraint behavior. For custom iterative growth or accumulation,
Cavalry's Mesh Solver and Value Solver are closer references. A general feedback
container and a physical collision system solve different design problems.

## Houdini's relevant contract

Houdini's Solver SOP runs a user-authored SOP network repeatedly, supplying the
previous frame's geometry through `Prev_Frame`. Original/current input geometry
and additional external inputs remain separately available inside the network.
This supports feedback and simulation-like construction from ordinary geometry
operators. It exposes initial state, start frame, substeps, reset, a memory
cache limit, and optional disk caching. Its documentation also warns that
viewing/playing the inner network can produce un-feedback cached results.
[Solver SOP](https://www.sidefx.com/docs/houdini/nodes/sop/solver.html)

The underlying SOP Solver DOP evolves geometry at each simulation step and
exposes simulation time, timestep, and object identity. It can solve geometry
or field data. This reinforces that time-step context and owned state belong
to the solver boundary, while the update algorithm can remain an ordinary
node network. [SOP Solver DOP](https://www.sidefx.com/docs/houdini/nodes/dop/sopsolver.html)

## Recommended Fold shape

These are design recommendations inferred from the sources, not descriptions
of existing Fold implementation.

The [product architecture](../Fold_Application_Product_Architecture.md), sections
11 and 13, distinguishes random-access animation from explicit simulation
regions with initial conditions, fixed steps, checkpoints, invalidation, and
bake policy. It requires motion evaluation to work without sequential playback.
[dev-plan.md](../../dev-plan.md) currently defers simulations generally; this
project is a concrete reason to scope an initial simulation region, not a reason
to silently discard the existing requirements.

1. **Expose a Solver containing an editable update graph.** Its conceptual
   contract is `next_state = update(previous_state, inputs_at_step, step_context)`.
   Initial state and live external input are distinct. Start with a concrete
   useful state type, such as identified points or geometry with attributes;
   do not hardcode a particular growth, packing, or bounce effect into Solver.
2. **Keep solver boundaries explicit.** The inner update graph is acyclic;
   only the designated previous-state input crosses steps. Provide time,
   delta-time, step index, seed, and element identity to ordinary operators.
   A spatial iteration loop and a temporal solve should have distinct semantics.
3. **Make seeking deterministic.** Resolve a request from a matching checkpoint
   or the initial state and replay fixed rational steps. Cache state by relevant
   input/graph identity, algorithm version, timing, seed, and instance context.
   Treat caches as derived data, not mutable document state. Define behavior
   before the start time and between step boundaries, including motion blur.
4. **Reuse the same solve in all consumers.** Separate mutable worker execution
   from immutable published results; preview, export, and headless evaluation
   share semantics. Cancellation, bounded history, checkpoint eviction, and
   independent viewers must not change results or block the UI.
5. **Build the example from replaceable operations.** Depending on visual
   inspection: distribute points, sample/mix fields, find neighbors, alter
   positions or attributes, resample paths, record trajectories, instance
   shapes, and style geometry. Add only operations the real shot establishes
   as necessary; solver extensibility comes from graph composition.
6. **Prove reuse with a second behavior.** The same solver should support at
   least two substantially different update graphs, such as accumulated
   displacement and persistent/fading influence, without changes to its
   evaluator. Compare sequential evaluation, late-frame cold seek, reverse
   seeks, parameter-edit invalidation, and isolated document instances.

## Applying the research to the reference shot

### Observed appearance

- White portrait layout, a small “16th FORM” mark, and dark stacked text reading
  “Become / Obvious / First / Choice.”
- During roughly the first 1.5 seconds, the letters have blocky contours and
  staggered offsets before settling into smooth, aligned typography.
- Orange forms cross the composition, including over the text. Their silhouettes
  alternate between small squares, horizontal/vertical bars, and diagonal shapes
  with clipped corners. Opacity varies, with short pale extensions/echoes.
- Mid-shot samples show translating shapes and changing elongation/direction.
  No clear contact response establishes that rigid-body collisions are needed.
  Seamless looping, 3D geometry, and the creator's actual solver use are unverified.

### Reconstruction hypotheses to compare

The orange silhouettes are consistent with short histories of translating square
profiles, or swept shapes between sampled positions. This is an inference, not
proof of the original technique. A conventional thick stroked path may give
different end caps and corner silhouettes; compare isolated shapes before scaling
up the composition. Explicit geometry histories and blur are separate treatments.

In Cavalry I would begin with a square source and Duplicator, independent
per-copy motion and phase, then compare Mesh Solver retained geometry against
Trails and a swept-profile construction. Drive palette/opacity independently.
If the motion accumulates velocity, use Value2 Solver or geometry feedback;
if its trajectories are pure functions of time, sample those trajectories
directly. Motion history does not by itself require a physical simulation.

For the opening, start from live Text, animate per-character offsets with
Behaviours and Stagger, and test Voxelize with decreasing grid size to obtain
the blocky-to-smooth transition. Voxelize converts arbitrary shapes into a
controllable grid; this makes it a useful general operation, not a text-only
effect. [Voxelize](https://cavalry.studio/docs/nodes/behaviours/voxelize/)
This recipe is proposed, not executed in Cavalry; a font switch or another
deformation could also explain the opening.

## Fold implementation assessment

Inspected source, rather than assumed roadmap completion:

| Area | Existing foundation | Work this project may require |
| --- | --- | --- |
| Composition | Native text/paths, scene objects, copy instancing, explicit layer order | Author the portrait layout and choose embedded fonts; exact reference font remains unknown. |
| Motion controls | Typed fields, Math, Random Value, Oscillator, falloffs, exact-time Stagger, glyph/instance responses | Compose drivers; add a deterministic time-varying direction/noise operation only if the chosen trajectory needs it. |
| Editable tools | Group definitions, typed ports, published controls, scoped graph editing | An editable solver update graph and an accessible entry point using the existing group workflow. |
| Persistent state | Element IDs and typed element attributes exist | Explicit previous-state bindings, state materialization, attribute read/write access, and rules for element birth/death or changing topology. |
| Execution | Per-request evaluator memoization, cancellation/work budget, immutable output compilation | A package-owned solve runtime with fixed steps, bounded checkpoints, invalidation and shared preview/export behavior. The prepared-root cache is not a simulation cache. |
| History rendering | Shared shape instances and native path drawing | Bounded trajectory/history collection and replaceable path/profile rendering; neither is currently a solver/history node in the catalog. |
| Blocky type reveal | Native shaped glyph outlines and per-glyph transforms | Test a reusable shape-to-grid operation against the reference; keep glyph animation separate. |

Source pointers: [evaluation](../../crates/motion/src/evaluation/mod.rs),
[prepared roots](../../crates/motion/src/prepared.rs),
[node catalog](../../crates/motion/src/graph/registry.rs),
[groups](../../crates/motion/src/nodes/interface.rs),
[geometry](../../crates/motion/src/geometry/mod.rs),
[fields](../../crates/motion/src/fields/mod.rs), and
[text](../../crates/motion/src/nodes/content/text.rs).

### Proposed delivery sequence

1. Establish one orange element's trajectory and silhouette against reference
   samples. Compare retained instances and swept geometry; choose based on the
   image, not the convenience of a proposed node.
2. Implement the solver with concrete point/content state and typed attributes.
   Keep the update graph replaceable. Materialize state at each step rather
   than retaining lazy field closures that can capture a previous time/context.
   Separate initial input from live inputs sampled at each step. Define start,
   substep, between-step sampling and reset semantics before introducing caches.
3. Add attribute/history/geometry primitives needed by the chosen graph. Build
   the orange-motion tool as an editable group with source, count, seed, speed,
   direction, trail duration, size, color and fade controls as appropriate.
   Do not place these effect controls on the generic Solver itself.
4. Recreate the typography using glyph controls and a reusable grid treatment;
   assemble an editable `.fold` project through the normal transaction and
   Motion → Composite → Sequence interfaces, and render a comparison preview.
5. Validate cold late-frame seeks, reverse/random seeks, fractional time,
   edits/undo, save/reopen, cancellation, bounded eviction, two viewers and
   preview/export equivalence. Checkpoint identity must include solver instance
   context and all relevant animated dependencies, not just node ID.
6. Demonstrate a second update graph, such as a moving falloff that leaves a
   fading per-element attribute, without adding an effect mode to Solver. This
   is the practical test of the user's flexibility requirement.

The primary implementation belongs in `fold-motion`; project/render contracts
continue to consume feature-neutral immutable results. Existing simulation
deferral in the plan should be narrowed explicitly when this slice is scheduled.
The research pass originally changed documentation only. Implementation and
verification are now tracked in [phase 42](../phase-42.md).
