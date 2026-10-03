# Phase 17 — shared GPU rendering

## Goal, boundaries, and status

**Implemented and verified on the reference GTX 1070.** The phase-17 GPU rendering and two-video/procedural-title checkpoint pass. This replaces the earlier partial-progress status; that intermediate core-operator implementation was not phase acceptance.

Execute the existing immutable image graph on a feature-neutral GPU host shared with desktop presentation and usable headlessly. Keep the CPU reference/fallback, explicit ACEScg/input/output boundaries, exact timing, independent viewer generations, and phase-15A presentation admission. Decoder replacement/hardware decode remain phase 18; generalized scheduling and sustained real-time acceptance remain phase 19. No authoring graph, project schema, or panel control was added.

## Implementation

### Host ownership and execution

- `crates/render/src/gpu/host.rs` owns the device/queue service, texture pool, budget, and stopped-device state. Desktop adopts its surface-compatible device and shares it with the preview worker; headless callers create a Vulkan host without ImGui. Panels still receive registered presentation IDs, not device access.
- The application host budget is **512 MiB**, covering pooled working/display images, explicit staging/readback buffers, parameter buffers, status/timestamp buffers, and LUT reservations. The existing **256 MiB shared presentation cache** remains a separate retention policy, not an additional unaccounted allocation allowance. Pipeline/driver metadata and opaque driver overhead are not represented as exact VRAM accounting.
- Pool reuse requires both release of consumer leases and release of submission-held leases by a queue-completion callback. Local scratch reuse is permitted only after the last logical read in the same ordered command stream. Held viewer images and UI submissions retain GPU display leases independently. Presentation verifies a globally unique host-service owner: separate wgpu Instances can reuse raw device IDs, so raw Device equality is not an ownership check.
- Reservations/backpressure occur before allocation. Driver allocation runs outside the pool accounting lock. Exhausted budgets fail explicitly without waiting on the UI. Cancellation is checked before work/submission/publication; submitted work is not assumed preemptible and retains leases through completion.
- Device loss/uncaptured GPU errors put the service in a **stopped-render state**. Recovery is restart/recreation, not an unverified transparent recovery scheme. Tests exercise a destroyed device and budget exhaustion/recovery; deliberately exhausting the physical GPU/OS is not claimed.
- Structural validation, input-color preflight, reachability, and input-use counts are shared by CPU and GPU execution. Nested outputs remain graph fragments, not CPU-rendered intermediates. Application CPU/GPU paths share scene compilation and rational frame timing.

### Operators, native planes, and color

- GPU operators: solid, nearest transform, opacity, ordered Porter-Duff over, crop, alpha mask, RGB gain/grade, and separable full-kernel box blur with transparent borders.
- Working images use **RGBA32F** in this implementation. This avoids introducing half-float range loss while establishing equivalence for signed/HDR working values and repeated grade/blur operations. RGBA16F optimization is not required for correctness and is not claimed. No operator silently clamps ACEScg RGB to alpha or bypasses an effect.
- The current decoder's native reconstructed signal is **full-resolution planar GBR float32**. `fold-media::RgbImage` now retains these immutable planes (12 bytes/pixel, rather than expanding them to 16-byte RGBA). The GPU packs native planes to RGBA and applies the assigned OCIO input processor. RGB8 fixture input also receives GPU OCIO conversion.
- **Explicit CPU adapters remain:** existing FFmpeg decode, BT.709 YUV range/chroma reconstruction to native float RGB, and motion/vector coverage rasterization. They are bounded and included in adapter timings and upload counts. There is no NVDEC, native YUV surface import, or zero-copy decoder claim; replacing the batch decoder/establishing hardware interoperability remains phase 18.
- `fold-color` exports owned OCIO GLSL/LUT descriptions through an optional extension to the private ABI-1 bridge. Existing CPU-only packages remain usable; GPU selection with an older bridge fails with an actionable rebuild message, not a substitute transform.
- Production OCIO processing uses the pinned processor's generated shader, **not a sampled approximation of its color function**. The integration supports analytic transforms, 1D LUTs represented as 2D textures, and 3D linear/tetrahedral LUTs. Resources share CPU config/LUT/runtime content identity. The cache is bounded to 16 pipelines; descriptors/LUTs are bounded to eight textures and four million float LUT components. Unsupported dynamic uniforms/shaders fail explicitly; selecting the CPU backend is deliberate.
- OCIO input/output processing preserves alpha independently, unpremultiplies/repremultiplies where needed, and validates float results before quantization. A GPU validation pass is separate from generated GLSL because Naga's GLSL frontend does not support the required atomic status operation. Later crops/masks cannot conceal invalid intermediates.
- GPU timestamps and validation use small asynchronous readbacks: **24 bytes per timed submission**, not working-image pixels. Readiness requires completed validation. Explicit worker-only scene/output readback is timeout-bounded and cancellable.

