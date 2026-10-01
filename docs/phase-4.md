# Phase 4 — Engine texture in the viewer

## Goal

Display the existing engine's generated solid source in the docked viewer, using
one explicit CPU-to-GPU presentation fallback (product sections 7, 14–17).

## Tasks

- Add a bounded, owned opaque sRGB RGBA8 display result using the same output
  transform as headless PPM delivery; do not alter scene-linear engine pixels.
- Assemble a committed demo document and static provider in app; render its
  immutable snapshot on a worker, delivering one result through a bounded channel.
- Keep UI evaluation-free: poll without blocking, upload the display result once,
  retain a host-owned texture registration, and fit the image without distortion.
- Exercise output equivalence, pending/error/image UI states, and native display.

## Acceptance criteria

- One native window visibly displays the engine-rendered demo at correct aspect
  ratio through resize; Inspector and Timeline remain placeholders.
- UI drawing never performs source compilation or CPU rendering. Pending and
  failed rendering are reported explicitly; the one-shot queue is bounded.
- Display bytes match the headless output transform. Color and ownership are
  explicit; texture handles stay private to the UI host.
- Focused tests, affected-crate checks, and dependency-boundary checks pass.

## Scope

One opaque SDR solid at exact time zero, 640×360, one in-flight request, and one
presentation texture. CPU engine buffers remain owned by render; the UI owns only
the uploaded display copy and its registry lifetime. No engine GPU pool, graph,
media decode, transport, cache, interactive requests, or zero-copy claims. Those
require subsequent phases. A non-sRGB surface and unorm texture avoid the backend's
approximate gamma correction: the engine applies the exact sRGB transform once.

## Verification

Implemented and verified on the available Linux/COSMIC Wayland desktop:

- Touched Rust files formatted with rustfmt.
- `cargo check -p fold-render -p fold-platform -p fold-ui -p fold-app --features desktop --all-targets --locked` — passed.
- `cargo test -p fold-render --locked` — 2 tests passed, including display/PPM
  byte equivalence, unchanged scene pixels, and transparent-output rejection.
- `cargo test -p fold-ui --locked` — 2 tests passed, including aspect fitting and
  pending/error/image frame construction at three window sizes.
- `cargo test -p fold-app --bin fold-desktop --features desktop --locked` — passed;
  registered demo provider produces 640×360 opaque `[0, 188, 255, 255]` pixels.
- `python3 scripts/check_boundaries.py` — passed, including headless UI isolation.
- Locked desktop build passed. Native screenshots confirmed the generated image
  in the viewer at initial window size and after maximizing to 1920×1080; image
  bounds retained 16:9 aspect (approximately 679×382 and 1319×742 pixels).
  Screenshot pixel samples were exactly `[0, 188, 255, 255]` in both sizes.
- Native maximize input succeeded. Closing the new window through its title-bar
  close button exited its process without stderr diagnostics. An already-running
  older placeholder Fold window was left untouched.
- Native evidence remains outside the repository at
  `/home/rgame/.local/state/agent-desktop/fold-phase4/`.

## Completion and deferred work

Phase 4 acceptance criteria passed for this one-shot opaque CPU-upload path.
The bounded channel has capacity one and is polled with `try_recv`; only one
result is accepted and only one display texture is allocated/registered. Dropping
the desktop unregisters it, then drops handles without explicitly destroying
in-flight GPU resources. Render errors and worker disconnection produce a visible
failure state; backend errors stop with diagnostics.

Workspace/full-suite/Clippy checks remain deferred to the media slice checkpoint.
No performance targets, HDR/monitor color management, transparency presentation,
sRGB-only surface support, GPU readback test, device-loss recovery, repeated seeks,
stale-generation rejection, or engine GPU resource pooling are claimed here.
The one-shot demo cannot produce competing or stale preview generations. Phase 3
minimize/restore, native layout reset, IME/accessibility, and physical DPI matrix
checks remain deferred; normal close was exercised successfully in this phase.
