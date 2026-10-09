# C3 — prepared Compositor execution

9 October 2026. Base: C2 commit `6f6f161` (C1: `f80804f`). Implementation and evidence are in the working-tree change following C2.

## Implementation

- Native Compositor records remain the only persistence and authoring representation. `Composite::prepare` borrows them immutably and returns private typed operation/animation state, stable node and parameter ID indexes, a validated reusable interface, and a retained topological order. No mutable access to prepared configurations is exposed. Editing the native records requires releasing that borrow and preparing again.
- Each supported operation is decoded once per preparation. Validation, output traversal, channel inference, dependency/asset declarations, and animation sampling share those contracts. Scalar animation tracks are decoded once and sampled with the shared graph evaluator. Parameter sampling resolves native IDs through indexes. Compilation and read-only document services share document admission and preparation instead of validating and then decoding the same operations again for each consumer.
- Reusable invocation values consume a prepared graph and replace only its typed values and shadowed tracks. Definition records, animation, extensions and identities remain untouched. Static parameter protection is still enforced; authored instance controls retain their native identity/protection/reconciliation path.
- Deleted `configuration()` and `try_configuration()`, including the panic-based assumption that an earlier validation makes publicly mutable records safe. Updated all callers, including native authoring, shader migration, inspectors, app Read/transition consumers, tests and the maintenance example. Input counts are checked before socket indexing. Invalid/unknown operation records return diagnostics; project opaque preservation remains unchanged. Image kernels and pixel expectations were not changed.
- Added only preparation decode counters and a test-only operation-decode probe. The all-operation fixture contains 26 records (the 24 registered contracts plus a source/output pair) and one animation track. Validation, repeated channel discovery, ID lookups and three non-monotonic sample times leave the actual operation decode count at **26**. A two-record reusable fixture remains at **2** after invocation binding and sampling. Tests also check unchanged native bytes and protected runtime defaults.
- Reused the C0 inventory. No new UI controls/feedback, compatibility path, general diagnostic service, schema, dependency, C4 implementation, or M6 work. The maintenance example's replaced conversion-only measurement is now named `compositor_prepare_*`; it measures different work and is not a comparison with C0's historical `compositor_configurations_*` timings. No new timing/allocation claim is made.

## Verification

Commands used `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and `--offline --locked`. Tests used `-- --test-threads=1`.

- `cargo test -p fold-compositor --features ui --lib --tests`: **11 library tests and 1 agent-graph test passed**, with **1 native capture ignored**, before the animation target exposed a malformed pre-existing standalone Crop fixture. It used `Node::new` with zero slots for a one-input operation; it now uses `Node::disconnected`, preserving its exact value assertions. [Initial run, including the resolved failure](initial-tests.log).
- `cargo test -p fold-compositor --features ui --test animation --test channels --test graph_authoring --test native_authoring --test reusable`: **38 passed**, zero failed/ignored. Includes all 24 operation roundtrips, animation/Bezier/combination semantics, channel routing and mask pixels, reusable instances and shaders, protection, stable node/parameter/edge/track/key identities, extension preservation, undo, parameter-service sampling and unknown-record save/reopen without execution. [Final integration results](compositor-tests.log).

- `cargo test -p fold-app --features desktop --test compositor_workflow --test compositor_session --test current_contracts --test parameter_placement`: **19 passed**, zero failed/ignored. Nested timing/alpha/pixels, transactional preview/cancel/undo, stale previews, shader publication, current fixtures, unsupported-record preservation and exact parameter placement all pass. [App integration results](app-tests.log).

**69 distinct focused tests passed; one native capture remains ignored.** The initial malformed-fixture failure is resolved, with no changed pixel/value expectation. No unrelated test failure remains in these invocations. The standalone compositor build reports two existing dead-code warnings in `render/src/gpu/yuv.rs`.

Affected desktop/all-target check passed:

```sh
cargo check --offline --locked -p fold-compositor -p fold-app \
  --all-targets --features fold-app/desktop
```

[Check output](affected-check.log). Touched Rust files were formatted and `git diff --check` passed. Final diff review covered immutable ownership, conversion removal, native identity/protection/unknown payload preservation, validation before indexing, shared pixel kernels, unnecessary duplication and scope. A source search finds no remaining Compositor `configuration()`/`try_configuration()` definitions or callers. `cargo check --offline --locked -p fold-app --no-default-features` also passed: [headless check](headless-check.log).

## Acceptance limits

Implementation and automated C3 gate evidence only. The offscreen ImGui tests cover existing normal/narrow inspector behavior and gesture/undo semantics, not native visual or interactive acceptance. The native shuffle capture remains ignored. No full workspace suite, workspace Clippy, native desktop interaction/capture or native GPU/export qualification was run for C3; this phase is not a broad checkpoint.

C0's desktop SIGSEGV captures and the original freeze remain unresolved. Native latency, GPU allocation measurements and physical acceptance remain unavailable. C1's unperformed desktop normal/narrow interaction remains unperformed. M3–M5/M5a–M5c acceptance remains open and M6 remains gated. C4 remains planned.
