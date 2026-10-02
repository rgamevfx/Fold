# Phase-11 OCIO/wgpu feasibility probe

Not a production color module. Python is only fixture generation against pinned
OCIO; Rust executes the generated shader through the same wgpu/Naga versions as
Fold (27.0.1 / 27.0.3), using Vulkan on the GTX 1070. The isolated Cargo workspace
and lockfile do not add dependencies to Fold's runtime or headless graph.

Run from the repository root (choose new absolute artifact directories):

```sh
python3 -m venv /absolute/external/ocio-venv
/absolute/external/ocio-venv/bin/pip install -r scripts/phase11-ocio/requirements.txt
/absolute/external/ocio-venv/bin/python scripts/phase11-ocio/generate.py /absolute/external/ocio-fixture
CARGO_TARGET_DIR=target cargo build --release --locked --manifest-path scripts/phase11-ocio/Cargo.toml
timeout 60 target/release/fold-phase11-ocio-probe /absolute/external/ocio-fixture/display.json
timeout 60 target/release/fold-phase11-ocio-probe /absolute/external/ocio-fixture/lut-display.json
```

Generation refuses an existing directory. Execution writes/replaces `.cpu.ppm`
and `.gpu.ppm` next to each fixture. The 64×16 reference image contains gray and
colored exposure ramps, negative/above-one RGB, opaque black/white and alpha
0/0.01/0.5/1. The wrapper unpremultiplies before OCIO and repremultiplies afterward;
zero-alpha pixels become zero. Comparison is on float results **before** PPM
quantization/clamping; negative display values are not hidden from the test.

`display` is the unmodified ACEScg → sRGB display / ACES 1.0 SDR Video processor.
The analytic processor needs no LUT. `lut-display` prepends an asymmetric 33³
linear-interpolated synthetic LUT to exercise extraction, ordering, texture
upload and binding. This supplemental transform is not the selected display
policy or a claim to support every external config.

Required GLSL adaptations, all in `generate.py`:

- Flatten OCIO nested `.rgb.r/g/b` swizzles: Naga rejected stores through them.
- Separate combined texture/sampler declarations into explicit bindings.
- Use explicit LOD zero for compute sampling; preserve OCIO's `.zyx` coordinates.
- Use read/write storage output declaration: Naga rejects write-only buffers.

Tolerances: analytic absolute channel error ≤0.0002; supplemental LUT ≤0.001
(less than one quarter of an 8-bit code value). A preliminary 2³ LUT showed
0.02298 error; the 33³ fixture reduced it to 0.000428. Hardware interpolation
precision matters, especially near the display toe. Do not raise production
tolerances automatically: use manual float interpolation or another verified
path for configs that exceed their declared tolerance. Production must validate
1D/2D/3D LUT types, interpolation modes, uniforms, resource limits and shader
constructs; this spike rejects unsupported resource classes instead of silently
omitting them. RGBA32F filterable LUT storage is explicitly required here; it does
not prove RGBA16F working-frame equivalence.

Output includes adapter, driver, maximum float error and host wall times. Upload
includes host allocation/staging; dispatch includes synchronization/readback.
These are **not GPU timestamp measurements, 1080p throughput, swapchain
presentation, native Fold acceptance, or zero-copy codec interoperability**.
The executable intentionally refuses another adapter. Wrap runs with the shown
timeout because driver waits are not guaranteed to terminate.

See `docs/phase-11.md` for results, contracts, runtime packaging and remaining gates.
