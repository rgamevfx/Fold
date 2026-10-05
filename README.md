# Fold

Fold is a modular VFX, editing, and motion graphics application written in Rust,
with a Dear ImGui desktop interface. It is under active development; the current
reference platform is Linux with an NVIDIA GTX 1070. Other platforms and GPUs
are not yet qualified.

## Build and run

Install stable Rust with Cargo, Python 3, and the Linux build dependencies.
On Ubuntu 24.04:

```sh
sudo apt-get update
sudo apt-get install -y build-essential cmake pkg-config libasound2-dev ffmpeg
```

From the repository root, run the headless smoke example:

```sh
cargo run -p fold-app --bin fold-cli --no-default-features --locked
```

Build and launch the desktop in a graphical session with working graphics and
audio drivers:

```sh
cargo run -p fold-app --bin fold-desktop --features desktop --locked
```

The desktop compiles without the private native runtimes. Color-managed
rendering requires the pinned OpenColorIO runtime described in
[color runtime setup](resources/color/README.md). Native CUDA decoding also
requires the pinned video runtime and helper; see
[the native-video implementation notes](docs/phase-18A.md) and
`scripts/build_video_runtime.py`. A plain desktop build does not install these
runtimes. System FFmpeg supplies the process-based media paths.

## Checks

GitHub CI checks formatting, package boundaries, Python tooling, and the Rust
workspace with desktop features. It builds the desktop without opening a window.
To run the same checks locally:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/check_boundaries.py
cargo fmt --all --check
cargo check --workspace --all-targets --features fold-app/desktop --locked
cargo test --workspace --features fold-app/desktop --locked -- --test-threads=1
cargo build -p fold-app --bin fold-desktop --features desktop --locked
```

Tests run serially because ImGui uses shared process state. Tests marked ignored
need hardware, media fixtures, or private runtimes and remain separate local
checks. Avoid `--all-features` for ordinary builds: it also enables the standalone
CUDA helper, which requires a separately prepared native runtime.

During development, run checks for the affected crates; reserve full workspace
checks and Clippy for integration checkpoints.

## Build storage

Cargo stores generated builds in `target/`. Different features and repeated
development builds can accumulate substantial disk usage. Check and clear it
from the repository root with:

```sh
du -sh target
cargo clean
```

Stop active builds before cleaning. This removes generated outputs, including
locally built executables; source files and Git history are preserved. The next
build recompiles dependencies. CI disables incremental builds and debug symbols
to reduce storage use.

## Project documentation

- [Development plan and current status](dev-plan.md)
- [Architecture](docs/architecture.md)
- [Product and architecture proposal](docs/Fold_Application_Product_Architecture.md)
- [Development rules](AGENTS.md)
