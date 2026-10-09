# C0 baseline results — 9 October 2026

**Status: baseline collection complete, with native measurement failures explicitly open.** C0 has established the removal assignments, reproducible workflow suite, frozen comparison profile and available measurements. It has not qualified the desktop, fixed the original freeze, or implemented C1. The C0 gate permits unavailable native checks to be recorded separately; no failed native capture is counted as a pass.

## Scope and provenance

Starting commit: `6280fb628beddf1b72725f2b7849740eaeb146f9`. Implementation and evidence are recorded together in the C0 baseline commit. At entry the only working-tree changes were the new maintenance plan and its link from the Motion plan. [Environment](environment.json) records build tools, CPU, GPU, runtime files and their fingerprints; [capture hashes](capture-hashes.json) identify the instrumented sources and executables. Host: i7-8750H, GTX 1070 8 GiB, NVIDIA 580.173.02/Vulkan, Rust 1.95.0. Native libraries/configuration came from `/home/rgame/Documents/fold-native-pipeline/release-package`; application executables were built from this checkout.

No existing Fold process/session was found at entry. All application mutations ran in generated fixtures or disposable copies. All 15 tracked `.fold` archive hashes still match the inventory. No original artwork or workspace sidecar was changed.

C0 adds read-only inventory and heartbeat scripts, a release benchmark example, and a bounded passive mode in the existing `native-probe` diagnostics. The only UI-crate changes collect revision/content keys and timings behind that opt-in feature. **No widgets, labels, indicators, badges, notifications, popups or other visible feedback were added.** C2's typed failures and capabilities remain internal contracts under the plan's explicit rule.

## Gate evidence

| C0 obligation | Evidence and disposition |
| --- | --- |
| Starting state and obligations | [Environment](environment.json), fixture hashes and source baseline. M3–M5/M5a–M5c remain open; M6 remains gated. |
| Assigned deletion closure | [Semantic inventory](../../c0-inventory.md) plus [machine inventory](inventory.json): 1,403 references, eight assigned groups, 15 parsed archives, zero parse errors. Registration/alias/fixture/shared-consumer/dependency dispositions are explicit; no matched group is unassigned. Lexical matches are not automatic whole-file deletion instructions. |
| Reproducible workflows | [Workflow matrix and commands](../../c0-workflows.md), [test summary](tests-summary.json), preserved CLI/MCP transcripts and native pixel checks below. |
| Measurement boundaries | [Frozen profile](../../c0-profile.json), three fresh release runs, raw samples, RSS and retention below. CPU allocator counts unavailable; native edit-to-present and preview allocation counters blocked by the desktop failures below. These have no fabricated baseline value. |
| Freeze evidence | Original incident remains unresolved. Read-only heartbeat records dispatch latency, revisions, process state, RSS and thread wait channels; native diagnostics record existing draw/completion transitions. New native crashes have their own evidence and no assigned cause. |

## Automated verification

**84 distinct Rust tests passed; 16 Python tests passed.** See [test summary](tests-summary.json) and logs:

- [Workflow suites](workflows-host.log): 43 passed across media import/placement, multitrack Timeline render/export/save/reopen, Composition, public editing, lifecycle and independent viewer transport.
- [Public GPU parity](public-gpu.log): one explicitly invoked native test passed; CLI and MCP produced identical revision-bound frame/crop observations. This resolves the one native test ignored in the first workflow invocation. [Transcript summary](handoff-summary.json) records hashes; identical CLI/MCP state is stored once in `handoff-state.json.gz`, with requests/transport observations in `handoff-transports.json.gz`.
- [Agent jobs/dispatch and grouped panel routing](routing-cancellation.log): 17 passed, including cancellation, stale-job behavior, and Camera → Light → grade at normal/narrow widths. One separate screenshot-capture test remains ignored; it is not a pass or artist acceptance.
- [Project persistence](project-tests.log): 18 passed, including opaque preservation and the compile-fail documentation test.
- [Probe behavior](probe-tests.log): two passed, including passive no-seek/no-format-requirement and existing schedule/cancellation/report safety.
- [Integrated GPU scene/control tests](integrated-gpu.log): two explicitly invoked tests passed. [Preview/direct GPU parity](preview-gpu.log): one explicitly invoked test passed. Existing tolerances were unchanged.
- [Boundary check](boundaries.log) and [16 Python tests](python-tests.log) passed. Final formatting, Python compilation, fixture hashes and `git diff --check` passed; see `final-checks.log`.
- [Focused Clippy](clippy.log) completed successfully for the benchmark and its dependencies. It reported existing warnings in untouched code; no warning was attributed to the new benchmark or passive-probe changes. This is not a warning-free or full-workspace qualification claim.

