# C8 — Measured local preparation optimization

**Status: complete for the measured local preparation scope.**

Base: C7 `c6cd6bf`; initial `git status --short` was empty. Reused the frozen
[C0 profile](../../c0-profile.json), [C0 measurements](../c0/result.md),
[C7 qualification and exception](../c7/result.md), and C6's causal desktop fix.

## Measurement and selection

Built the current C7 runtime with the existing release `maintenance_profile`
example, adding separate animation-validation and node-preparation stages before
changing production code. [Environment/provenance](environment.json),
[before build](build-before.log), `before-{1,2,3}.json` and corresponding logs
retain the measurements. Each process has one first invocation and 30 warm
samples; three processes run serially without overlapping builds/tests. First
invocation is not OS-cache-cold. The runner is [measure.py](measure.py).

Ranked current costs (warm median ranges across the three before processes):

1. Motion evaluation: 12.52–14.01 ms. Largest overall, but full incremental
   dependency/field execution belongs to gated M6; unchanged.
2. Compositor decode/validation, 256 nodes: 3.26–4.04 ms. Its preparation component
   takes 1.93–2.15 ms; node preparation takes 1.57–1.77 ms. Animation path validation
   alone costs 0.88–0.99 ms despite these nodes having no tracks. These stages
   overlap and must not be summed. This is the largest isolated avoidable cost.
3. Session undo+redo, including two refreshes: 1.105–1.122 ms.
4. Offscreen shell frame: 0.659–0.669 ms. Not physical presentation.
5. Motion clone/validate/prepare: 0.523–0.524 ms; decode: 0.248–0.256 ms.

C0's 256-node decode/validation was 9.82–11.57 ms and its Session refresh pair
1.34–1.48 ms. Those historical differences are **not C8 gains**; C1 re-authored the mixed
fixture and C3 changed preparation. The current
benchmark's `compositor_prepare` stage replaced C0's configuration-only stage
in C3 and is not directly comparable to that historical stage. Current before/
after stages use the same code and workload. No rendered edit-to-present baseline
is inferred from CPU or offscreen timings.

## Implementation and scope

Prepared Compositor nodes with zero decoded animation tracks return immediately
from animation-path validation. Previously they allocated the whole property/
channel lookup table and then checked zero paths. Track decoding, structural and
identity validation still precede this check; nodes with tracks retain the exact
existing path validation. No evaluated values, IDs, time, color, alpha, errors or
correctness tolerances change.

This is intentionally a local optimization with **zero new retained cache bytes**.
No cache, cache key, worker, provider contract, UI, dependency or SDK is added.
Prepared documents remain immutable borrows scoped to their native document;
there is no retained result shared across content, provider implementations,
invocation controls, presentation edits, semantic/source edits, times, dimensions
or cameras. Separate same-revision transient roots still prepare independently.
Existing C5 metadata remains the sole Session metadata cache; a new regression
checks its two-root eviction, release on drop, and release of an opaque 1 MiB
document after actual Session project replacement. The existing same-revision
transient/undo test is rerun. There is no duplicate prepared-result cache requiring
an additional byte budget or eviction policy.

No synchronous refresh worker migration is justified by this bounded local
change: the measured refresh pair is about 1.11 ms. Further Motion compilation,
source/layout/topology/field/time caches and presentation-versus-semantic
invalidation remain M6 work, gated by M5/M5a/M5b/M5c acceptance. A possible JSON
copy in operation decoding was inspected but not changed without isolated timing.
C4 provider ownership, C5 Session authority, C6 panel ownership and C7 GPU
completion/admission/resource ownership are unchanged.

## Verification and acceptance

[Comparison including first invocation, medians, p95 and RSS](comparison.json)
retains each run separately. Raw after results are `after-{1,2,3}.json` with
stage/time logs; binary fingerprints are in `before-binary.json` and
`after-binary.json`. All figures below are milliseconds, runs 1 / 2 / 3.

| Stage | Before median | After median | Before p95 | After p95 |
| --- | --- | --- | --- | --- |
| compositor_decode_validate_16 | 0.2208 / 0.2198 / 0.2307 | 0.1532 / 0.1560 / 0.1537 | 0.4725 / 0.3135 / 0.3500 | 0.2830 / 0.1917 / 0.2813 |
| compositor_decode_validate_64 | 0.8916 / 0.8480 / 0.9272 | 0.5957 / 0.6275 / 0.6345 | 1.5747 / 0.9644 / 1.4391 | 1.1453 / 0.6603 / 1.1777 |
| compositor_decode_validate_256 | 3.6013 / 3.2563 / 4.0352 | 2.4965 / 2.3727 / 2.6167 | 5.0324 / 3.3828 / 5.0269 | 3.9246 / 3.1707 / 3.9451 |
| compositor_prepare_256 | 1.9837 / 1.9346 / 2.1543 | 0.9260 / 0.9090 / 0.9720 | 3.1071 / 2.1405 / 3.0825 | 1.8921 / 0.9807 / 1.6354 |
| compositor_animation_validation_256 | 0.9696 / 0.8795 / 0.9923 | 0.0006 / 0.0006 / 0.0010 | 1.9983 / 0.9213 / 3.0152 | 0.0006 / 0.0006 / 0.0010 |
| session_undo_redo_with_refresh | 1.1218 / 1.1052 / 1.1144 | 1.1045 / 1.0982 / 1.1435 | 1.2416 / 1.5084 / 1.1853 | 1.4623 / 1.1443 / 1.4570 |
| offscreen_shell_frame_1600 | 0.6674 / 0.6651 / 0.6693 | 0.6907 / 0.6828 / 0.7205 | 0.8565 / 1.1558 / 0.7116 | 1.0725 / 0.7142 / 1.1794 |
| offscreen_shell_frame_580 | 0.6593 / 0.6672 / 0.6666 | 0.6623 / 0.6750 / 0.6971 | 0.7334 / 0.9668 / 0.7193 | 1.0790 / 0.9677 / 0.9220 |