### Presentation and delivery

- `Session` preview workers receive the shared host, evaluate scene graphs, and publish GPU display leases. `PreviewHost` waits nonblockingly for readiness, registers the completed **RGBA8Unorm** texture, and retains it in the existing cache. **Normal GPU preview performs no working-frame or display-frame image readback, and no CPU display upload.** Cache hits do not repeat decode, OCIO, or upload.
- Output is an explicit black matte plus selected OCIO output transform, followed by validation/quantization. The surface and registered texture are non-sRGB because display processing has already occurred. No second display transform is applied.
- Generation admission, held images, stale-result rejection, independent transports, Every-frame pacing, and actual-error reporting remain in their existing owners. GPU completion alone never advances a playhead.
- GPU delivery compiles from the pinned committed snapshot and persisted output settings, never a preview cache/key. It reads back only the explicitly output-transformed RGBA8 encoder input. Matching GPU preview/delivery settings produce identical pre-encode bytes in the checkpoint. Preview defaults to sRGB/ACES SDR; delivery's normal Rec.709 default is intentionally separate.
- The evaluator content version was advanced. Backend selection is startup/job policy, not project mutation:
  - Desktop preview defaults to shared GPU. `FOLD_RENDER_BACKEND=cpu` selects the CPU reference.
  - `FOLD_RENDER_BACKEND=gpu` selects GPU export in a GPU-enabled desktop/CLI build. Export otherwise retains the CPU reference default; no effect is individually or silently switched between backends.
  - `cargo build -p fold-app --features gpu` supports headless GPU delivery. Default/no-feature headless builds remain CPU-only and reject a GPU delivery request clearly.
- Packaging includes the new bridge and hashes WGSL sources in the reproducibility manifest.

## Precision and filtering policy

- Raster coordinates remain top-left, pixel-center sampled; transforms use nearest-neighbor with transparent outside samples. GPU inverse coefficients/arithmetic are float32; CPU coordinates are float64. At nearest-sample discontinuities, floating-point rounding can select an adjacent sample. This is an explicit backend precision boundary, not a bilinear filter substitution or a promise of bit-identical arbitrary transforms.
- Blur is the same transparent-border box kernel, implemented as two GPU passes with float32 accumulation versus CPU float64 sums. Extreme finite values can exceed GPU arithmetic range; invalid intermediates fail rather than becoming a valid-looking image. The CPU reference is available for higher-precision cases.
- Tested legacy operator absolute tolerance: **3e-6**. Tested ACES/OCIO working tolerance: **2e-4** (OCIO CPU optimized math and GPU shader math are not bit-identical). Display/output and real-workload comparisons permit **one SDR code value**, including LUT hardware interpolation/quantization. Alpha remains explicit and is not color-transformed.
- Native-plane storage tests check channel order, negative/above-one precision, byte accounting, malformed length, and nonfinite rejection. GPU differential fixtures cover signed/HDR RGB, alpha edges, nested fragments, transformed/cropped masks, radii 0/1/4/64, rotated/reflected transforms, and retained-image safety.

## Verification

