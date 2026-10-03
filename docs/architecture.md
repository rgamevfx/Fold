# Current architecture and dependency policy

Fold follows the ownership model in `Fold_Application_Product_Architecture.md`.
This document describes the current implementation; phase documents record
historical slice evidence, not the current architecture. See `consolidation.md`
for the remaining consolidation work and `phase-9.md` for motion acceptance.

## Ownership

| Crate | Responsibility |
| --- | --- |
| `fold-foundation` | Stable IDs and checked rational time |
| `fold-project` | Feature-neutral envelopes, assets, revisions, transactions, history and persistence |
| `fold-media` | Bounded CPU image/media adapters, supervised FFmpeg processes and audio preparation |
| `fold-render` | Feature-neutral image IR, CPU reference evaluation, vector rasterization and offline audio plans |
| `fold-platform` | Static package registration, document/video/audio capabilities, commands and desktop host contracts |
| `fold-ui` | Private desktop backend, shared editor components, viewer/transport and presentation cache |
| `fold-timeline` | Sequences, tracks, clip editing, video/audio compilation and optional UI |
| `fold-compositor` | Image graph model, validation, compilation and optional UI |
| `fold-motion` | Procedural graph, geometry, fields, animation, authoring and optional UI |
| `fold-app` | Concrete assembly, cross-feature commands, workers, playback, desktop and CLI |

Creative packages are peers: none imports another's implementation. Project and
render do not import creative models. Cross-feature nesting uses stable document
references resolved within one snapshot and compiled into shared render IR.
Only app assembles concrete implementations. Cross-feature authoring operations
such as Create Composition stage one atomic batch and retain linked timeline audio.
Document registration does not require video metadata; timed output information
belongs to the video capability. Audio validation does not require a video output.

Creative packages have optional `ui` features. App has no default features;
`desktop` opts into the UI and native audio device dependencies. Static Rust
traits are in-process contracts, not a stable plugin ABI.

`scripts/check_boundaries.py` checks declared dependencies (including optional,
target-specific, renamed, dev and build dependencies), transitive dependencies,
and headless UI isolation. Its regression tests live beside it. This checks
crate ownership, not runtime semantics.

## Project state and editing

`Project` is the single writer. `EditBatch` uses an expected revision; the
coordinator validates envelopes, declared references and cycles before publishing.
Unchanged document roots are shared through `Arc`. Undo/redo publishes fresh
project revisions, preventing stale asynchronous proposals from becoming valid
again. History is bounded by entry count, not yet by retained bytes.

`CommittedSnapshot` is issued only by the coordinator. Persistence, application
export and playback require it. It dereferences to a read-only `Snapshot` for
provider queries and evaluation; `evaluation()` retains a worker handle without
persistence authority. There is no reverse conversion.

`EditSession` owns a transient proposal and generation. Updating it replaces the
staged mutations without publishing or adding undo entries. Its `snapshot()`
returns an immutable evaluation-only handle that remains valid after later updates
or cancellation. Desktop gestures use this same session mechanism; releasing a
gesture stages/commits the final command once, while cancellation drops the overlay.
Save/export always pin the committed root, never the active overlay. Compile-fail
tests enforce that evaluation handles cannot be passed to save/export.

Known document validation currently runs through registered package command
handlers; project core deliberately validates only generic state. Consolidating
all application publication paths behind typed/capability validation remains work.

## Evaluation and media

Timeline, compositor and motion compile to one full-frame image IR. Nested outputs
append graph fragments rather than render intermediate images. All providers use
one `VideoCompile` request carrying the snapshot, reference, exact time, dimensions,
cancellation token and nested resolver. Nested requests inherit cancellation;
provider dispatch, dependency walks, motion fields and scene traversal check it.
The CPU evaluator
validates graph ordering and parameters, evaluates only output-reachable work and
releases intermediate frames at their last consumer. Operations include media,
solid/vector sources, affine transform, opacity, crop, blur, grade, mask and over.

Working pixels are scene-linear sRGB, premultiplied RGBA32F, square pixels, with
explicit sRGB display conversion. Transforms currently use nearest sampling and
transparent borders. Vector rasterization uses 8-bit linear coverage/color before
expanding to float pixels. Delivery is opaque black-backed SDR; nested sources
retain alpha. This is a restricted reference path, not a general HDR pipeline.

