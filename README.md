# Fold

Fold is an early-stage VFX, editing, and motion graphics application built with
Rust and Dear ImGui.

## Build and run

Requires stable Rust. On Ubuntu, install the build dependencies and launch:

```sh
sudo apt-get install -y build-essential cmake pkg-config libasound2-dev ffmpeg
cargo run -p fold-app --bin fold-desktop --features desktop --locked
```

Color-managed rendering requires the [OpenColorIO runtime](resources/color/README.md).
See the [native video notes](docs/phase-18A.md) for CUDA decoding setup.

## Documentation

- [Development plan](dev-plan.md)
- [Architecture](docs/architecture.md)
- [Development rules](AGENTS.md)
