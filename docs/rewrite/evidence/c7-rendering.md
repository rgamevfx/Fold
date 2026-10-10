# C7 continuation of M4 rendering/resource qualification

9 October 2026. [C7 results](../../maintenance/evidence/c7/result.md) and
[input inventory](../../maintenance/evidence/c7/vector-inputs.md) own the detailed
commands, failures and removal evidence.

Request preparation, operation encoding, submission/completion, immutable frame
ownership and statistics now have focused Render modules. Host accounting and
explicit completion-owned leases remain shared. One retained vector execution
path covers the old large/intersecting-bound dispatch through shared f64 geometry
clipping; its duplicate Rust/WGSL execution and counters are deleted. Explicit
CPU reference execution remains. No Motion document, evaluation, authoring,
provider, Session or desktop ownership contract changed.

Final GTX 1070/Vulkan verification passes eight vector, three resource-lifecycle,
six image/adapter/region and seven spatial tests. Native device destruction
exposed and verified a fix for reading destroyed status buffers after an already
queued successful map callback. Completion now decodes after callback delivery,
checks host failure and retains accounting through consumption/abandonment.
Device-loss testing is explicit device destruction, not a physical driver reset.
Earlier camera-plane, text, color, sampling, YUV and precision fixtures also passed;
the C7 result distinguishes those runs from the final completion replay.

The eighth spatial fixture fails its unchanged extrusion ray-depth tolerance at
(71,50). The retained C6 checkpoint executable reproduces the same depth values
and failure. Planar text interiors pass; this does not qualify the failing extruded
surface. After review, the user accepted C7 maintenance completion with this
specific inherited failure deferred. C8 maintenance work may proceed. The failing
fixture and M4 acceptance remain open. No tolerance was loosened, test marked as
passed or native artist acceptance inferred from GPU tests.

Preserve all remaining M3–M5/M5a–M5c acceptance obligations. The original
unknown-trigger freeze, separate C0 unbound shutdown SIGSEGV and physical artist
acceptance remain open. C6's empty-layout restoration SIGSEGV remains causally
fixed. The status-buffer/device-destruction regression is a separate finding.
C8 has not begun, and M6 remains gated.
