# Motion native Skia input

Fold uses fframes' existing SVG/Skia renderer. The pinned Skia dependency can
quantize linear paint colors to byte vertex attributes. The adjacent patch keeps
byte storage only when it preserves the exact components. Other colors use
Skia's existing float attributes. It changes no surface format or blending rule.

## Build

Get the source archive from
[rust-skia m153-0.101.2](https://codeload.github.com/rust-skia/skia/tar.gz/refs/tags/m153-0.101.2).
The builder verifies SHA-256
`30c8b51a0a32fae63562c303bdc91605519a9598951422610e3fb8685b3c4f6f`.
Supply the standard rust-skia 0.153.3 archive that matches the locked features
and host. Keep its filename for the output, in a different directory.

```sh
python3 scripts/build_motion_skia.py SOURCE.tar.gz REFERENCE.tar.gz OUTPUT/REFERENCE.tar.gz \
  --source-dir target/motion-skia-source
```

Use the actual archive filenames in place of `REFERENCE.tar.gz`. The source
directory can be reused. The builder checks its source against the pinned archive
and patch. Add `--offline` only after native dependencies have been fetched.
Cargo dependencies must already be available. The build needs the usual Skia
Clang, Python and Ninja toolchain. WebAssembly tools are not used.

Pass the resulting archive to `scripts/package_fold.py --motion-skia`, or to
`scripts/stage_motion_sdk.py --skia`. SDK staging rejects the original unpatched
archive and mismatched library hashes. Runtime uses the precompiled SDK libraries;
it does not build Skia or invoke Cargo.

## Check

With the prepared source directory, run:

```sh
SKIA_SOURCE_DIR="$PWD/target/motion-skia-source" \
  cargo run --manifest-path crates/motion-fframes/Cargo.toml --release \
  --example color_precision --offline
```

This needs a native Vulkan device. It checks three SVG shapes against an analytic
linear-sRGB sample and preserves signed HDR shader output and premultiplied alpha.
The small CPU surface is a diagnostic reference in this example only.

Recorded results are in [color-precision-evidence.json](color-precision-evidence.json).
See [current work](../../docs/current-work.md) for remaining qualification;
local build/package paths are not durable runtime configuration.
