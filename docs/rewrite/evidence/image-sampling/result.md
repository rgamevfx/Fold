# Image sampling evidence — 2026-10-11

Image Texture and Sample Image are implemented in the current Motion system.
Historical code at `66ebb2e` informed the behavior. No old engine or deferred
ImageField was restored. Samples are ordinary typed color/scalar values.

## Behavior

Image Texture selects an asset or declared document output. Source time is exact;
source identity stays static for dependency validation. Sample Image supports
nearest/bilinear filtering, four wrap modes, color and five scalar channels.
Filtering handles premultiplied alpha; output color is straight linear Rec.709.
Negative RGB clamping defaults on for material inputs and can be disabled.
Positive HDR values remain intact. Scalar values are not clamped.

The worker-owned image service uses immutable snapshots and shared media/color
services. It caches native-size frames within an evaluation (128 MiB/64 frames).
Rendering, observation, and parameter sampling use the same service. Candidate
observations use the candidate snapshot. Source dependencies support save/reopen,
undo, missing-resource errors and cycle rejection. Inspector controls use metadata
and route edits through existing transactions. Attribute Create evaluates values
in each element's context so sampled values can be stored as attributes.

## Verification

All Cargo commands used `--offline --locked -j 2`, disabled incremental compilation,
and disabled dev/test debug symbols.

- `cargo check -p fold-app --features desktop`: passed.
- `cargo test -p fold-motion --features ui --lib --test images`: 46 library tests
  and 9 image integration tests passed; 10 optional GPU captures were skipped.
- `cargo test -p fold-motion --features ui --lib ui::presentation_render::image_inspector_capture -- --ignored --exact --test-threads=1`: passed.
- Platform and color library tests: 21 passed. Motion distribution, procedural,
  and parameter-service regression tests: 26 passed.
- Real CLI demo and existing `agent_observe` tests: passed (20 tests).
- `cargo test -p fold-app --features spatial --test agent_images public_image_sampling_native_frame -- --ignored --exact --test-threads=1`: passed with
  `FOLD_COLOR_ROOT` set to the packaged runtime. The public workflow created two
  documents, sampled times 2/0/1, checked colors/stable IDs, saved/reopened, and
  rendered through the native GPU host. The image source was sampled on the CPU.
- Motion Clippy with `--features ui --all-targets --no-deps`: completed; existing
  `items_after_test_module` and `collapsible_if` warnings remain. Strict warnings
  also encounter existing media warnings. No new Motion warnings remain.

Reviewed [Inspector capture](inspector.png) at 400 and 260 pixel widths. Labels,
source names and controls fit. Reviewed [native clone render](native-cloner.png):
one red clone and one green clone. [Measurements and provenance](observation.json)
record the CPU samples and GPU frame. Open [saved project](image-cloner.fold) to
repeat the workflow. Rebuild with [public demo](../../../../demos/image-sampling.py).

## Limits and acceptance

CPU sampling cannot consume GPU-only source graphs. Existing import formats apply;
PNG/JPEG decoding was not added. EXR sequences use 24 fps. No full workspace
checkpoint or physical artist interaction was performed. This bounded M7 addition
closes neither M6 nor M7. See the [usage guide](../../../../skills/fold/references/image-sampling.md).
