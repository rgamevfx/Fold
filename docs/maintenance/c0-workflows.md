# C0 acceptance workflows and reproduction

These fixtures establish the behavior to preserve during maintenance. They do not expand Motion capability acceptance. The [profile](c0-profile.json) freezes sample sizes, fixture hashes, timing boundaries and comparison criteria before performance runs.

## Environment

Use the exact source baseline and capture instrumentation recorded in [C0 results](evidence/c0/result.md). Use `--offline --locked`, two build jobs, and no incremental compilation. Development/test debug information is disabled. Performance comparisons use release builds; developer test elapsed time is not an application benchmark.

On this host the available runtime is `/home/rgame/Documents/fold-native-pipeline/release-package`; its native libraries and configuration are fingerprinted in `evidence/c0/environment.json`. Set both `FOLD_COLOR_ROOT` and `FOLD_TEST_COLOR_ROOT` to that directory. Set `FOLD_VIDEO_ROOT` to the same directory. Do not launch the old packaged application binaries: build Fold from this checkout. Socket and native GPU tests require host access outside the restricted sandbox.

```sh
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export FOLD_COLOR_ROOT=/home/rgame/Documents/fold-native-pipeline/release-package
export FOLD_TEST_COLOR_ROOT="$FOLD_COLOR_ROOT"
export FOLD_VIDEO_ROOT="$FOLD_COLOR_ROOT"
cargo test -p fold-app --offline --locked --features desktop \
  --test project_ingest --test project_placement --test nle_workflow \
  --test common_composition --test agent_project --test agent_handoff \
  --test project_lifecycle --test viewer_transport -- --test-threads=1
```

## Workflow matrix

| Workflow | Executable baseline | Preserved contract |
| --- | --- | --- |
| Media ingest and placement | `project_ingest` and `project_placement` integration tests | Asset-only import, reuse, source metadata, bins, stale/cancel handling, atomic placement, protected/invalid destinations. The two-layer relink fixture is explicitly assigned to C1 re-authoring. |
| Timeline editing and delivery | `nle_workflow::multitrack_nle_edit_mix_render_save_reopen_and_export` | Multitrack video/audio editing, exact timing, mixing, rendered samples, save/reopen and export using generated temporary media. |
| Mixed Composition | `common_composition` and `agent_project` integration tests | Mixed scene/image layers, named connections, keyframes, object scopes, nested clocks, cycle rejection, protection and one undo group per atomic action. |
| Parameters and keyframes | `agent_handoff` parameter-expression/property tests and `common_composition` key tests | Shared native controls, typed samples at rational times, stale revision rejection, preserved keys and native identities. Cross-document expressions are still unavailable. |
| Public proposal/inspection/apply | `agent_handoff` CLI and MCP tests; `agent_project` rebase tests | Same public schema and semantic result; exact retries; fresh inspection after rebase; no bypass of protected data. Capture variables `FOLD_SLICE_CAPTURE_DIR` and `FOLD_M5_HANDOFF_DIR` write transcripts into a fresh temporary directory. |
| Undo/redo and project lifecycle | `project_lifecycle`, `common_composition`, public handoff tests | Dirty-state tracking, stale open/save behavior, transient separation and edit restoration. |
| Opaque persistence | `fold-project` tests and current cutover regressions | Unsupported provider bytes/extensions remain intact, atomic publication, committed-save authority, unknown data preservation. |
| Independent preview clocks | `viewer_transport` and native preview parity test | Per-viewer seek/play/rebind isolation, exact source time, held-frame independence, no project mutations caused by navigation. |
| Native pixels | `motion_integrated` ignored tests; `agent_handoff::cli_and_mcp_return_identical_revision_bound_frames`; `motion_spatial::motion_spatial_preview_worker_matches_direct_output_and_keeps_named_routing` | Existing numerical/pixel assertions, native snapshot-bound rendering, identical public transports and preview/direct agreement. Integrated reference allows at most 0.002 absolute/channel where specified; never broaden tolerances to hide a regression. |
| Grouped native panel routing | `panel_linking::native_grouped_selection_is_stable_and_routes_inspector_network_and_occurrences` with `native-probe` | Camera → Light → grade sequence, normal/narrow native panel layout, stable selection, linked clocks and preserved content. Offscreen automated interaction is distinct from physical artist acceptance. |