The initial restricted-sandbox run had five local Unix-socket PermissionDenied failures ([log](workflows.log)); the authorized host run passed. Full workspace tests/Clippy belong to the plan's vertical-slice checkpoints and were not run at C0. Release desktop/example builds passed ([build log](release-build.log)).

## Release CPU and retention baseline

Three sequential processes; each stage has one first invocation and 30 warm samples. The first invocation is not an OS-cache-cold claim. No test/build work was intentionally overlapped with these runs. Raw results: [run 1](profile-1.json), [run 2](profile-2.json), [run 3](profile-3.json); stage timestamps are in corresponding `.log` files. [Summary](profile-summary.json) retains each run separately.

35-node Motion construction, 31,214 payload bytes; evaluate at exact 0, 1/2 and 2 seconds. Synthetic Compositor chains have 16/64/256 nodes at 480×270, 24 fps, 96 frames. Shell drawing uses real panels offscreen at 1600×1000 and 580×1000. Undo+redo measures two refreshes on the mixed-document fixture. It does not measure a rendered edit.

| Stage | Warm median ms, runs 1 / 2 / 3 | Warm p95 ms, runs 1 / 2 / 3 |
| --- | --- | --- |
| motion_decode_structure | 0.2507 / 0.2531 / 0.2539 | 0.3692 / 0.5999 / 0.2780 |
| motion_clone_validate_prepare | 0.5243 / 0.5306 / 0.5258 | 0.7953 / 0.7130 / 0.5575 |
| Motion evaluate t=0 | 12.5951 / 14.3670 / 13.4789 | 14.5852 / 16.2603 / 15.8977 |
| Motion evaluate t=1/2 | 13.0757 / 12.7777 / 13.8961 | 14.5042 / 14.6022 / 15.6889 |
| Motion evaluate t=2 | 13.6876 / 13.6539 / 12.5728 | 15.2882 / 15.4799 / 14.5967 |
| motion_precancelled_evaluation | 0.0002 / 0.0001 / 0.0001 | 0.0003 / 0.0001 / 0.0001 |
| compositor_decode_validate_16 | 0.6127 / 0.6917 / 0.6224 | 0.7741 / 1.2382 / 0.8064 |
| compositor_configurations_16 | 0.0320 / 0.0320 / 0.0322 | 0.0577 / 0.0331 / 0.0333 |
| compositor_decode_validate_64 | 2.5358 / 2.6706 / 2.5186 | 3.0168 / 4.3160 / 3.8189 |
| compositor_configurations_64 | 0.1325 / 0.1357 / 0.1360 | 0.1362 / 0.2401 / 0.1529 |
| compositor_decode_validate_256 | 9.8210 / 10.4324 / 11.5734 | 10.6823 / 12.6583 / 13.8630 |
| compositor_configurations_256 | 0.6041 / 0.5559 / 0.6526 | 0.7805 / 1.2288 / 1.4250 |
| session_undo_redo_with_refresh | 1.3427 / 1.4829 / 1.3726 | 1.6360 / 2.3572 / 1.9763 |
| offscreen_shell_frame_1600 | 0.6780 / 0.7029 / 0.6896 | 0.6927 / 0.8238 / 1.1500 |
| offscreen_shell_frame_580 | 0.6776 / 0.6720 / 0.6809 | 0.7392 / 0.7842 / 1.1652 |

