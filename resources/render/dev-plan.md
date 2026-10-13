# Compositor optimization plan

Status: partial delivery. Do not mark the full plan complete.
This file owns the phase checklist. `report.html` contains the measured results.
The report uses short, direct English in the style of ASD-STE100. It is not a
formal language-conformance check.

## Completed work and open work

- [x] P1 — Done: pool completed status readback buffers.
- [x] P2 — Done: fuse color conversion, validation, and display storage.
- [ ] P3 — Partial: one graph parameter arena and a blur-kernel cache are in use.
  Bind groups are still created per pass. Color and filter parameter buffers are
  not part of the graph arena. General buffer/binding reuse remains open.
- [ ] P4 — Partial: fuse masked pointwise chains and single-input channel shuffles.
  General merge fusion and constant inputs inside a consumer shader remain open.
- [ ] P5 — Open after tests: reject both new blur memory layouts. Neither layout
  gave a useful speed gain. Keep the exact separable filter. Radius-dependent
  box/Gaussian algorithms remain open.
- [ ] P6 — Partial: reuse equal constant sources and blur subgraphs in one request.
  Prepared document/topology caches and cross-frame effect caches remain open.
- [x] P7 — Done: use bounded, incremental cache eviction with two thresholds.
- [ ] P8 — Open: per-node regions, tiled execution, and source-region decode are
  not implemented. Existing region tests pass with the other changes.
- [ ] P9 — Open: format-aware working storage and pooled native ingress are not
  implemented. Direct import and automatic FP16 are not enabled.
- [x] P10 — Done for this change set: run focused checks and build the HTML report.
  Full workspace, installed package, and desktop artist acceptance remain open.

## Method

Use release builds of `fold-render`. Run the smallest useful test first. Run GPU
tests in one thread. Measure 60 frames after ten warmup frames. Use the same GPU
and workload for comparisons. Keep raw data in `benchmarks/`. A renderer timing
is not a desktop frame-rate measurement. The desktop stayed active. GPU clocks
and other GPU work were not controlled.

## Development notes

### P1: status storage

The reuse test failed before the change. The first setup attempt also found that
the test needed COPY_DST usage to write an invalid status. That setup failure was
fixed before the reuse failure was used as evidence.

The host now retains at most eight completed, unmapped readback buffers. Live
requests never share a buffer. The GPU flag is explicitly cleared in the command
stream. Map or device failures do not return storage to the pool. A dropped
consumer keeps its callback-owned storage until completion. Idle buffer bytes
are included in reclaimable memory. Pressure can release them.

The single-solid median fell from 6.726 ms to 1.111 ms after this phase alone.
GPU time stayed near 0.70 ms. The failure, cancellation, device-loss, and host
accounting tests pass. The synthetic 3 ms p95 gate now passes. This is a diagnostic
gate, not an agreed product limit.

### P2: color passes

The display allocation test failed before the change. Color conversion now checks
the float result before its final store. The display path does not allocate a
second full float image. The old validation shader was removed.

Naga 27 does not parse GLSL atomicOr. A direct implementation failed its GPU tests.
The final implementation parses the generated GLSL, changes only the generated
validation field and store to atomic typed IR, and passes that module to wgpu.
It does not use a racy write or rewrite arbitrary OCIO expressions. The atomic
change checks the binding, member count, function name, and store count.

Working overflow, display allocation, signed/HDR alpha, pinned OCIO LUTs, raster
input, and ProRes alpha parity tests pass. This does not qualify HDR monitors.

### P3: parameters and kernels

Generic image operators and pointwise chains use one mapped uniform arena for
each graph. Every binding uses a separate aligned range. The arena is unmapped
before submission. The submission owns the budget lease until completion.

The filter keeps at most 16 exact kernels. Keys include the float size bits and
filter type. Submitted work holds its own reservation after cache eviction.
The kernel reuse/eviction test and GPU pixel tests pass. Bind-group reuse needs
stable, completion-safe buffer identities; it must not cache a previous frame's
parameter ranges.

### P4: pass fusion

A mask mix can consume a pointwise chain in one pass when the chain starts at the
mix's original input and no other node uses its intermediate results. Up to 16
steps can be fused. A one-input shuffle can be one of those steps. The shader
checks every logical intermediate, even if a zero mix hides it in the final image.

