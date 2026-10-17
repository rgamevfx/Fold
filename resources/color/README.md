# Pinned color resources

`config.ocio` is the serialized built-in
`studio-config-v2.2.0_aces-v1.3_ocio-v2.4` from OpenColorIO **2.4.2**.
SHA-256: `d8b361f76750ebfbedf0ded0b5e4315b283eed5e095814486b9d7c416cfbbb4c`.
No external LUT files are required by this built-in config. Do not regenerate
using an unpinned runtime.

- OCIO source and `OCIO-LICENSE`: OpenColorIO tag `v2.4.2`.
- `ACES-CONFIG-LICENSE`: OpenColorIO-Config-ACES tag `v2.1.0-v2.2.0`,
  <https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/blob/v2.1.0-v2.2.0/LICENSE>.
- The native packaging script also copies dependency licenses and the bundled
  xxHash/sampleicc licensed headers from the exact OCIO build sources.

Cargo prepares and caches the private Linux runtime automatically. To produce a
standalone runtime package for development:

```sh
python3 scripts/build_color_runtime.py /absolute/build-dir /absolute/package-dir
```

The script verifies the OCIO source archive SHA-256, builds the dependencies
selected by that source tree (not the system OCIO), and writes a content-hashed
package manifest recording the compiler/CMake versions actually used.
`toolchain.json` records the original reference environment; it is not a build
requirement. Cargo caches by producer, bridge, resource and toolchain identity;
cache reuse verifies all packaged file hashes. Packages are published only after
a successful build, with a lock serializing concurrent producers.

For the complete desktop/CLI package, use the [development guide](../../docs/development.md#package).
The packager records compiler/source/binary identities. Platform graphics/audio
libraries and FFmpeg remain system prerequisites. Python is only a build tool;
system OCIO is not used.

Package layout:

```text
bin/<executable>
lib/fold/libfold_ocio.so
lib/fold/libOpenColorIO.so.2.4
share/fold/color/{config.ocio,manifest.json,*LICENSE,third-party/}
```

The private bridge uses `$ORIGIN` RPATH. Rust resolves this layout from its
executable (or an explicit absolute root), never CWD or `$OCIO`. Executables
running inside their original Cargo profile directory use the automatically
prepared build cache. Relocated executables require the complete package; they
never fall back to a developer machine’s cache. External configs
must be self-contained directories with relative resource paths, no environment
variables or symlinks, at most 4096 entries / 32 levels / 64 MiB. They are copied
to worker-owned temporary storage and content-hashed before OCIO opens them.
Missing/invalid transforms fail rather than select a replacement. Processor
construction verifies that used file LUTs remain inside the pinned copy.

`fold-color` owns the native OCIO unsafe boundary. Public Rust APIs expose neither
raw handles nor borrowed native strings. Config/processor handles retain the
loaded library; exception-safe C entry points copy errors into caller buffers.
Handles are worker-local, deliberately not `Send`/`Sync`. This keeps loading,
resource I/O and processing out of the UI thread.

New application projects use ACEScg; archives without a color contract retain
legacy linear-sRGB interpretation. Viewers use sRGB/ACES SDR independently of
the selected output transform. See [current work](../../docs/current-work.md)
for Motion color/delivery qualification limits.