Sampled process-tree RSS peaks: 46,931,968 / 46,850,048 / 46,571,520 bytes (about 44.4–44.8 MiB). See `profile-{1,2,3}-rss.json`; 50 ms sampling can miss brief peaks and RSS can double-count shared pages. Project retention after the measured edit: two roots, one external snapshot handle, 43,748 logical payload bytes. This excludes allocator overhead and is not a long-running leak test. No CPU allocator-call counts were collected.

For future comparisons use the same frozen profile/hardware/configuration, preserve current semantic and pixel tolerances, and investigate a >20% warm median or p95 regression in two of three runs. This is an investigation threshold, not a product latency guarantee. Preparation/evaluation timings identify candidates for later work; C0 makes no optimization claim.

## Native failures and unavailable measurements

| Capture | Observation | Disposition |
| --- | --- | --- |
| [Initial release capture](native/run.json) | Public seed proposal/inspection/apply succeeded on a disposable copy, revision advanced to 6. Trace recorded 850 redraws over 15 seconds, then process exited with signal 11. Default workspace had no bound viewer; every presented-key list was empty. | Preserve trace and heartbeat as failed-run evidence. `native.json` reporting `success: true` means the timed probe finished writing, **not** that process shutdown or rendered presentation passed. No edit-to-present value can be derived. |
| [Bound-viewer release](native-bound/run.json) | Explicit current workspace v4 pinned the fixture's main output. Public connection reset, process exited signal 11; report remained empty. | Native capture failed before usable presentation evidence. |
| [Bound-viewer debug](native-debug/run.json) | Same fixture/workspace; connection reset and signal 11. | Failure is not limited to the release build. |
| [Runtime probe disabled control](native-no-probe/run.json) | Same release executable and bound workspace with `FOLD_PASSIVE_PROBE` removed before launch. Connection reset and signal 11. | Failure also occurs without the new probe active. No causal attribution to a provider, driver, workspace restoration or original incident is established. |

The driver's cleanup flag records that supervisor cleanup was attempted after a request failure; the observed child result is SIGSEGV (-11), not a successful controlled shutdown. Every run confirms the original fixture was unchanged. The control used the same driver with the environment variable removed in the subprocess launch; it was not a different renderer.

The first failed run's process-tree RSS peaked at 457,498,624 bytes (~436.3 MiB) and NVIDIA process-memory sampling at 134,217,728 bytes (128 MiB): [report](native-rss.json). These include driver/UI startup and supervisor/CLI overhead, and **do not describe a working preview**. GPU process memory is not an allocation count. Because there were no presented outputs or preview records, native edit-to-present latency and preview GPU allocation/transfer baselines remain unavailable. Do not compare future successful rendered sessions against this unbound trace.

Reproduce the bound failure with the release `native-probe` desktop and debug CLI built, host display/GPU access, then:

```sh
python3 docs/maintenance/native_capture.py /tmp/new-c0-capture
```

Use `--desktop target/debug/fold-desktop` for the debug case. The driver installs current navigation state only in its disposable state directory and uses public discover → inspect → propose → inspect candidate → apply for the seed edit. Original artwork is never saved over. Request stderr, process exit, hashes and heartbeat are preserved even on failure.

**Follow-up owners:** desktop/UI-render lifecycle investigation before claiming C6 native acceptance; native timing/allocation baseline must be rerun successfully before any C8 edit-latency or GPU-allocation improvement claim. The earlier [desktop freeze](../../../rewrite/evidence/m5a-desktop-freeze.md) stays open and distinct from the later fixed [selection feedback loop](../../../rewrite/evidence/m5a-selection-linking/result.md). No relationship between these incidents is proven. No physical artist acceptance was performed.

## Next phase

C1 is planned, not started: remove superseded document/operation generations and re-author supported fixtures against the captured contract. C0 deleted no runtime implementation or dependency and retained no new compatibility shim. The native failures and unavailable metrics travel with the baseline; later phases must not erase them from acceptance records.