Reference environment: Linux `7.1.5-76070105-generic` x86_64; Intel i7-8750H, 12 logical CPUs; 33,560,539,136 bytes RAM; NVIDIA GTX 1070 with 8192 MiB VRAM; NVIDIA driver `580.173.02`; Vulkan/wgpu 27; Rust `1.95.0`; OCIO 2.4.2 / ACES 1.3 Studio 2.2.0. Release measurements are based on `9be8aae` plus the phase-17 working tree; package manifests record dirty state and source/binary hashes.

Build/test environment:

```sh
export PKG_CONFIG_PATH=/home/rgame/.local/share/fold-build/alsa/root/usr/lib/x86_64-linux-gnu/pkgconfig
export FOLD_COLOR_ROOT=/home/rgame/Documents/fold-phase-17/package
export FOLD_TEST_COLOR_ROOT="$FOLD_COLOR_ROOT"
```

The first desktop check without the existing local ALSA pkg-config path failed; supplying the documented path resolved it. A native storage assertion was updated from 16 to 12 bytes/pixel after retaining native planes. A separate-instance ownership regression exposed colliding raw wgpu device IDs; globally unique service IDs now enforce ownership in presentation. These checks were corrected and rerun successfully. No failing test is being treated as acceptance.

- `cargo test --workspace --all-features -- --test-threads=1`: **188 passed, 0 failed, 13 opt-in tests ignored**.
- Explicit `--ignored --test-threads=1` invocation: **13 passed, 0 failed**. This includes packaged color/CLI/export tests, the GPU-connected Session preview/cancellation test, native audio, real UI texture lifetime checks, GPU operators, native adapters, and OCIO display/1D/3D LUT tests.
- Final focused GPU rerun after shortening the pool allocation lock and adding tetrahedral LUT coverage: **4 passed**. Final UI rerun after explicit foreign-device rejection: **38 passed**, with the native texture test separately covered above.
- Workspace/all-feature/all-target Clippy completed with only inherited warnings: four platform test initializers, four UI test `drop_non_drop`, one compositor conditional, and ten Motion warnings (plus duplicated lib-test reports). No new-code warning remains.
- Strict affected `fold-render`/`fold-color`/`fold-media` Clippy with `-D warnings` passed. No-feature headless check, workspace formatting check, diff whitespace check, and package-boundary/headless UI isolation checks passed. Python tooling suite: **12 passed**.
- The private runtime/package builds succeeded. Packaged CLI GPU export succeeded from `/` with `FOLD_COLOR_ROOT` unset and `$OCIO` pointing to a nonexistent file. Packaged desktop launch was separately inspected. Selecting an old CPU-only bridge for GPU export failed clearly and left no output file (`old-runtime.log`). No clean-machine or broader-platform certification is implied.

All machine-local logs/projects/packages/screenshots are outside the repository under `/home/rgame/Documents/fold-phase-17/`.

### Two-video + procedural-title GPU checkpoint

`crates/app/examples/execution_probe.rs` retains its CPU baseline mode and adds an explicit GPU mode. It creates a new ACES project containing two linked 1080p30 video sources, opacity/compositing, nested animated motion text, and stereo audio. The GPU mode explicitly chooses matching sRGB preview/delivery settings, compares CPU/GPU output, and exports four frames through GPU output processing.

```sh
FOLD_RENDER_BACKEND=gpu cargo run -p fold-app --release --features gpu \
  --example execution_probe -- \
  /home/rgame/Documents/fold-phase-11/reproducible/background.mp4 \
  /home/rgame/Documents/fold-phase-11/reproducible/overlay.mp4 \
  /absolute/new-checkpoint.fold 6
```

Final native-plane run: `planes-workflow.log`, `resources-final.json`, `planes-acceptance.fold`, and `planes-acceptance.mp4`.

