# Initial operation coverage

All keys below are suffixes of `fold.motion.`; initial version is 1. This is a planned-home matrix, not discovery advertising. M2 modes must qualify in M3; M5 rendering depends on M4. Remaining family modes described in the proposal belong to M7 unless explicitly assigned M8. Each M7 row requires its own mode fixtures before implementation; none is claimed complete.

| Palette | Key | First phase | Initial modes | Domains | Dependencies | Acceptance fixture |
| --- | --- | --- | --- | --- | --- | --- |
| Artist | `text` | M5 | single packaged font, word groups | text selectors | Parley layout | `kinetic-type` |
| Artist | `shape` | M2 | rectangle | collection/curve | none | `animated-shape` |
| Artist | `path` | M7 | line/quadratic/cubic segments; open/closed subpaths | point/curve | path topology, stable handles | `path-modes` |
| Artist | `image` | M7 | still/video, contain/cover/stretch, explicit source time | collection/UV | image resource service, color decode | `image-modes` |
| Artist | `mesh` | M5 | box prototype | point/face/face-corner | normals/UV preparation | `cloner` |
| Artist | `asset` | M2 | native definition/instance/override; imports M8 | collection | input/output | `asset-overrides` |
| Artist | `transform` | M5 | translation/rotation/scale, explicit pivot | collection/instance | typed spaces | `mixed-scene` |
| Artist | `layout` | M7 | align/fit/row/column/grid, explicit bounds policy | collection/instance | measure, transform | `layout-modes` |
| Artist | `cloner` | M5 | one shared prototype on point stream | instance | distribute, mesh/asset | `cloner` |
| Artist | `merge` | M7 | ordered mixed collection, reject conflicting attribute schemas | all retained source domains | geometry collection, provenance | `merge-modes` |
| Artist | `animator` | M5 | instance scale/color/rotation; explicit combination | instance | signal/field binding | `cloner` |
| Artist | `text_animator` | M5 | word position, group pivot, stagger | word | text, tween | `kinetic-type` |
| Artist | `follow_path` | M7 | normalized progress/distance; clamp/wrap; tangent orientation | instance | path arc length, resample_path, rotations | `follow-path-modes` |
| Artist | `field` | M5 | radial/linear in explicit space | instance | element position | `cloner` |
| Artist | `random` | M5 | uniform deterministic rotation, static named stream | instance | stable IDs/seed | `cloner` |
| Artist | `noise` | M7 | seeded 2D/3D scalar/vector gradient noise; explicit time coordinate | uniform/point/instance | expression IR, space, versioned hash | `noise-modes` |
| Artist | `ramp` | M5 | scalar/color stops, clamp, linear interpolation | uniform/instance | field | `handoff` |
| Artist | `fill` | M5 | solid linear color | collection | retained planar/text | `vector-quality` |
| Artist | `stroke` | M5 | center stroke, round/butt caps, round/miter joins | curve | vector backend | `vector-quality` |
| Artist | `material` | M5 | unlit and bounded lit assignment | collection/face | spatial renderer | `mixed-scene` |
| Artist | `clip` | M7 | planar coverage intersection with explicit space | collection/curve | shape/path, vector backend | `clip-modes` |
| Artist | `extrude` | M5 | closed profiles with holes, caps/sides | point/face/face-corner | tessellate | `glyph-holes` |
| Artist | `deform` | M7 | offset/displace/bend/twist/taper; upstream sampling basis | curve controls/tessellated points/mesh points | field, selection, normals and bounds update | `deform-modes` |
| Artist | `trim` | M7 | distance/normalized interval, open/closed boundary policy | curve | arc-length table, endpoint correspondence | `trim-modes` |
| Artist | `place` | M2 | Screen in M2; World/Camera Plane in M4 | collection | geometry, camera for camera-plane | `mixed-scene` |
| Artist | `camera` | M5 | perspective/orthographic | uniform | typed transforms | `mixed-scene` |
| Artist | `light` | M5 | directional | uniform | linear lighting units | `mixed-scene` |
| Artist | `scene` | M2 | ordered screen and spatial composition | collection | place; camera/light in M5 | `mixed-scene` |
| Foundation | `distribute` | M5 | grid stable integer cell IDs | point | typed spacing/count | `cloner` |
| Foundation | `resample_path` | M7 | count/distance/error; points or rebuilt path | point/curve | arc-length table, provenance | `resample-path-modes` |
| Foundation | `realize` | M7 | selected shared instances with explicit geometry budget | instance to point/curve/face/face-corner | cloner, attribute transfer, identity correspondence | `realize-modes` |
| Foundation | `outline_text` | M5 | monochrome glyph contours | curve/text provenance | text layout and font outlines | `glyph-holes` |
| Foundation | `tessellate` | M5 | closed planar nonzero/even-odd contours | point/face/face-corner | vector adapter | `glyph-holes` |
| Foundation | `filter` | M7 | keep/remove selected parts; explicit topology repair | point/curve/face/instance | selection, topology, provenance | `filter-modes` |
| Foundation | `measure` | M7 | bounds/dimensions/count/center/length; local/projected | collection/curve/instance reductions | geometry, camera for projection, arc length | `measure-modes` |
| Foundation | `element` | M5 | ID/index/count/position context | instance | bound upstream snapshot | `cloner` |
| Foundation | `read_attribute` | M7 | typed named read, error or explicit default on missing | declared storage domain | field context, attribute schema | `read-attribute-modes` |
| Foundation | `store_attribute` | M7 | materialize field, reject or replace existing schema | collection/point/curve/face/face-corner/instance | field evaluation, attribute schema | `store-attribute-modes` |
| Foundation | `convert_domain` | M7 | explicit gather/reduce; mean/nearest; discrete categories | declared source and target domains | topology correspondence, typed reductions | `convert-domain-modes` |
| Foundation | `convert_space` | M7 | position/direction/normal/rotation; explicit mappings | uniform or declared field domain | affine/inverse-transpose, camera/plane for projection | `convert-space-modes` |
| Foundation | `expression` | M2 | bounded arithmetic, interpolation | uniform/instance | typed IR | `animated-shape` |
| Foundation | `select` | M7 | uniform geometry branch; per-element typed value branch | uniform/point/instance | Boolean expression, compatible branch types | `select-modes` |
| Foundation | `time` | M2 | scene/asset rational clock | uniform | clock | `seek-order` |
| Foundation | `tween` | M2 | scalar/vector, linear/cubic-out, clamp | uniform | time | `animated-shape` |
| Foundation | `keyframes` | M2 | scalar/vector, linear/hold | uniform | time | `handoff` |
| Foundation | `wave` | M7 | sine/triangle/saw/square, duration or BPM, phase | uniform/point/instance | rational time, common expression IR | `wave-modes` |
| Foundation | `time_remap` | M7 | affine/curve upstream clock mapping, explicit region | uniform region clock | acyclic dependency graph, keyframes, asset clocks | `time-remap-modes` |
| Foundation | `sample_image` | M7 | nearest/bilinear, clamp/repeat/transparent, channel decode | UV field on declared domain | image resource/source time, color and alpha services | `sample-image-modes` |
| Foundation | `input` | M2 | typed default/protected control | uniform | schema | `handoff` |
| Foundation | `output` | M2 | named typed publication | uniform | schema | `save-reopen` |

Later candidates stay outside the initial 50: `scatter`, `sample_geometry`, `offset_path`, `sweep`, `bevel`, `subdivide`, `morph`, `text_on_points`.


## M5a compositor catalog cutover

The compositor's 28 versioned operations are inventoried separately in
[m5a-compositor-inventory.md](m5a-compositor-inventory.md). All now persist shared
native operation/parameter/edge records under schema 8. Public discovery exposes
native default records and named typed sockets. Useful image kernels remain
specialized; no Motion dependency, old document reader or Parameters-tag protocol
alias remains. Qualification and limitations are in the linked cutover evidence.
