# System map

This describes current ownership and durable contracts, not feature acceptance.
See [current work](current-work.md) for replacement limits.

## Feature boundaries

| Part | Responsibility | Implementation entry |
| --- | --- | --- |
| Motion | Native 2D objects, path points, groups, animation and script controls | `crates/motion-canvas`; host, UI and worker lifecycle in `crates/motion-context` |
| Motion renderer | Native objects and script output use one upstream SVG/Skia path | Isolated `crates/motion-fframes` workspace |
| 3D | Spatial hierarchy, geometry, materials, lights and cameras | `crates/three-d`; Bevy behind the render adapter |
| Composition | Layer assembly and controls | `crates/composition`; cross-document wiring in `crates/app` |
| Compositor | Image-processing graph, masks and Roto | `crates/compositor` |
| Timeline | Time placement of media and document outputs | `crates/timeline` |
| Shared services | Storage, assets, exact time, animation, evaluation, color and resource ownership | `crates/project`, `foundation`, `animation`, `graph`, `media`, `color`, `render`, `gpu-sharing` |
| Host and UI | Provider contracts, shared controls, session and application assembly | `crates/platform`, `ui`, `app` |

Motion and 3D publish image outputs. Composition, Compositor and Timeline can
consume document outputs; they are not a mandatory fixed pipeline. Registration
lives in [app packages](../crates/app/src/packages.rs). Build membership and pins
live in Cargo manifests and lockfiles, not a separate dependency inventory.
Shared infrastructure stays independent of feature-specific document models.

## One document, multiple views

Artists and agents change the same authoritative documents through guarded
transactions. Stable object, point, parameter and key IDs survive edits. Protection,
revision checks, unknown data and Undo apply to both routes.

Motion's canvas, hierarchy, inspector and graph project one native document.
A script remains one object even when it generates thousands of shapes. Exposed
controls change runtime input without changing script source. **Convert to
editable** captures supported output at an explicit time; it does not reconstruct
the script's procedural logic or animation.

Motion schema 5 adds text frames, layer visibility/locks, native transform pivots
and prepared code motion. The canvas and layer timeline share selection and the
existing preview/commit/cancel lifecycle. Select manipulates objects; Direct
Selection retains ImPlot point/handle editing. Shared design geometry owns bounds,
selection transforms, alignment and distribution; UI guides never rasterize artwork.
Typography currently uses the packaged Noto Sans face for deterministic wrapping
and measurements, shared with the fframes text output. Font import and rich text
are not implemented.

An `animate` scoped proposal targets an existing object ID and carries a Rust
`Spec` with source, numeric controls and an explicit sample rate (1–480 Hz).
Its `animate(MotionContext) -> Result<MotionPose, String>` entry may use fframes
animation utilities. Preparation is bounded to 18,000 keys per channel, runs
on the retained worker and stores six separate curves: position offsets in parent
pixels, rotation offsets in degrees, and unitless scale/opacity multipliers. Ordinary
keyframes evaluate first, then these offsets/multipliers apply around the native
pivot. Between prepared samples evaluation is linear; values hold outside the
sampled interval. This is sampled procedural motion, not live Rust evaluation at
every render time. Source/control changes require atomic re-preparation; the
inspector keeps unsubmitted changes in a draft. Playback of native artwork uses
the native worker without compiling its retained motion source. Disabling motion
reveals the authored/keyframed design. Text, shapes, IDs and source remain editable.

There are two different curve editors:

- **Canvas paths:** ImPlot provides point/handle input and guides; fframes/Skia
  renders artwork. Fold owns evaluation, coupling and transactions.
- **Animation curves:** the shared animation editor changes values over time.
  Acceptance of canvas path editing does not qualify keyframe interaction.

## Shared Gallery

`app::gallery` owns the Built-in/My Library catalog and cross-document assembly.
`platform::gallery` exposes the same requests to UI and MCP; project component
identities specify an immutable library revision. Motion owns extraction and fresh
native IDs. Existing placement and effect services assemble Composition and
Timeline instances. Library I/O, previews and motion compilation run on workers.
Previews use the existing renderer and shared GPU host.

