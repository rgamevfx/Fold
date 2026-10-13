# Development

Commands run from the repository root. This is the Linux reference setup;
packaging and GPU checks have additional requirements beyond a Cargo build.

## Build and run

Use stable Rust, Python 3, FFmpeg and a Vulkan graphics driver. On Ubuntu:

```sh
sudo apt-get install -y build-essential cmake ninja-build pkg-config libasound2-dev libvulkan-dev git ffmpeg python3
cargo build -p fold-app --bin fold-desktop --release --features desktop --locked
```

The executable build alone does not install the color/RIFE runtimes or Motion SDK.
Color-managed rendering needs the private runtime. Build it with the exact
compiler/CMake versions in [toolchain.json](../resources/color/toolchain.json):

```sh
python3 scripts/build_color_runtime.py /absolute/build-dir /absolute/new-color-package
```

Retime's RIFE mode needs the pinned ncnn/Vulkan runtime and v4.6 model. Its producer
requires Vulkan headers, CMake, Ninja and Git; it fetches pinned upstream sources:

```sh
python3 scripts/build_rife_runtime.py --work /absolute/rife-build --prefix /absolute/rife-package
```

Set `FOLD_RIFE_ROOT=/absolute/rife-package` for checkout builds. The package
producer includes this runtime automatically. Missing/incompatible runtime or
model files produce an explicit error when RIFE is used; Nearest and Frame Blend
remain independent. The native adapter currently supports Linux Vulkan with
external-memory FD sharing; no Python or CUDA inference installation is needed.

For Motion rendering, supply a **precompiled** SDK matching the current source.
To stage one, obtain the native archives matching the locked Motion dependencies
and a Rust sysroot containing `bin/{cargo,rustc}`, libraries and redistribution
notices. Skia must include Fold's [precision patch](../resources/motion-context/Skia_Precision_Build.md).
The fframes FFmpeg archive is a separate input from the optional native-video
runtime. Keep the archives' original `skia-binaries-*.tar.gz` and `ffmpeg-*.tar.gz`
filenames. Cargo's locked dependencies must already be fetched for offline staging.

```sh
python3 scripts/stage_motion_sdk.py /absolute/new-sdk \
  --vendor --precompile \
  --skia /absolute/skia-binaries-MATCHING-KEY.tar.gz \
  --ffmpeg /absolute/ffmpeg-MATCHING-KEY.tar.gz \
  --toolchain /absolute/rust-sysroot
```

These paths are placeholders for real inputs, not downloadable artifact names.
See each producer's `--help` for its current interface. Source-only SDK staging
is not sufficient for runtime rendering. Use a new absolute destination.

Launch the built desktop with the prepared runtime and SDK:

```sh
FOLD_COLOR_ROOT=/absolute/new-color-package \
FOLD_RIFE_ROOT=/absolute/rife-package \
FOLD_MOTION_BRIDGE=/absolute/new-sdk/crates/motion-fframes \
  target/release/fold-desktop --protocol-dir /tmp/fold-review
```

Append `--project /absolute/project.fold` after `--protocol-dir` to open a project.
The app defaults to a user-cache SDK unless compiled with an installed relative
SDK path; it does not compile a missing SDK from the checkout. Rebuild the SDK
when its dependency inputs change. Save live artwork through the public protocol
before restarting a review instance.

## Package

The complete producer bundles desktop, CLI, color and RIFE runtimes/model, Motion SDK and native
video helper. It also needs CUDA development headers; native decode needs a
compatible NVIDIA driver. See [video requirements/notices](../resources/licenses/video-runtime.md).

```sh
python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package-dir \
  --motion-skia /absolute/skia-binaries-MATCHING-KEY.tar.gz \
  --motion-ffmpeg /absolute/ffmpeg-MATCHING-KEY.tar.gz \
  --motion-toolchain /absolute/rust-sysroot
/absolute/new-package-dir/bin/fold-desktop
```

Use absolute paths and a package directory that does not exist. Packaged runtime
paths resolve relative to the executable. Do not globally add the package's
minimal FFmpeg libraries to `LD_LIBRARY_PATH`; they can override system FFmpeg.
Relocation, GPU and artist acceptance require separate checks; a producer pass
alone is not package acceptance.

## Validation

During iteration, format touched files and run the affected crate/check or
regression. Use optimized builds for artist review. For integration checkpoints,
[CI](../.github/workflows/ci.yml) owns the command matrix, including:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/check_boundaries.py
python3 scripts/check_proposal_ownership.py
cargo fmt --all --check
cargo check --workspace --all-targets --features fold-app/desktop --locked
cargo clippy --workspace --all-targets --features fold-app/desktop --locked
cargo test --workspace --features fold-app/desktop --locked -- --test-threads=1
```

ImGui tests share process-global state: keep them serial. `--all-features` also
enables the CUDA helper and is not the ordinary CI configuration. The isolated
Motion worker, installed package, ignored GPU tests and `native-probe` regressions
need their own relevant checks; the default workspace run does not qualify them.
For the seek-during-key-drag regression:

```sh
cargo test -p fold-app --features native-probe --test composition_gesture --locked -- --test-threads=1
```

Historical results and remaining native gates are in [current work](current-work.md).
