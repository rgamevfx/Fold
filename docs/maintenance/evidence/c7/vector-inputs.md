# C7 vector dispatch inventory

Base: C6 `ecf016f`. Reuses `docs/maintenance/c0-inventory.md` and traces
`gpu::VectorPipeline::packet` → `Sources::prepare` → `GeometryCache::get`.
There was exactly one call into `vector_legacy`: any surviving paint whose
transformed local hierarchy bounds had a component with absolute value >65,536
sent the **entire drawing batch** to that pipeline. It was not a device fallback.

| Input reaching that dispatch | Retained qualification |
| --- | --- |
| Finite affine transforms, large intersecting/offscreen bounds, negative scale, reflection, shear and cancelling translation | Extreme-coordinate native matrix; f64 viewport clipping produces ordinary identity instances for the retained hierarchy/bitset/coverage pipeline |
| Move/Line/Quad/Cubic/Close, multiple contours, holes, nonzero and even-odd fills | Extreme curved/holed path fixture and retained text/fill-rule native fixtures |
| Fill-only, stroke-only and combined paint; open and closed paths | Extreme matrix includes closed fill/stroke and open stroke in the same ordered batch |
| Butt/Round/Square caps, Miter/Round/Bevel joins, positive finite stroke widths | All nine cap/join combinations in each extreme transform/fill-rule case; existing thin-stroke/text checks retained |
| Signed/HDR scene-linear RGB, fractional alpha and overlapping ordinary/extreme paints | Extreme matrix uses signed/HDR fractional paints between ordinary rectangles; reference comparison <1e-4 per channel |
| Ordinary drawings accompanying an extreme paint (the old whole-batch dispatch) | Ordered mixed batches now retain ordinary instances; no change in paint order or premultiplication |
| Transparent, empty, singular, fully offscreen and zero-width paint accompanying a triggered batch | Existing no-work preparation tests plus extreme invisible-paint fixture; no execution space is consumed |
| Repeated paths, changed colors/transforms, packet reuse, abandoned encoders and budget rejection | Existing native source/atlas/packet tests retained; no cached packet is treated as evidence of completed GPU preparation |

Both working spaces still enter the shared vector kernel; top-level graph color
validation remains authoritative. Invalid transforms/colors/path coordinates,
segment/subdivision/triangle/coverage/tile and aggregate host limits remain errors.
An exploratory 1e8/1e6 scale fixture exceeded the existing subdivision precision
budget; the qualified matrix stays within the supported geometry contract.
An exploratory below-dispatch-bound case (`[6553.7,0,0,1,-32768,0]`) produced
0.001354 channel error under an intentionally stricter 1e-4 probe; it never reached
the removed implementation. The C7 extreme matrix instead tests a bound above
65,536. Existing text/fill-rule qualification retains its 2e-3 tolerance. No
production threshold or existing qualification tolerance was loosened.

Removed: old Rust pipeline, WGSL, registration, pipeline field/construction/clear,
optional-packet dispatch, `needs_cpu_transform`, fallback report/statistics fields,
and their application/test consumers. GPU-only paint-record layout/bounds were
removed from the CPU binning records. CPU analytic reference execution and its
binning remain independently required. No crate dependency was exclusive to the
removed pipeline; manifests and lockfile are unchanged.

Shared clipping is geometry preparation, not a second coverage/compositing
implementation. It retains f64 viewport intersection before f32 upload, applies
the transform once, and uses the same retained GPU preparation, visibility and
coverage passes. There is no alternate shader or fallback framework.
