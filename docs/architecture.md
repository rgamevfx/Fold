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

There are two different curve editors:

- **Canvas paths:** ImPlot provides point/handle input and guides; fframes/Skia
  renders artwork. Fold owns evaluation, coupling and transactions.
- **Animation curves:** the shared animation editor changes values over time.
  Acceptance of canvas path editing does not qualify keyframe interaction.

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

## Replacement constraints

Keep one active implementation per replaced function. Unsupported document
payloads survive through generic opaque project storage, not legacy readers or
fallback engines. Preserve independently valid shared services: Compositor masks
and Roto still use their own vector coverage; that is not a Motion renderer.
Public tools discover provider schemas and capabilities from the connected build.
Repository tests and historical reports do not substitute for that discovery.
