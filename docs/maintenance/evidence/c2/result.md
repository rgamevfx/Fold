# C2 — typed scheduling failures and optional provider capabilities

9 October 2026. Base: C1 commit `f80804f`. Implementation and evidence are committed together in the C2 change following that base.

## Implementation

- Added feature-neutral `fold_foundation::{Failure, FailureKind}`: cancellation, temporary resource pressure, permanently oversized request, invalid input, missing resource, unsupported capability and execution failure. Explanatory text and contextual IDs remain separate. Existing presentation receives the message; structured context does not add artist-facing feedback. Unclassified library/non-scheduling string errors become non-retryable execution failures, never classifications inferred from wording.
- Media storage/source/process admission and GPU reservations produce categories at the failure site. An allocation larger than its entire allowance is permanent; contention with held leases is temporary. CPU live-pixel limits are permanent. Kept existing admission ceilings, lease ownership, cancellation and 30-second preview/export retry bounds. Preview CPU admission now uses the same typed pressure policy as evaluation.
- Propagated errors through decode/cache, CPU/GPU evaluation, native allocation callbacks, nested provider compilation, effect stacks, and output/presentation preparation. The signature changes in provider compilers and render kernels carry errors only; they do not change creative models, preparation representations, algorithms or pixels. Native allocation callbacks preserve their caller's error type without adding a dependency or compatibility entry point.
- Removed message-based decisions from preview, export, thumbnails and EXR cache recovery. Only `ResourcePressure` retries. Thumbnail failures that are permanent remain cached; pressure remains eligible for a later request. EXR retains its single cache-trim retry. Error text containing the former trigger phrases cannot cause a retry.
- Reduced required document behavior to identity, schema support and validation. Optional trait references register authoring, queries/receipts, creation/browser metadata, parameters/source placements, animation, hierarchy, artist unlocking, settings, placement, image inputs, relinking and specialized evaluation identity. Separate capabilities reflect actual differences among the existing providers. `has_capability` derives support from those references and the current schema, with no duplicated boolean declarations or dynamic plugin system.
- Deleted empty-success provider defaults and unsupported method stubs. Direct calls to absent capabilities fail explicitly. Generic project parameter traversal, native inspection and desktop adapters discover support before requesting optional data. Their existing empty views/optional metadata remain unchanged. The generic validated payload identity belongs to the registry when no specialized identity exists. A validation-only provider remains registered, valid and usable without authoring or video.
- Specified [C4's scoped proposal interface](../../c4-scoped-proposal-interface.md): provider-owned preparation, inspection, reconciliation, protection and diagnostics; host/project-owned candidate lifetime, retry identity, proofs, revision/epoch guards and atomic mixed-document publication. No C4 interface implementation or dispatch ownership move was made.

No new UI feedback, compatibility shim, diagnostic infrastructure, dependency, schema, public protocol generation, C3 representation rewrite or M6 work was added. The C0 inventory was reused; the audit was not repeated.

## Verification

Commands used `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and `--offline --locked`. Tests used `-- --test-threads=1`.

**84 focused tests passed, zero failed, zero ignored in these invocations.** Filtered-out and uninvoked tests are not qualification evidence.

| Invocation after `cargo test --offline --locked` | Passed | Evidence |
| --- | ---: | --- |
| `-p fold-app --features desktop --lib preview_worker::` | 11 | [Preview cancellation, pressure, bounds and fairness](preview-tests.log) |
| `-p fold-app --features desktop --lib output::tests` | 4 | [Export wording independence/cancellation plus two matching playback tests](output-tests.log) |
| `-p fold-app --features desktop --test provider_capabilities --test agent_dispatch --test agent_retry --test agent_jobs --test agent_observe --test parameter_placement` | 43 | [Provider discovery, exact-time placements and existing agent conflicts/cancellation](provider-agent-tests.log) |
| `-p fold-platform --test packages --test video_compile --test outputs` | 7 | [Minimal provider, registration, transactions, nested cancellation and outputs](platform-tests.log) |
| `-p fold-platform --test video_compile nested_failure` | 1 | [Nested failure retains category and document IDs](nested-failure-test.log) |
| `-p fold-foundation failure::` | 1 | [Wording-independent categories and quiet presentation](failure-tests.log) |
| `-p fold-media --lib budget::` | 1 | [Held leases, recovery and oversized requests](media-budget-tests.log) |
| `-p fold-media --lib process::tests` | 1 | [Bounded process admission and audio reservations](media-process-tests.log) |
| `-p fold-media --lib video_stream::tests` | 2 | [Blocked-pipe cancellation/reaping and preserved codec diagnostics](stream-tests.log) |
| `-p fold-render --features gpu --lib scheduling::tests` | 4 | [Admission cancellation, fairness and reserved lanes](scheduling-tests.log) |
| `-p fold-render --features gpu --lib budget_tests` | 1 | [GPU pool oversized/pressure classification without a device](gpu-budget-tests.log) |
| `-p fold-render --test graph` | 7 | [CPU graph behavior and permanent live-pixel limits](cpu-budget-tests.log) |
| `-p fold-ui --lib media_thumbnails::tests` | 1 | [Thumbnail retry/cache behavior with misleading wording](thumbnail-test.log) |

Affected-crate check passed:

```sh
cargo check --offline --locked --keep-going \
  -p fold-foundation -p fold-media -p fold-native-video -p fold-render \
  -p fold-platform -p fold-timeline -p fold-composition -p fold-compositor \
  -p fold-motion -p fold-ui -p fold-app \
  --features fold-app/desktop --all-targets
```

[All affected targets](affected-check.log). Final review identified nested Composition/Timeline/effect helpers that needed to retain the category and kept contextual IDs out of existing presentation text. The affected follow-ups passed: [Media/Platform/Timeline/Composition/App all targets](final-check.log), [App desktop all targets after presentation preservation](presentation-check.log), [headless App](headless-check.log), and [App desktop library after removing two redundant conversions](review-check.log). The added nested-failure/thumbnail tests and updated foundation/codec tests were run after their respective changes. Compiler-driven adaptations during iteration were resolved; no unrelated failure remains in the C2 gate.

Touched Rust files were formatted. `git diff --check` passed. Final diff review checked error propagation, removed defaults, unnecessary conversions, scope and UI presentation. A focused search found no remaining `budget exhausted`/`capacity exhausted` string-based scheduling decisions. No full workspace test suite, workspace Clippy, native render or desktop interaction run was performed; those are not C2 checkpoint requirements.

## Acceptance limits

This is implementation and automated C2 gate evidence only. It is not native artist/agent acceptance or a performance comparison. C0's desktop SIGSEGV captures and the original freeze remain unresolved. C1's unperformed desktop normal/narrow interaction and physical acceptance remain unperformed. M3–M5/M5a–M5c acceptance remains open and M6 remains gated. Headless tests and ImGui thumbnail-cache tests do not qualify desktop lifecycle behavior. C3 and C4 implementation remain planned.