Authored output validation is independent of codec restrictions. Odd raster sizes
and durations longer than 18000 frames can describe procedural outputs. The
verified MP4/H.264 adapter separately requires even dimensions and limits imported
sources/exported ranges to 18000 frames. It accepts a strict zero-origin CFR,
limited BT.709/yuv420p profile and rejects unsupported interpretations rather than
silently guessing. WAVE and video audio are prepared as stereo 48 kHz float PCM.
Video presentation and audio sample ranges share exact rational timing. Exact
256-entry input-transfer and quantization-threshold output-transfer tables avoid
per-pixel powers without approximating the existing RGB8 conversion semantics.

The CPU graph limits are 4096 nodes and 4194304 pixels per frame. `render()` allows
64 MiB of live working pixels; worker `render_with()` allows 128 MiB. Decoder,
source-copy, audio, vector and presentation budgets are separate. These are not
an aggregate process-memory/disk budget. There is no GPU evaluation scheduler yet.

Media workers retain verified temporary source copies and bounded frame read-ahead.
Audio workers prepare disk-backed PCM; the device callback consumes a bounded
ring without decoding or taking the project lock. The playback worker retains its
audio decoder across seeks/restarts and drops PCM sources absent from the next
plan before preparation. Device streams/rings remain generation-local.
Preview requests replace a bounded mailbox and obsolete results are rejected.

Motion document/video providers share a package-owned cache of validated immutable
roots and canonical evaluation identities. It retains at most four roots and
8 MiB of source payloads (not an exact heap-byte budget); larger documents bypass
retention. Full envelope equality invalidates entries. Parsing/evaluation never
holds the cache lock, and reference control overrides copy rather than mutate a
cached root. Time-dependent geometry is not cached by this mechanism.

Video still uses codec processes and temporary raw-frame files for bounded batches.
This remains a measured throughput bottleneck. Global budget enforcement and
long-running native performance acceptance are not complete; see
`consolidation-performance.md`. No real-time performance claim is implied.

## UI

The native window uses Dear ImGui, winit and wgpu. GPU ownership is currently for
presentation, not engine evaluation. `fold-ui` owns a byte-bounded display-texture
cache and protects in-flight textures from eviction. Layouts stay outside project
content; current docking layout is session-local.

Timeline, compositor and motion contribute package-owned panels. Compositor and
motion share the graph editor and Node Inspector window. Viewer transport and
review ranges are shared; review ranges are session state, not authored duration.
The shared `EditResponse` centralizes property activation, change, completion and
Escape cancellation. `UiId` scopes inspector identity by package, panel instance,
document and object; property paths are independent of labels. `NumericProperty`
provides scalar identity, units, step, optional limits and reset metadata. Motion
scalar/vector/color fields and compositor transform fields use it. Remaining
custom controls retain the trusted ImGui escape hatch; this is deliberately not
a full public extension schema or an animation-stack interface.

Delivery output is an undoable persisted `fold.output` document reference. The
Delivery panel can explicitly select the viewed document and reports the selected
output's frame range independently of viewer navigation. An unavailable explicit
selection errors instead of falling back. Older projects retain their previous
root-selection convention until the user chooses an output. Legacy two-layer
media documents register through the same capability/compiler path as sequences.

Decode, save/load and export run on workers. Some parsing, validation and content
identity calculation still occurs on immediate edit/refresh paths; responsiveness
must be measured rather than inferred from the existence of workers.

## Persistence and compatibility

The v1 `.fold` archive is a versioned JSON container with independent opaque payload
byte arrays. Unknown payload bytes, metadata, dependencies and manifest assets
survive load/save. The core neither migrates unknown data nor prunes assets.
Known schemas are interpreted by their owning packages.

Save writes and syncs a same-directory temporary file, replaces the destination,
and on Unix syncs the parent directory. `SaveError::DirectorySync` means the new
archive is visible but durability could not be confirmed. Interrupted temporary
writes preserve the old archive. Recovery retention, provider migrations, broader
platform durability and complete reproducible environment records remain work.

## Validation

During iteration, format touched files and check/test affected crates. At a
vertical-slice checkpoint:

```sh
python3 scripts/check_boundaries.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked
```

Desktop builds require the platform's development dependencies, including ALSA
on Linux. Keep machine-specific dependency paths outside repository configuration.
Native interaction/audio acceptance and release-build performance measurements
are separate from passing headless or synthetic ImGui tests.
