# Phase 16 — production ACES/OCIO semantics

## Status and scope

**CPU/application color acceptance passed.** Desktop and CLI now use the same
explicit scene/input/output semantics. This is not GPU evaluation, decoder
replacement, real-time throughput acceptance, or completion of phases 17–21.

The user's UI refinement governs this implementation: new projects work in
ACEScg, viewers use sRGB / ACES 1.0 SDR Video, and normal color interaction is one
input dropdown and one output dropdown. There are no added viewer-toolbar color
controls or routine explanatory/status rows. The viewer policy is fixed, not a
project delivery setting; there is currently no editable viewer color choice to
persist in workspace state. If such choices are added later, they must remain
workspace-only. Display/view/look combinations are available in the output
picker; advanced external-config replacement is an explicit CLI operation.

## Implemented contracts

### Persistence, compatibility, and identity

- New desktop/CLI projects receive versioned `fold.color` settings with ACEScg
  working space and the pinned bundled configuration. Initial settings do not
  create an undo entry. An absent contract means legacy linear-sRGB, never ACES.
- Archive version 3 prevents older applications from silently interpreting new
  ACES projects as sRGB. V1/V2 archives remain readable; their settings and
  provider payloads are retained, not automatically color-converted. Unknown
  project/provider/extension data remains preserved by project persistence.
- Asset `fold.color.input` assignments and per-compositor-Read overrides are
  undoable; a Read override takes precedence. Delivery `fold.color.output` is
  persisted on the selected output document, independently of viewer policy.
- Config identity includes runtime/bridge binary hashes and config/LUT content
  hashes, not just names or paths. Input and config changes enter content keys;
  delivery-only changes do not invalidate viewer scene content.
- External configuration replacement is explicit, undoable, validates required
  application transforms, and refuses automatic conversion of legacy projects.
  Resources are pinned for each worker; a new worker rejects changed resources
  rather than silently adopting them.

### Native service and packaging

- `fold-color` owns the private, exception-safe OCIO C ABI. Unsafe Rust is confined
  to its native adapter. Handles retain their library and copy native strings;
  configs/processors are deliberately worker-local, not `Send`/`Sync`.
- OCIO **2.4.2**, Studio **2.2.0**, ACES **1.3** are pinned. Source/config SHA-256,
  native compiler and CMake versions are checked by the build script. Licenses,
  dependency notices, toolchain record, and resource hashes accompany the package.
- Runtime lookup is executable-relative, independent of CWD and `$OCIO`.
  `FOLD_COLOR_ROOT` is an explicit development override, not an implicit fallback.
- External resources are copied to owned temporary storage before native loading:
  at most 64 MiB, 4096 entries, and 32 levels. Symlinks, declared environment
  variables, and used LUTs outside the pinned package are rejected. Application
  default transforms are validated on replacement; other processors are validated
  when requested. This is not exhaustive evaluation of every possible external
  config transform, nor support for arbitrary dynamic environment-based configs.
- `scripts/package_fold.py` builds locked release desktop/CLI binaries and the
  relocatable color package. Its manifest records binary/source hashes, revision,
  dirty state, and Rust compiler version. This is a Linux reference-platform
  package, not a hermetic OS image or cross-platform installer.

### Evaluation, authored colors, and delivery

- Scene requests contain explicit document/output, exact time and dimensions,
  rather than a viewer key. Scene results retain alpha; preview and export apply
  their own explicit transforms and matte afterward.
- New video evaluation reconstructs float BT.709 signal RGB from the supported
  decoder profile without first converting it to sRGB. The OCIO input assignment
  is applied once. Default video input is `Camera Rec.709`; encoded still fixtures
  default to sRGB. Native metadata remains retained and input assignment is
  overridable. The decode cache distinguishes signal-float and legacy RGB8 data.
- CPU graph operations retain finite signed/above-one ACEScg RGB. Compositor
  solids/parameters and motion fills/strokes use working-space values. Vector
  rasterization produces coverage, then composites float authored color/alpha;
  coverage is still 8-bit, but working RGB is not quantized through that buffer.
- Shared SDK color pickers display sRGB and convert explicitly to/from working
  color. Working numeric values are available in the context popup. Merely
  displaying an HDR/negative color does not rewrite it, and changing alpha alone
  does not clip its working RGB. Legacy rendering restrictions remain explicit.
- Nonlinear processing unpremultiplies and repremultiplies. OCIO receives an RGB
  descriptor with RGBA stride so coverage is not processed at all. This fixes
  OCIO's optimized exponent path perturbing nominal identity alpha. Zero-alpha
  RGB is canonicalized; invalid/nonfinite results fail instead of publishing.
- Frame descriptors cover dimensions, pixel aspect, component precision,
  planes/strides, alpha/color identity, exact timing, storage and readiness.
  Implemented CPU frames own their pixels. Future GPU/external lease identifiers
  are descriptive only, not resource-access authority; actual GPU ownership and
  synchronization remain phase 17.