Tests cover mask absence, mask aliasing, inversion, amounts 0/0.25/1, selected
channels, odd dimensions, and hidden overflow. Existing shared-input tests pass.
General merges, multi-input shuffles, and arbitrary spatial operations remain
separate. Do not relax those boundaries without new lifetime and pixel tests.

### P5: rejected blur layouts

Test A used 8x8 output tiles and a 72x8 shared-memory strip for vertical filtering.
At size 128, total GPU time rose from about 29.25 ms to 49.97 ms. Reject this layout.

Test B transposed the output of each separable pass. Both passes then read along
rows. Total GPU time was about 29.66 ms. It did not improve the baseline and added
a differently sized scratch texture. Reject this layout too. Neither experimental
shader remains in the engine. The raw size-128 measurements remain in benchmarks/.

Next: build a filter-only benchmark with axis timestamps. Test a sliding-sum box
filter separately from Gaussian. For a different Gaussian algorithm, define and
test an error bound first. Do not silently change the exact filter to a pyramid
approximation. Current full-image cost is still proportional to filter radius.

### P6: repeated work

The shared CPU/GPU planner now reuses equal solids and blur results within one
request. It runs after logical validation. Keys use exact float bits and canonical
input IDs. External sources and authored shaders are not assumed to be pure.

An early version also shared opacity results. It disabled opacity-over fusion
and raised the small 64-layer case's peak allocation to about 28.4 MiB. Reject
that policy. Cheap pointwise results now remain available for consumer fusion.
The final small-case peak is about 2.9 MiB. Tests lock down this choice, distinct
parameters, signed zero, and repeated filter execution.

One old CPU budget test used four equal solids. These now share one image. The
test was changed to four distinct colors so that it still tests budget rejection.

Next: move immutable prepared compositor data into an owned cacheable value.
PreparedComposite currently borrows authored nodes. Do not store those borrows
past their snapshot. Cache identity must include document content, invocation
values, dimensions, color configuration, placement, and source identity. A
cross-frame image cache also needs time, region, cost, and resource-budget keys.
No prepared-document or cross-frame effect cache is implemented in this change.

### P7: pressure

Pressure work starts above 75 percent live allocation. It stops at two thirds of
the budget or when no cache can remove an entry. It removes one entry at a time
across eight cache classes. Each request does at most 128 eviction attempts.
Submitted and displayed resources keep their own leases. The new pressure
counter records removed entries. A test removes seven of eight coverage entries,
keeps the newest entry, and verifies that a second call does no extra work.

This is a bounded fair policy, not a globally cost-ranked cache. A real mixed-media
pressure benchmark and detailed hit-rate telemetry remain useful follow-up work.

### P8: region design gate

The graph currently uses one working raster and one coordinate system for all
operators. Region planning uses a common enclosing rectangle. External Source,
Spatial, and unknown Shader footprints stay conservative. WindowedSource still
renders its native source before extraction.

Next: add explicit origin/extent metadata to physical image values and operator
bindings. Propagate per-input demand bounds. Keep filter halos and edge rules.
Add region demand to source adapters where the decoder can support it. Test odd
sizes, transformed inputs, alpha, data channels, blur boundaries, and source
fallback bounds against full-frame pixels. Then add bounded tile scheduling.
These changes are not implemented; smaller allocation limits alone are not tiling.

### P9: format and ingress design gate

Working operators still use RGBA32F. The existing native ingress copy still
establishes compositor ownership. FP16 storage can lose range and precision.
Direct import can extend the producer lifetime past its current release point.

Next: add a typed, device-owned destination pool to the shared-image boundary.
Check dimensions, format, device identity, queue ownership, and completion before
reuse. Test cancellation and producer failure before removing any copy. Add an
explicit storage/quality policy before FP16 working operators. Test extreme HDR,
small alpha, repeated grades, filters, and export parity. These changes are not
implemented. No copy or precision check was removed to claim a speed gain.

### P10: evidence and limits

`validation.json` records commands and test counts. The focused tests pass.
`build_report.py` builds `report.html` from raw timings and this plan. The report
includes failed experiments and open work. No desktop rebuild was needed.
Existing uncommitted media, Motion, and UI edits were left unchanged.

Before release, run workspace checks, tests and Clippy. Check the installed
package and real Session workflows. Benchmark a real slow project with distinct
sources, media decode, ACES, masks, Motion and cache pressure. Obtain artist
acceptance separately. This change set is not full completion of all ten phases.