A `.foldcomponent` package stores one native Motion document or reusable image
effect, embedded asset bytes, metadata and an optional bounded preview. My Library
uses immutable revision files under the user data directory (`FOLD_LIBRARY_ROOT`
can override it). Placed instances copy documents and retain their version; saving
or importing never edits projects. Explicit updates pass the same provider guards
and Undo transaction as placement. Unknown extension data remains opaque; the
recognized component-origin field may change revision only within its component
identity. Effect interface and placed graphic format changes require a new instance.

MCP uses `gallery` for library operations and `propose` with edit kind `gallery`
for placement/update, followed by candidate inspection and Apply. Motion presets
prepare their retained Rust source against the destination through the existing
motion service. Native properties and existing exposed controls stay editable.

The ten curated fframes components are ordinary native Script documents.
`motion-canvas::presets` owns their public inputs and control presentation metadata;
`motion-fframes::presets` adapts these immutable inputs to the pinned upstream
components and owns each returned SVG tree. The curated third-party source uses
Fold's fframes revision, with no second renderer or media loader. Fonts and images
are packaged resources; image controls resolve through the existing asset snapshot.
Worker raster transport uses lossless deflate/base64 encoding with decoded size
bounded by the declared dimensions and transport allowance. Pixels retain their
explicit premultiplied encoded-sRGB interpretation; image snapshots are sent only
on resource changes. This avoids expanding full-resolution asset bytes into large
JSON number arrays.
The shared inspector uses provider-declared `ParameterSources` to show the source
provider's controls on placed containers, with exact source time and inherited locks.
The owning Motion provider retains preview/commit/cancel state. Source edits use
the same command as direct editing; MCP targets remain the existing stable native IDs.

## Edit and render lifecycle

A drag previews an edit; release commits the matching preview as one Undo entry.
Cancel restores committed data. The app Session owns this lifecycle, including
async command completion. Test lifecycle failures through the real Session,
not just a synchronous widget fixture.

Workers render immutable snapshots. Fold owns exact rational time and time
mapping across placements. Media work and script compilation stay off the UI
thread. Preview, export and headless paths share evaluation semantics, with
explicit color/alpha contracts and resource ownership.

Motion retains workers and compiled scripts. The precompiled SDK supplies the
native-only worker and libraries; authored source compiles with its bundled
`rustc`, not runtime Cargo. GPU sharing avoids CPU pixel transport at qualified
seams. Display observations include a display transform and are not HDR/alpha
exports. See [development](development.md) for SDK setup.

Retime timing, shutter sampling and artist settings belong to Compositor. The
renderer owns GPU preparation, cut detection and reconstruction; `crates/rife`
is the isolated unsafe Linux Vulkan adapter to the packaged ncnn runtime. Model
weights and bounded tensor pools persist on render workers. GPU buffer exchange
uses external-memory FDs, explicit queue-family ownership and host completion
waits; tensors stay on the GPU. The cut detector reads back only a scalar score.
RIFE estimates motion from an sRGB guide, then Fold warps the original linear
premultiplied RGBA with the inferred flow and mask. This preserves floating-point
color/alpha storage; it is not pixel parity with RIFE's ordinary sRGB output.
Auxiliary channels retain temporal frame blending. Model/source pins and checksums
live in the runtime producer, with redistribution notices in its package.

Compositor's parameterless Viewer is a terminal working-route node. Its image
input drives the existing linked viewer panel through the provider's working
output; the first published `video` port still follows Output. Ordinary node
selection never retargets the Viewer. An explicit View action requests the
existing viewer in that editor's link group, preserving local time. Docking is
node presentation metadata, excluded from evaluation identity, and uses the
shared graph gesture/Undo lifecycle. Delete and shake extraction bridge the
primary image pipe (Merge B), including working Viewer consumers. Node bypass
retains authored parameters, animation and connections, and prunes evaluation to
the primary input at the current time, including bypass of temporal processing.

## Replacement constraints

Keep one active implementation per replaced function. Unsupported document
payloads survive through generic opaque project storage, not legacy readers or
fallback engines. Preserve independently valid shared services: Compositor masks
and Roto still use their own vector coverage; that is not a Motion renderer.
Public tools discover provider schemas and capabilities from the connected build.
Repository tests and historical reports do not substitute for that discovery.
