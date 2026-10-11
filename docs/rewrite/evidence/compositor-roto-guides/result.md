# Roto pen guides and keyed evaluation — 10 October 2026

The artist reported invisible open Bézier/B-spline guides and slow interaction
after default shape keying. Two separate implementation faults were confirmed.

1. The existing pen draft went through the ordinary unselected-shape overlay.
   Its ID was absent from the selected document, so anchors/handles were hidden
   and the line was subdued. There was no live cursor segment or close target.
   The keying work retained this faulty path; its closed-shape tests missed it.
2. Fully keying shapes exposed quadratic evaluation: every keyed scalar called
   `property_reference`, rebuilding the entire property list. A prepared 20-point
   keyed shape took a median **31,170 µs** to evaluate in the debug build.

The pen now draws an explicit draft overlay independent of selection and image
readiness: outlined accent curves/anchors, visible tangents, a B-spline control
polygon, cursor preview, and a 10-pixel closing target with a Close label. The
first anchor appears immediately. Clicking the target or Enter closes the same
draft through the existing transaction. A failed close retains the draft.
Ordinary mouse motion no longer clones the entire native node; the clone occurs
only when freezing a gesture. Toolbar tooltips now use ImGui's item-tooltip API
so retained button focus does not place help over an active drawing.

Evaluation now walks native component offsets once, using the same reference
addresses and sampler for curves, links and expressions. The identical fixture
measures **1,675 µs**, about 19× faster. This measures prepared CPU evaluation,
not total frame latency or physical pointer-to-display latency.

## Verification

All commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0`, `--offline -j 2`.

- `cargo test -p fold-compositor --features ui --lib roto_open_pen -- --nocapture` initially failed with “open pen must draw its first anchor before closure.” [Failing log](guide-before.log).
- `cargo test -p fold-compositor --features ui --lib roto_keyed_evaluation_cost -- --ignored --nocapture`: [timing baseline](timing-before.log).
- `cargo test -p fold-compositor --features ui --lib roto_ -- --include-ignored --nocapture --test-threads=1`: seven passed, including actual mouse input for both tools, first-point/live-guide geometry, stale-image interaction, enlarged close target, closure/first keys in one undo step, existing drag/cancel/key controls, and CPU timing. [Final log](tests.log).
- `cargo test -p fold-compositor --features ui --test references --test roto`: five passed; three native GPU tests skipped in this invocation. Public links/expressions and existing Roto animation/feather semantics remain intact. [Log](references.log).
- GTX 1070 native captures reviewed: [Bézier](bezier.png), [B-spline](bspline.png), [close target](close.png). A checkerboard tests visibility over bright/dark areas. Existing normal/narrow inspector captures also ran.
- `cargo build -p fold-app --features desktop --bin fold-desktop`: passed. The executable is rebuilt; the existing artist session was not closed. [Log](build.log).

No document model or render algorithm changed. Full workspace validation and a
physical artist session remain unverified. Existing Motion gates remain open.
