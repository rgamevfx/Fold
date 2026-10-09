# Geometry attributes and Rhai Wrangle

Discover the Motion provider in `scoped_proposals` by `type` and use its
`requests.catalog` as `catalog` below. M5b builds expose `catalog.wrangle.functions`
from the actual registered host functions. Older running builds do not acquire
these capabilities when the repository changes. Use the guarded
[public workflow](public-workflow.md) for proposals, inspection, Apply and saving.

## Native attribute workflow

`distribute.grid` version 2 now returns mesh Geometry. Its points can feed `cloner.points` version 2
(Copy to Points), which also accepts points belonging to another mesh. Grid faces
are triangles; Primitive means triangle, Corner means triangle corner, and Edge
includes triangulation edges. Supported attribute domains are `point`, `primitive`,
`corner`, `edge`, `detail`, and `instance`; the input geometry must expose the chosen
domain. Instances expose Instance, not Point; realize support is not implied.

The background-sphere construction is:

```text
Sphere ────────────────────────────────┐
Grid → Attribute Randomize (point Cd) → Copy to Points → Soft material → Output
```

- `attribute_create.set`: geometry, domain, attribute, typed literal value.
- `attribute_randomize.uniform`: geometry, domain, attribute, matching typed
  minimum/maximum, integer seed. Float/vector components use independent uniform
  samples; integers remain integers; quaternion bounds use spherical interpolation
  with a uniform amount. Normal results are normalized on the mesh point domain.
- `attribute_transfer.radius`: destination `geometry`, `source`, `domain`,
  `source_domain`, `attribute`, positive metric `radius`, and `sampling` (`nearest`
  or `blend`). Spaces/units must match. Blend uses linear radial falloff and supports
  floats, vectors and quaternions. Categories require nearest. Unmatched elements
  retain their existing destination value; absence of that value is an error.
- `cloner.points`: prototype, point-bearing geometry, and whitespace-separated
  `attributes` to copy. Missing stored attributes are skipped. Placement consumes
  `P`, optional quaternion `orient`, and optional positive float `pscale`.
  Prototypes remain shared. Copied `Cd` lives on instances and drives material RGB.

`P` reads/writes actual mesh positions, stored at mesh f32 precision, in World
meters. `N` addresses point normals. `Cd` is a three-component vector with
scene-linear Rec.709 encoding and separate opacity; `orient` uses a normalized
World quaternion in x/y/z/w order. Other names carry an explicit value type and
domain. Type conflicts fail rather than coerce. Public CPU observation includes
stored attribute schemas/values and intrinsic mesh positions/normals; instance
measurements include orientation.

## Wrangle contract

Create `fold.motion.wrangle`, version 1, mode `rhai`. Read its current parameter
contracts and defaults from discovery rather than constructing Rust objects.

- `inputs`: list of geometry bindings; empty is valid.
- `run_over`: `detail` (once), or a supported element domain of the first input.
- `source`: Rhai source, retaining comments and formatting inside the native node.
- `parameters`: JSON object available as `params`. Numeric/boolean entries have
  native inspector controls; definitions remain editable. These entries are
  uniform script parameters, not independently keyed native channels.
- `time`: native Duration binding, defaulting to asset time.

Return a Geometry as the last expression. `geo` starts as the first input or empty
geometry; `input` and `inputs` start each invocation from the original upstream
snapshots. Geometry values use copy-on-write: changing a local input copy cannot
mutate upstream geometry or the next invocation's input. In element mode, `index` and integer `id` identify the
original element. Invocation order is the original domain order; edits accumulate
in `geo`, while an unmodified `input` provides the original sampling basis. Geometry
creation is allowed in every mode. Deleting topology can invalidate later output
indices; use original input queries and explicit output indices deliberately.

`time` is floating-point seconds derived from the requested asset clock.
`time_numerator` and `time_denominator` retain its exact rational components.
Each evaluation starts fresh: use time-driven procedures, not frame accumulation.
`id` and `element_id` preserve 64 identity bits in a signed integer, never a float.

Rhai is dynamically typed. Fold validates host arguments, attributes and output
geometry; some errors are runtime errors with source locations. Prepared ASTs are
cached across time/parameter evaluations. No Cargo build or restart is involved.
Source is limited to 64 KiB; execution shares the graph work budget and geometry
quota, with bounded script operations, recursion and containers. Script and native
loops are cancellable. Modules use a closed resolver; there is no filesystem,
network or process interface. Failed evaluations publish no partial geometry.

## Examples

A complete sphere animation in one Wrangle, with no inputs:

```javascript
let points = grid(11, 4, 0.5);
for i in 0..count(points, "point") {
    let p = point(points, i);
    p.z = sin(time + p.x * 2.0) * 0.2;
    set_point(points, i, p);
    let key = element_id(points, "point", i);
    set_attribute(points, "point", "Cd", i,
        vec3(random(key, 12), random(key, 19), random(key, 27)));
    set_attribute(points, "point", "orient", i,
        orient(vec3(0.0, 1.0, 0.0), time));
}
copy_to_points(sphere(0.18), points, "Cd")
```

Connect its output to Material and the ordinary scene/output construction. Source
creates geometry; camera, lighting and presentation remain explicit downstream
scene data.

Point Run Over, creating geometry from each original point:

```javascript
let p = point(input, index);
add_point(geo, p + vec3(0.0, sin(time), 0.0));
geo
```

Detail Run Over, procedural topology:

```javascript
let g = geometry();
add_point(g, vec3(-1.0, 0.0, 0.0));
add_point(g, vec3(1.0, 0.0, 0.0));
add_point(g, vec3(0.0, 1.0, sin(time)));
add_triangle(g, 0, 1, 2);
compute_normals(g);
g
```

Use floats (`0.0`) for vector components, positions and radii. `orient(axis, angle)`
takes radians. `random(integer, seed)` is deterministic; `noise(vec3, seed)` is
continuous lattice value noise in [-1,1]. Discover math/vector/quaternion and
geometry function signatures under `catalog.wrangle.functions`.

Topology edits preserve surviving attributes by stable element identity. New
elements receive typed defaults (zero, false, empty text, identity rotation;
normals use +Z and pscale uses one). Native curves, mixed-prototype
collections, realization, arbitrary module imports, named output sockets,
individually keyed JSON parameters and stateful simulation are not qualified by
this first runtime. Do not advertise those as working.

## Verified public projects

[Background spheres](../../../demos/m5b-background-spheres.fold) uses Grid →
Attribute Randomize (`Cd`) → Copy to Points with a shared Sphere prototype.
[Animated Wrangle](../../../demos/m5b-wrangle.fold) creates and animates all spheres
inside one input-free Wrangle. Its `amplitude` and `speed` controls are live.

To reproduce on an isolated copy containing the Background spheres object, use
`python3 demos/m5b-background-spheres.py <fold-cli> <socket> <new.fold> <new-report.json>`;
append `wrangle` for the programmable example. Destinations must be unused. The
workflow discovers contracts, scopes its source edit to the inspected object,
inspects and applies a candidate, checks public frames, and saves a copy. It needs
no Rust source access. The Wrangle variant verifies source/parameter edits and
repeatable seeks without rebuilding the running app.
