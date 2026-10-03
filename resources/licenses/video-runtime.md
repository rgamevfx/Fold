# Optional native-video decoder runtime

Fold's optional `fold-video-helper` dynamically links the pinned, minimal
FFmpeg 6.1.1 build produced by `scripts/build_video_runtime.py`. It is a separate
supervised process; the application does not link FFmpeg or CUDA libraries.

- FFmpeg: Copyright (c) the FFmpeg developers, LGPL 2.1 or later for this build.
  Official source: https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz
  SHA-256: `8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`.
  GPL, nonfree and version-3 components are explicitly disabled. No libx264 is
  linked. Only H.264 decoding, MOV demuxing, file protocol and CUDA/NVDEC hardware
  support are enabled. This does not change the licensing of a separately
  installed FFmpeg executable used by Fold's software/reference adapters.
- nv-codec-headers n12.1.14.0: NVIDIA copyright, permissive license reproduced in
  the installed `share/fold-video/licenses/nv-codec-headers-license.h`.
  Source: https://github.com/FFmpeg/nv-codec-headers/tree/n12.1.14.0
  SHA-256 of the pinned release archive:
  `2fefaa227d2a3b4170797796425a59d1dd2ed5fd231db9b4244468ba327acd0b`.
- CUDA and NVDEC driver libraries are provided by the user's NVIDIA installation,
  not bundled by this build script. Building the helper additionally requires
  installed CUDA development headers. Generated FFI bindings are build outputs.

The runtime build installs the original source archives, exact build script,
configuration manifest and LGPL text under `share/fold-video/`. Distributors
must preserve these materials, license notices and the replaceable shared
libraries when packaging the helper. The libraries are not statically linked.
ABI-compatible LGPL 2.1 builds can be substituted; the helper rejects GPL,
nonfree, unexpected ABI/version and non-minimal runtime configurations explicitly.
The binary resolves its runtime from `$ORIGIN/../lib`; never globally prepend
these minimal libraries to `LD_LIBRARY_PATH`, which would override an unrelated
system FFmpeg executable's libraries.