| Measurement | Observed result |
| --- | --- |
| GPU evaluation + output timestamps, 12 frames across cold/repeat passes | **5.551–6.644 ms/frame** |
| Cold first-frame host time, including decode/setup | **1096.991 ms** |
| Subsequent host evaluation-to-ready times | **424.783–511.262 ms/frame** |
| CPU adapter time | cold **920 ms**; subsequent **372–459 ms/frame** |
| Per-frame uploads | **82,944,000 bytes**: two native GBR frames + vector RGBA |
| Normal GPU preview image readback | **0 bytes** |
| Validation/timestamp readback | **48 bytes/frame** |
| Compute passes | **12/frame**, including native-plane packing and color validation |
| Host-accounted GPU peak | **265,422,040 bytes** under **536,870,912-byte** budget |
| Retained pool in the probe | **182,476,816 bytes** |
| Sampled process-tree RSS peak | **946,196,480 bytes** |
| Sampled scratch peak | **35,467,757 bytes** |
| CPU/GPU output maximum difference | **1 code value** |
| Matched GPU preview/delivery | **identical bytes** |

Process/scratch measurements cover the full probe, including CPU comparisons, audio preparation, export, and subprocesses; RSS can double-count shared pages and sampling can miss brief peaks. They are not a pure-viewer or total-driver-VRAM measurement. Export inspection confirmed four H.264 1920×1080 frames at 30/1, BT.709 metadata, plus AAC audio. Hardware decode/encode was not used.

**This is not sustained real-time playback acceptance.** GPU image execution fits comfortably below a 33.3 ms frame period in this fixture, but decode/reconstruction/vector adapters still dominate overall latency. Phase 18 owns the retained decoder/hardware-interoperability work; phase 19 owns aggregate scheduling and sustained uncached 1080p30 acceptance. No target has been silently revised.

### Native desktop and playback

- `native-final-report.json`: success on the real desktop/shared-device path. Ten cold frames and ten presentation-cache seeks plus a cancelled/replaced demand were exercised. Logs show **zero image-readback bytes** for every GPU preview and zero CPU-upload bytes for registering GPU results. Cached seek-to-present-call observations were approximately **6.4–8.0 ms**. These are application/present-call observations, not physical scanout latency.
- `mixed-playback-report.json`: success with two independent GPU viewers, using the existing phase-15A application-command harness adapted to forward the GPU host/result service. Eight deliberately delayed Every-frame frames arrived consecutively; then **64 cached sequential admissions took 2.134 seconds** at 30 fps. An independently audio-monitored Real-time viewer continued alongside it. Mode switches and exact paused seeks to frame 3 passed. This legacy-color fixture checks backend/presentation isolation; the separate ACES workload above checks color/end-to-end composition.
- Screenshots `20-27-55` and `20-55-32` show the ACES two-source/title result in the native and packaged desktop. Screenshot `21-40-39` confirms the final `verified-package` desktop after the explicit host-owner check. Screenshot `20-46-15` shows the two independently playing viewers in narrow columns, with held images and bottom transports intact.
- The initial desktop probe used an ordinary file argument (asset import) rather than `--project`; that launch was stopped and rerun with the correct project-open command. It is not counted as render acceptance.
- No pointer dragging was attempted. Exhaustive OS menu/drag/DPI workflows and physical scanout are not certified here. Existing normal/narrow ImGui input tests and native application-command probes cover the affected presentation contracts; no new visible UI controls were added.

## Limits retained deliberately

- Reference Linux/Vulkan/GTX 1070 only; no broad GPU/codec support claim.
- Current decode/YUV reconstruction and vector coverage are measured CPU adapters, not hidden GPU acceleration. Retained/hardware decode is phase 18.
- GPU output readback is intentional for the software encoder/reference tests, never the viewer source. Codec acceleration and delivery hardening remain phase 20.
- No driver-level forced physical OOM, hot device recreation, HDR monitoring, or ten-minute mixed-load certification. Errors stop rendering clearly; longer recovery/resource stress remains phase 21.
- No new generalized scheduler, disk cache, range-preparation workflow, or real-time performance claim. The initial 512 MiB host budget and 256 MiB presentation retention policy are bounded, not tuned as optimal for phase 19.