The 256-node decode/validation median improves **27–35%** and preparation
**53–55%** in all three runs; both p95 stages improve in all three. Animation
validation now only checks that the track collection is empty (sub-microsecond to
~1 microsecond per 256-node sweep here); this is not whole-graph evaluation cost.
No refresh, Motion evaluation, native latency or general throughput gain is
claimed. First-invocation samples are retained without claiming OS-cache-cold
performance.

Both offscreen shell widths initially crossed C0's >20% p95 regression threshold
in two of three paired runs, while median changes stayed below 8%. Investigation:
reran three serial processes with the **identical after binary**, retaining all
original samples (`after-repeat-*`, including binary hash). The two-of-three p95
threshold did not repeat for either width. Repeat medians are 0.660–0.674 ms;
1600-wide p95 is 0.7065 / 1.1633 / 0.8823 ms and 580-wide p95 is
0.6804 / 0.8035 / 1.0482 ms. This demonstrates run variability, not a shell latency
improvement or a proven cause for every spike. The targeted preparation gain
persists. No other initial stage crossed the two-of-three p95 threshold.

Maximum process RSS (GNU time, KiB): before 45,856 / 45,656 / 45,780; after
46,200 / 45,296 / 46,060. These overlap; no memory reduction is claimed. Unlike
C0's 50 ms process-tree sampling, this uses process high-water RSS and must not
be interpreted as an allocation count or a precise historical RSS comparison.
Logical fixture retention is unchanged at two roots, one external handle and
43,748 payload bytes in every run. No new persistent allocation was introduced.

Automated verification:

- [Affected Compositor/App all-target check](check.log), including native-probe:
  passed. [Release build](build-after.log): passed; refreshed after the final test
  edit so the final executable source fingerprint includes that edit.
- [Compositor suite](compositor-tests.log): **43 passed**, zero failures/ignored.
  Includes prepared-record validation, all operation defaults, unknown animation
  paths, malformed tracks, protected controls, invocation shadowing, exact-time
  sampling, immutable snapshots, render semantics and serialization/undo.
- [Metadata regressions](metadata-tests.log): **2 passed**, including same-revision
  transient roots and actual eviction/project-replacement release.
- [Session regressions](session-tests.log): **10 passed**, including transient
  previews, cancel/commit/undo, worker cancellation and project-open behavior.
- [Package boundaries](boundaries.log), [provider ownership](ownership.log),
  touched-file rustfmt and `git diff --check`: passed. No full workspace or Clippy
  rerun: C9 owns the next maintenance-wide checkpoint and this is no Motion
  vertical-slice completion.

Reproduce checks with `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0
CARGO_INCREMENTAL=0`, `cargo ... --offline --locked`:
`check -p fold-compositor -p fold-app --all-targets --features fold-app/native-probe`;
`test -p fold-compositor`; `test -p fold-app --lib preview_metadata`;
`test -p fold-app --test compositor_session`. Build/run the profile with the
command in environment.json followed by `python3 docs/maintenance/evidence/c8/measure.py after`.


No new native GPU or physical artist acceptance is claimed. The changed code
constructs CPU validation metadata and does not touch GPU submission, rendering
kernels, cancellation or admission; C7 hardware evidence is reused at its recorded
scope. The inherited extrusion-depth test remains **failed/open**, with its
unchanged tolerance. C7 remains complete by user acceptance of that exception.
C6's empty-layout restore fix remains verified; the original unknown-trigger
freeze, separate C0 unbound shutdown SIGSEGV and physical artist acceptance remain
open. No C9 or M6 implementation or Motion gate closure is claimed.

## Final review

Reviewed the complete diff: one five-line production guard; two timing stages
in the existing benchmark; one resource-lifetime regression; evidence and phase
records. No duplicate cache, alternate evaluator, unrelated refactor or public
contract change. The Fold authoring skill, discovery, clients and schemas require
no update because authoring behavior/contracts are unchanged.
