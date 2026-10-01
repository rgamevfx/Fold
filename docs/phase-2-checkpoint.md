# Foundation slice checkpoint

## Goal

Close product gate A with an inspectable headless generated image, using committed
snapshots, a statically registered motion provider, and a feature-neutral CPU renderer.

## Tasks

- Add a bounded CPU solid-color render operation with explicit linear-sRGB,
  premultiplied RGBA ownership and an opaque sRGB PPM output path.
- Add a motion-owned solid document and a small static video-provider registry.
  Unknown providers, schemas, and ports fail explicitly; preservation stays in project.
- Expose CLI checkpoint and render commands. Exercise create/edit/undo/save/reopen
  and snapshot isolation in integration tests.
- Run workspace checks, tests, Clippy, and boundary checks at this slice checkpoint.

## Acceptance criteria

- The CLI creates and reopens a project and writes a pixel-verified generated image
  without UI dependencies. Editing changes pixels; undo restores them.
- A retained snapshot renders unchanged after edits. Reopened output matches exactly.
- Unsupported documents/ports and invalid or oversized render requests fail clearly.
- Workspace checks and full tests pass; verification and deferred work are recorded.

## Scope

One static opaque solid source, CPU full-frame execution, and PPM image delivery.
No graph, GPU, media decoding, scheduling, nested source compilation, or UI yet.
PPM accepts only opaque frames rather than silently discarding alpha. I/O and render
are synchronous in the headless process; a future UI must dispatch them to workers.
The capability currently returns only a solid operation, not a speculative graph.
The CLI selects the sole document's `video` port at exact time zero and 64×64.
CPU frames have fixed documented color/layout metadata; timed media frames and
additional output formats remain media-slice work. No cross-platform, GPU,
performance, or crash-durability claims are made.

## Verification

Passed locally on Linux:

- Focused `cargo test -p fold-app -p fold-render`: 3 new tests passed, including
  pixel-verified CLI checkpoint and rerender without desktop features.
- `cargo check --workspace --all-targets --all-features --locked`.
- `cargo test --workspace --all-features --locked`: all 16 tests and doc tests passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- `python3 scripts/check_boundaries.py`: package boundaries/headless isolation passed.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: all 8 tests passed.
- `cargo fmt --all --check` and `git diff --check`.

Tests verify the exact sRGB bytes `[0, 188, 255]` for linear `[0, 0.5, 1]`,
edit/undo and pinned-snapshot behavior, identical reopened output, invalid colors,
zero/oversized images, alpha rejection by PPM, duplicate/missing providers,
unsupported schemas/ports, malformed payloads, and refusal to overwrite outputs.
Reviewed the diff for package boundaries, unnecessary abstractions, and scope.

## Try it

```sh
cargo run -p fold-app --bin fold-cli --no-default-features --locked -- checkpoint demo.fold demo.ppm
cargo run -p fold-app --bin fold-cli --no-default-features --locked -- render demo.fold rerender.ppm
```

Use new paths. The first command leaves an editable `.fold` archive and a 64×64
opaque blue/cyan PPM; the second renders the saved source through the same engine.
The foundation tracker is complete; phase 3 (Dear ImGui shell) is next.
