# C8 continuation — local Compositor preparation

9 October 2026. See [maintenance evidence](../../maintenance/evidence/c8/result.md)
for the ranked current costs, controlled release comparison and validation.

The eligible measured hotspot is Compositor preparation: unanimated nodes were
allocating animation-channel tables with no paths to validate. They now skip
that empty work after track preparation and identity checks. Existing animated
validation, immutable native borrows, stable IDs, exact times and color/alpha
semantics remain. No Motion evaluator, provider protocol, cache, UI or GPU path
changes. C4/C5/C6 ownership and C7 completion safety remain intact.

This qualifies only the recorded CPU preparation workload, not edit-to-present,
preview/export throughput, GPU allocations or physical artist acceptance. C8's
local optimization does not start M6. Motion dependency compilation, source/layout,
topology, field, time-sample and presentation-context caches remain gated by
M5/M5a/M5b/M5c acceptance. No cache result is introduced that could alias distinct
transient roots, provider implementations or invocation contexts.

C7 remains complete by user acceptance with the inherited extrusion ray-depth
failure deferred. That fixture remains failed/open at its unchanged tolerance;
M4 acceptance remains open. C6's causally verified empty-layout restore fix is
preserved. The original unknown-trigger freeze, separate C0 unbound shutdown
SIGSEGV and physical artist acceptance remain open. No M3–M5/M5a–M5c gate or C9
work is claimed complete by these CPU results.
