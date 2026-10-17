# Development

Commands run from the repository root. This is the Linux reference setup;
packaging and GPU checks have additional requirements beyond a Cargo build.

## Build and run

The reference build platform is **Ubuntu 24.04, Linux x86_64**. Use stable Rust
and Python 3.12 or newer. Install build tools once:

```sh
sudo apt-get install -y build-essential cmake ninja-build pkg-config clang libclang-dev patch git python3 libasound2-dev libvulkan-dev libfontconfig1-dev libfreetype6-dev libegl1-mesa-dev libgl1-mesa-dev ffmpeg
cargo build -j 2 -p fold-app --bin fold-desktop --release --features desktop --locked
target/release/fold-desktop
```

Cargo prepares **OCIO, Motion and RIFE automatically**. No runtime paths or
archive filenames need to be supplied. The first build downloads pinned inputs
and compiles the native dependencies, including Fold's Skia precision patch.
It can take substantially longer than a Rust-only build. Later builds verify and
reuse native caches. Motion/RIFE share `target/fold-runtime-cache` across Cargo
profiles; OCIO uses the profile's `fold-color` cache. Custom Cargo target
directories are supported. Cross-compilation is not supported yet.

OCIO stays pinned to 2.4.2. Its yaml-cpp 0.7.0 source receives the upstream
[`<cstdint>` fix](https://github.com/jbeder/yaml-cpp/pull/1310) automatically,
so newer GCC versions do not depend on accidental transitive header inclusion.

Motion setup obtains the checksummed Skia source/reference and FFmpeg archives
listed in [runtime-inputs.json](../resources/motion-context/runtime-inputs.json),
vendors the locked Rust graph and stages a matching compiler, precompiled worker,
script libraries, Rust linker and native link inputs. Installed scripts compile
with the bundled tools; they do not invoke Cargo or a system C/C++ linker.
The build machine needs development libraries; installed machines need their
runtime libraries and graphics driver. SDK source or compiler changes invalidate
the cache. Failed producers never publish an incomplete runtime.

RIFE setup fetches pinned RIFE/ncnn/glslang revisions and verifies the v4.6 model
hashes. Inference requires a Vulkan device supporting Linux external-memory FD
sharing. It does not require Python or CUDA at runtime. Unsupported GPU sharing
produces an explicit error; it never silently changes RIFE into frame blending.
GPU-driver compatibility and numerical agreement across vendors need their own
acceptance tests. Building on a newer OS does not establish compatibility with
an older glibc; distribute packages built on the reference OS.

`FOLD_COLOR_ROOT`, `FOLD_RIFE_ROOT` and `FOLD_MOTION_BRIDGE` remain optional
specialist overrides. Normal checkout and installed launches do not need them.
Motion always uses its SDK's matching Rust compiler, not a caller's `RUSTC`.
The standalone producers remain available for native-development work; see their
`--help`. Append `--protocol-dir /tmp/fold-review --project /absolute/project.fold`
to the desktop command to review a project through the public protocol.

## Package

The producer uses the same automatic runtime caches as Cargo. It bundles desktop,
CLI, OCIO, RIFE/model, the Motion SDK and a runtime acceptance executable. The
complete package also includes the optional native-video service: building that
service needs CUDA development headers, and using native decode needs a compatible
NVIDIA driver. See [video requirements/notices](../resources/licenses/video-runtime.md).

```sh
python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package-dir
/absolute/new-package-dir/bin/fold-desktop
```

Use absolute paths outside the checkout and a package directory that does not
exist. Copy the complete package when installing. Runtime paths resolve relative
to the executable;
relocated installations cannot fall back to the original checkout's caches.
Do not globally add package FFmpeg libraries to `LD_LIBRARY_PATH`.

## Isolated runtime acceptance

Build/package success and rendered pixels are separate evidence. On a supported
Vulkan machine with `bubblewrap` installed, run:

```sh
python3 scripts/verify_runtime_package.py /absolute/package-dir
```

This mounts the package at a new path, hides home directories, checkout/build
caches and system compiler tools, clears environment overrides and disables the
network. It starts with an empty user cache. The bundled `fold-runtime-check`
validates ACES, renders native Motion, compiles a unique animation source and
checks its rendered movement, then checks fractional RIFE motion/HDR and GPU
tensor transport. A missing runtime, required link file or unsupported GPU fails
the command. This isolates software dependencies on the test host; qualification
on another GPU/vendor and native artist interaction remain separate gates.

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