Run named native checks explicitly; ignored tests do not count as passes:

```sh
cargo test -p fold-app --features native-probe --offline --locked --test panel_linking -- --test-threads=1
cargo test -p fold-app --features native-probe --offline --locked --test motion_integrated -- --ignored --test-threads=1
cargo test -p fold-app --features native-probe --offline --locked --test agent_handoff cli_and_mcp_return_identical_revision_bound_frames -- --ignored --exact --test-threads=1
cargo test -p fold-app --features native-probe --offline --locked --test motion_spatial motion_spatial_preview_worker_matches_direct_output_and_keeps_named_routing -- --ignored --exact --test-threads=1
```

## Measurements

Build `maintenance_profile` with `--release --features native-probe`. Run it in three sequential fresh processes, retaining stdout JSON and stderr stage events. It uses 31 samples per stage: first invocation separately, then 30 warm samples. It measures Motion structural decode, clone/validate/prepare, evaluation at 0, 1/2 and 2 seconds, pre-cancelled evaluation, Compositor decode/configuration at 16/64/256 nodes, undo+redo including refresh, and real offscreen shell drawing at 1600/580 pixels. No source artwork is written.

Use `scripts/measure_process.py` around each process to record process-tree RSS and scratch usage. Logical project retention is reported separately. GPU preview diagnostics report transfer/allocation counters; RSS is not an allocation count. CPU allocator-call counts remain unmeasured without a dedicated allocator profiler.

For desktop timing, build `fold-desktop --release --features native-probe`, copy a fixture into a new temporary directory, and set `XDG_STATE_HOME` to a new directory. `FOLD_PASSIVE_PROBE=/absolute/new-report.json` captures 15 seconds of existing draw/acquire/poll/controls/preview/submit/present timing, revision and presented content keys. It never seeks or edits, creates no widgets, and is absent from ordinary builds. The diagnostic process exits after capture. It cannot be used to observe an existing unsaved session by restarting that session.

Edit-to-present-call measurement, when run, uses a guarded public edit on the isolated copy: record wall-clock Apply start/completion and new revision, then locate the first native sample with the changed presented content key for the edited output. Report both endpoints. GPU completion callbacks are observation times, and the present call is not compositor scanout or physical input latency. Idle redraw timing alone must never be reported as edit latency.

`python3 docs/maintenance/native_capture.py /tmp/new-c0-capture` reproduces the isolated current-workspace setup and public seed edit; `--desktop target/debug/fold-desktop` selects the debug build. C0's release/debug attempts failed with SIGSEGV, including a runtime-probe-disabled control. Consequently edit-to-present and preview GPU allocation metrics are unavailable, not zero; see the result record before interpreting any native trace. The initial unbound capture's completed report does not mean the process passed.

## Freeze evidence and instrumentation

The [original desktop incident](../rewrite/evidence/m5a-desktop-freeze.md) remains open. The later [selection-routing repair](../rewrite/evidence/m5a-selection-linking/result.md) established a specific Camera/Light/grade feedback loop; it did not establish the earlier process-exit cause. Historical `/tmp` monitor files no longer exist on this host.

`scripts/maintenance_watch.py --pid PID --socket SOCKET --cli target/debug/fold-cli --output /absolute/new.jsonl` samples read-only public discovery, response latency, revision/epoch, process memory/state and thread wait channels. It rejects overwriting evidence and detects PID reuse. It never edits, restarts, signals or terminates Fold. A timeout is a dispatch-stall observation, not a driver or UI diagnosis. Use it alongside the opt-in native probe and stderr worker-completion records when reproducing a failure in an isolated session. For a user-reported live failure, inspect the existing process without discarding unsaved work.

Native logs and evidence are the only output of added instrumentation. No diagnostic labels, indicators, popups, status text or other visible feedback are authorized by C0 or C2.
