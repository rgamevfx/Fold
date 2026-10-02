# Pinned color resources

`config.ocio` is the serialized built-in
`studio-config-v2.2.0_aces-v1.3_ocio-v2.4` from OpenColorIO **2.4.2**.
SHA-256: `d8b361f76750ebfbedf0ded0b5e4315b283eed5e095814486b9d7c416cfbbb4c`.
It is identical to the phase-11 CPU/GPU reference; no external LUT files are
required by this built-in config. Do not regenerate using an unpinned runtime.

- OCIO source and `OCIO-LICENSE`: OpenColorIO tag `v2.4.2`.
- `ACES-CONFIG-LICENSE`: OpenColorIO-Config-ACES tag `v2.1.0-v2.2.0`,
  <https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/blob/v2.1.0-v2.2.0/LICENSE>.
- The native packaging script also copies dependency licenses and the bundled
  xxHash/sampleicc licensed headers from the exact OCIO build sources.

Build the private Linux runtime outside the repository:

```sh
python3 scripts/build_color_runtime.py /absolute/build-dir /absolute/package-dir
```

The script verifies the OCIO source archive SHA-256, builds the dependencies
selected by that source tree (not the system OCIO), enforces compiler/CMake
versions from `toolchain.json`, and writes a content-hashed package manifest.
This pins the native tool versions, not a hermetic OS/container image.

Build the complete Linux desktop/CLI package with:

```sh
python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package-dir
```

The packager uses locked release builds and records Rust/compiler/source/binary
identities. Platform graphics/audio libraries and FFmpeg remain system
prerequisites. Python is only a build tool; system OCIO is not used. See the phase
report for same-host relocation checks and the limits of installation coverage.

Package layout:

```text
bin/<executable>
lib/fold/libfold_ocio.so
lib/fold/libOpenColorIO.so.2.4
share/fold/color/{config.ocio,manifest.json,*LICENSE,third-party/}
```

The private bridge uses `$ORIGIN` RPATH. Rust resolves this layout from its
executable (or an explicit absolute root), never CWD or `$OCIO`. External configs
must be self-contained directories with relative resource paths, no environment
variables or symlinks, at most 4096 entries / 32 levels / 64 MiB. They are copied
to worker-owned temporary storage and content-hashed before OCIO opens them.
Missing/invalid transforms fail rather than select a replacement. Processor
construction verifies that used file LUTs remain inside the pinned copy.

`fold-color` is the only native unsafe boundary. Public Rust APIs expose neither
raw handles nor borrowed native strings. Config/processor handles retain the
loaded library; exception-safe C entry points copy errors into caller buffers.
Handles are worker-local, deliberately not `Send`/`Sync`. This keeps loading,
resource I/O and processing out of the UI thread.

New application projects use ACEScg; archives without a color contract retain
legacy linear-sRGB interpretation. Viewers use sRGB/ACES SDR independently of
the selected output transform. See `docs/phase-16.md` for application integration,
verification evidence, and the explicit later-phase boundaries.
