# Fold

Fold is an early-stage VFX, editing, and motion graphics application built with
Rust and Dear ImGui.

Native Motion editing is in development: the desktop includes initial controls,
graph wiring, group/notes controls, source editing/rebasing and scalar key editing. Linux builds expose the replacement agent protocol through opt-in local sockets, CLI
and MCP; see the [public workflow](skills/fold/references/public-workflow.md). Motion
rendering and the complete artist/agent handoff remain unqualified. Legacy Motion documents are preserved as unsupported
data. See the
[rewrite plan](docs/Motion_System_Rewrite_Plan.md) for current phase evidence.

## Build and run

Requires stable Rust. On Ubuntu, install the build dependencies and launch:

```sh
sudo apt-get install -y build-essential cmake pkg-config libasound2-dev ffmpeg
cargo run -p fold-app --bin fold-desktop --features desktop --locked
```

Color-managed rendering requires the [OpenColorIO runtime](resources/color/README.md).
See the [native video notes](docs/phase-18A.md) for CUDA decoding setup.

## Documentation

- [Motion System Rewrite Plan](docs/Motion_System_Rewrite_Plan.md) — active roadmap
- [Retired documentation](docs/Retired_Documentation.md) — historical plans and evidence
- [Architecture](docs/architecture.md)
- [Development rules](AGENTS.md)
