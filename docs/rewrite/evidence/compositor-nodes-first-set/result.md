# First compositor node set — 10 October 2026

User-directed replacement of the temporary Grade, Transform, Blur and Shuffle
inspectors against `crates/compositor/prototypes/nodes-reference.html` and its
companion interaction prototypes. This is a bounded compositor change, not
closure of the Motion rewrite or the complete 36-node reference.

## Implemented

- **Grade v3:** uniform/split RGB rows, preserved unequal values when collapsed,
  explicit black/white clamps, inverse grading, RGB default selection, selectable
  scalar/alpha processing, optional unpremultiplication, mask and mix.
- **Transform v3:** uniform/split scale, translate, rotation, center, independent
  degree-based skews and their order, inverse, reconstruction filter, ringing clamp,
  transparent/extended edges, exact rational shutter sampling of animated transforms.
  All input channels transform; the old effect mask/channel/mix surface is removed.
- **Blur v3:** uniform/split XY size, identity default, Gaussian/Box filtering,
  transparent/extended edges, channels, mask and mix. The old Gaussian-only
  compositor configuration and renderer pipeline names are replaced.
- **Shuffle:** visible, colored channel routing with source/output selection,
  click/drag assignment and zero/one constants. Up to two layer routes have direct
  diagrams; other channels retain explicit access without repeating those controls.
  The independently valid named-channel model remains the one implementation.
- Shared controls retain component track identities, keying, edit lifecycle and
  numeric entry. Collapsing is presentation only; combined drags retain offsets.
  Existing non-compositor property callers retain their layout and sensitivity.

Grade/Transform/Blur v2 records no longer execute; there are no version aliases,
migration readers or fallback kernels. Merge remains v2. Native document schema
remains 8; generic unsupported payload preservation is unchanged. Existing
independently used render operations remain supported.

## Verification

Build settings: `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. The initial debug build was
stopped for disk pressure and its newly generated cache artifacts removed.

| Command / evidence | Result |
| --- | --- |
| `cargo check -p fold-app --features desktop --all-targets` | Passed |
| `cargo test -p fold-compositor --features ui --lib --tests --no-fail-fast` | 51 passed; capture initially ignored |
| `cargo test -p fold-compositor --test animation transform_shutter` | Passed; 2 exact subframes produce the expected averaged pixels |
| `cargo test -p fold-ui --lib animated_property` | 4 passed, including combined drag and display-only split |
| `cargo test -p fold-render --features gpu --lib node_tests` | 2 passed: Grade inverse/clamps and fractional Box coverage |
| `cargo test -p fold-render --features gpu --test channels` | 7 passed; 5 native GPU cases ignored in default run |
| Explicit ignored Grade/merge parity and fractional-sampling/filter parity cases | Both passed; CPU/GPU tolerance 1e-5 |
| `cargo test -p fold-render --test regions` | 3 passed |
| `cargo test -p fold-app --test current_contracts` | 4 passed, including shipped fixtures and save/reopen |
| `FOLD_NODE_CAPTURE=1 cargo test -p fold-compositor --features ui --lib shuffle_inspector_normal_and_narrow -- --ignored --nocapture` | Passed; final normal/narrow captures visually reviewed |
| `git diff --check` | Passed |

Rendering adapter: llvmpipe LLVM 20.1.2, Vulkan, Mesa 26.1.6. Captures use the
production theme and bundled typography, at 420px and 280px panel widths, 1x scale.
Injected pointer events open shutter settings; the shared control test checks
preserved RGB offsets, one edit finish, and no mutation on split/collapse.
Physical artist input and additional DPI scales are not certified by this run.
Targeted GPU-only builds emit the pre-existing native-video conditional YUV
warnings; the desktop build check is clean.

## Native UI captures

| Node | Normal | Narrow |
| --- | --- | --- |
| Grade | [Combined](grade-normal.png), [split](grade-split-normal.png) | [Combined](grade-narrow.png), [split](grade-split-narrow.png) |
| Transform | [Main](transform-normal.png), [shutter](transform-shutter-normal.png) | [Main](transform-narrow.png), [shutter](transform-shutter-narrow.png) |
| Blur | [Normal](blur-normal.png) | [Narrow](blur-narrow.png) |
| Shuffle | [Normal](shuffle-normal.png) | [Narrow](shuffle-narrow.png) |

## Public authoring and removal evidence

Updated the public mixed-scene script to request Grade v3 from discovery. Executed
`m5a-mixed-scene.py`, then `m5b-background-spheres.py` and its `wrangle` variant
against a fresh local Session with the current renderer. Each workflow proposed,
inspected and applied native edits, observed frames and saved a new project copy.
Replaced only the three repository demo fixtures with those authored outputs.
Saved revisions: 13, 14 and 18 respectively. The mixed-scene workflow's static
background equality check passed. Full reports and rendered observations are in
`/tmp/fold-node-review`; the temporary host runner was removed after execution.
The active Fold skill and architecture inventory now describe the v3 contracts.

## Remaining reference scope

The renderer still uses the composition-sized raster: image bounds cannot grow
outside its format. Consequently the prototype's switchable **Crop to format**
is not exposed as a nonfunctional control. Extended data windows require a later
render-domain change. Transform shutter samples animate this node over the image
at the requested frame; they do not retime upstream footage. Box filtering is
labeled **Box**, without claiming a separately qualified performance target.
This record does not close the existing physical desktop, export or Motion gates.
