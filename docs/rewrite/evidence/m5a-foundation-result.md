# M5a shared foundation and Composition entry evidence

Date: 8 October 2026. Implementation slice only; M5a is not accepted.

## Implemented

- `fold-graph` owns shared typed graph records, bindings/tracks/ramps, values/units/clocks, expression IR/source, CPU expression execution and diagnostics. These implementations were moved out of Motion, not duplicated. Motion retains its operation registry, document envelope, semantic evaluator and transactional authoring. Compositor consumes the shared socket types; its document and authoring replacement remains pending.
- Motion supplies the existing Network panel through the layer-graph UI contract. Removed the independent Keys tab and timeline implementation. Graph and Source remain available. Inspector typed-key controls remain pending complete shared track projection.
- Add Motion layer creates graph, project item and Composition layer in one transaction. It uses the Composition's exact format/time and the provider's declared output. New starter artwork has explicit paint and a centered placement. The common layer-creation path resolves provider outputs rather than inventing a `video` port.
- Native scalar/distance animation remains editable through Composition's provider channel contract; the integration test verifies stable key IDs, graph/layer transaction history and pixels.

## Automated checks

Commands used `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`; tests additionally used `CARGO_PROFILE_TEST_DEBUG=0`.

| Check | Result and scope |
| --- | --- |
| `cargo test -p fold-graph -p fold-motion -p fold-compositor --offline` | Initial extraction: 136 tests passed; before the final expression-source move and starter changes. `/tmp/fold-m5a-tests.log`. |
| `cargo test -p fold-graph -p fold-motion --offline` | Final extraction: shared graph and all Motion suites preceding text passed. Text fixture failures were corrected and rerun below; this whole invocation is not reported as green. `/tmp/fold-m5a-final-core-tests.log`. |
| `cargo test -p fold-motion --test text --offline` | All 28 text/render-preparation tests pass after fixture correction. `/tmp/fold-m5a-text-tests.log`. |
| `cargo test -p fold-app --test common_composition --offline` | All seven tests pass, including new atomic Motion creation, exact format, native key editing, provider output identity, undo/redo and rendered pixels. `/tmp/fold-m5a-composition-tests.log`. |
| `cargo test -p fold-app --test motion_video --test motion_cutover --offline` | Nine tests pass, covering publication/alpha/color/source times and opaque old-motion rejection. `/tmp/fold-m5a-app-regressions.log`. |
| `cargo test -p fold-motion --features ui --lib --offline` | 38 passed, six GPU captures ignored at this stage; four expression-source tests subsequently moved to `fold-graph`, and the new Network capture was added. Includes normal/narrow draw and transaction tests. `/tmp/fold-m5a-motion-ui-tests.log`. |
| `cargo test -p fold-motion --features ui --lib shared_network_capture --offline -- --ignored` | Final Network capture passes on the GPU; no project revision change. `/tmp/fold-m5a-network-capture.log`. |
| Desktop library check and affected-library Clippy | Passed; final Clippy command covers graph, motion, compositor, composition, platform and app with `fold-app/desktop`. Existing media/render/platform/UI/compositor/timeline/app warnings remain; no new error. `/tmp/fold-m5a-desktop.log`, `/tmp/fold-m5a-clippy-final.log`. |
| `python3 scripts/check_boundaries.py` and `python3 scripts/test_boundaries.py` | Package/headless isolation passes; nine policy tests pass. `/tmp/fold-m5a-boundaries.log`, `/tmp/fold-m5a-boundary-tests.log`. |
| `cargo fmt --all -- --check`; `git diff --check` | Passed. |

The new visible starter exposed fixtures that implicitly relied on unpainted origin geometry or on the first Fill operation belonging to their custom text branch. Motion-video and style/text fixtures now explicitly establish the same original test geometry; numerical/pixel assertions were retained. Initial failures, including the missing paint caught by the new Composition test, were corrected rather than waived. The final grouped test invocation was not repeated after the focused text follow-up.

## Screenshot-only review

[Normal and narrow Network capture](m5a-network.png) was rendered by the actual Dear ImGui/WGPU panel implementation at 800- and 360-pixel widths and visually inspected. Graph/Source tabs and the asset/control row remain legible; narrow secondary controls use overflow. The graph canvas clips/pans its contents normally. No Keys tab or unconditional rendering-unavailable label remains. This is an offscreen UI capture, not a live desktop interaction test.

The user permits screenshots only: no computer-use clicks, typing or drags. Before that clarification, read-only capability discovery returned `runtime_unavailable`; the prescribed Orca startup attempt returned `runtime_open_timeout`, and the retry remained unavailable. Fold session discovery returned no connected sessions. No live application interaction was performed. Native selection routing, physical editing, focus/keyboard behavior and artist continuation remain unverified.

## Still required for M5a

Authoritative scene/object hierarchy and render assembly, Outliner, nested Composition object rows, per-object Network routing, full typed track projection and interpolation, compositor document/source/authoring replacement, verified public multi-document examples, and the mixed two-scene acceptance fixture. The existing demo is still a flat construction. This work does not close M0–M5 rendering/resource obligations or qualify the full workspace vertical-slice checkpoint.