- Output conversion requires a matching config/working-space processor and an
  explicit opaque matte. Legacy `to_display`/PPM reject ACES frames. Preview
  workers load/process off the UI thread and retain existing generation,
  independent-viewer, pacing and held-image admission rules.
- Delivery defaults to Rec.1886 Rec.709 / ACES SDR. Its encoded signal enters
  the encoder without a second sRGB transfer; the sRGB output option uses the
  existing sRGB-to-video path. Unsupported MP4 displays fail in preflight.
  Headless image output can use other valid configured transforms.

## UI review

- Read/clip input and Output/delivery settings use shared one-row dropdowns.
  Common input spaces and Rec.709/sRGB output presets come first. Input choices
  exclude display-reference color spaces. Missing runtime/config/interpretation
  errors remain visible; legacy interpretation is explicitly labelled.
- Actual ImGui tests exercise normal/narrow layouts, keyboard selection, and
  non-mutating authored-color display. The full existing shell/editor tests pass.
- Native packaged desktop review on COSMIC verified the rendered composition,
  Read input dropdown, and Output-only inspector. The final package also rendered
  from `/` with `$OCIO` invalid and `FOLD_COLOR_ROOT` unset. Later libei input
  attempts timed out before selection; screenshots were inspected before retry.
  No claim is made that those attempts changed a transform or tested native undo.
  Transactional input/output undo and live preview-worker routing are covered by
  integration tests. Native DPI/accessibility coverage was not expanded here.

## Verification

Artifacts and logs are outside the repository at
`/home/rgame/Documents/fold-phase-16/`:

- `package-acceptance/`: native runtime used by final tests.
- `release/`: latest packaged desktop and CLI.
- `acceptance-tests.log`, `clippy-final.log`, `release-build.log`.
- `screenshots/`: native review captures, including Read dropdown at
  `16-49-00`, Output inspector at `16-54-25`, and final package at `17-16-02`.

Final checkpoint:

```sh
export PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig
export FOLD_COLOR_ROOT=/home/rgame/Documents/fold-phase-16/package-acceptance
export FOLD_TEST_COLOR_ROOT="$FOLD_COLOR_ROOT"
cargo test --workspace --all-features --locked -- --include-ignored --test-threads=1
cargo clippy --workspace --all-targets --all-features --locked
cargo fmt --all --check
python3 scripts/check_boundaries.py
python3 -m unittest discover -s scripts -p 'test_*.py'
git diff --check
```

- **196 Rust tests passed, 0 failed, 0 ignored**, including explicit native OCIO,
  actual preview-worker routing, CLI references, native GPU presentation lifetime,
  and native audio tests. **12 Python tests passed**. Boundaries, formatting, and
  whitespace checks passed.
- Clippy completed successfully with 19 existing warnings in platform/UI tests,
  compositor package code, and motion code. No warning-clean workspace claim is
  made; unrelated warnings were not swept into this change.
- ACES neutral 0.18 reference is
  `[0.3559523, 0.35595256, 0.3559525, 1]`, independently confirmed with phase-11
  Python OCIO 2.4.2. Tolerance: 0.0002 for neutral display reference, 0.00001 for
  half-alpha/input-primary comparisons. Python is not a runtime dependency.
- Matching preview/CLI output agrees byte-for-byte before encoding. The native
  preview worker agrees with the same explicit transform. Encoded Rec.709 signal
  roundtrip is checked within 0.025 per channel, accounting for YUV/codec loss.
- Fixtures cover ACES defaults, legacy save/reopen and appearance, unknown data,
  undo/redo, Read overrides, HDR/negative values, float vector color/alpha,
  config/LUT pinning, missing resources, and output/viewer-key independence.
- The complete release package was copied to a fresh temporary installation.
  Its CLI checkpoint and rerender ran from `/` with invalid `$OCIO` and no root
  override, produced identical PPMs and v3 ACEScg archives. Corrupt/missing config
  resources failed with no completed output. Desktop launch from `/` also passed.
  These are same-host relocation/installation checks, not a second clean-machine
  or VM certification. System platform/audio/graphics and FFmpeg prerequisites
  remain required; system OCIO and Python do not.
- Initial ALSA build discovery, OCIO alpha drift, a picker context-popup assertion,
  and a runtime-dependent legacy test fixture were found and fixed during
  iteration. No introduced failing check remains known.

## Deliberate later-phase boundaries

CPU evaluation remains the reference; no GPU evaluation, hardware decode/encode,
zero-copy, real-time throughput, or ten-minute workload claim is made. The
retained FFmpeg batch/scratch decoder and its supported CFR/SDR profile remain;
float reconstruction does not invent precision absent from the source. General
external-config portability, additional delivery profiles, broader installation
certification, scheduling/resource-performance work, and later integration
hardening retain their respective phase-17–21 gates.
