# Fold implementation ownership

Updated 9 October 2026 for maintenance C9. This maps implemented ownership; it
is not a release or artist-acceptance claim. The [Motion rewrite plan](Motion_System_Rewrite_Plan.md)
owns Motion gates, the [maintenance plan](Codebase_Maintenance_Plan.md) owns C0–C9,
and the [product specification](Fold_Application_Product_Architecture.md) owns
shared product requirements. [Current handoff](rewrite/HANDOFF.md) records blockers.
Historical phase documents are evidence at their original scope, per the
[retirement register](Retired_Documentation.md).

## Package boundaries

| Package | Owns |
| --- | --- |
| Foundation | Stable IDs and checked rational time |
| Animation | Shared curves, stable keys, interpolation and exact timing maps |
| Graph | Native typed records, expressions, tracks, source primitives and reconciliation utilities |
| Color | OCIO configuration identity and CPU/GPU processor resources |
| Native Video | Helper protocol, native device/plane ownership and readiness |
| Project | Opaque document envelopes, assets/items, immutable snapshots, transactions, undo and archive persistence |
| Media | Ingest, source pinning, decode/audio/EXR preparation, encode adapters and bounded media resources |
| Render | Feature-neutral image/scene requests, CPU reference execution, GPU execution, text/vector/spatial adapters, admission and resource accounting |
| Platform | Static capability registry, optional provider services, desktop/workspace contracts and output orchestration interfaces |
| UI | Shared Dear ImGui SDK, shell/workspace routing, widgets and viewer presentation |
| Timeline | Sequence, tracks/clips, editing, audio/video compilation and optional panels |
| Compositor | Native image graph, validated preparation, operations/channels, authoring and optional panels |
| Composition | Common layer hierarchy, timing, placement, Track/Curve editing and provider-output assembly |
| Motion | Native Construction/Scene, typed source, operation registry, reference evaluation, geometry/attributes, bounded Rhai Wrangle and optional panels |
| App | Static assembly, explicit cross-document commands, Session coordination, workers, output rendering and public transports |

[Cargo boundary policy](../scripts/check_boundaries.py) checks declared and
transitive dependencies, including optional/dev dependencies and headless UI
isolation. Creative UI uses the optional shared SDK. UI's Project/Media dependencies
serve neutral settings and thumbnails and are explicitly permitted. Composition
resolves provider outputs without importing Motion or Compositor. Motion may reuse
Composition's layer-state contract. Project and Render contain no creative document
models. [Scoped ownership checks](../scripts/check_proposal_ownership.py) additionally
constrain generic App dispatch and provider proposal modules within these crates.

App has no default features. `gpu` enables headless GPU execution; `spatial`
adds the spatial adapter; `native-video` adds native media integration; `desktop`
enables the shared UI and audio-device integration. `native-probe` adds opt-in
acceptance instrumentation. The separate CUDA helper feature needs its native
SDK and hardware checkpoint; it is not enabled by ordinary CI's desktop build.

## Project and application authority

[Project](../crates/project/src/coordinator.rs) owns atomic publication and history.
Providers validate creative meaning; core validates neutral envelope/reference
integrity. Committed snapshots are immutable and required by save/delivery.
Transient edits retain separate roots/generations and commit once or cancel;
the committed revision alone cannot identify a transient result.

[Session](../crates/app/src/session.rs) remains the single application coordinator
and committed project writer. `session_commands` executes commands with explicit
outcomes, `session_jobs` owns file/export job kinds and lifecycle, and
`session_transport`/`viewer_transport` own transport coordination. Worker completion
requires the current project generation and relevant revision. Project replacement
cancels jobs but keeps their admission occupied until acknowledgement. Save refreshes
dirty/history state without redoing expensive output preparation. Output identity,
preparation and immutable render requests live in `output`, `output_compile` and
`output_scene`, not a second media-workflow coordinator. See [C5](maintenance/evidence/c5/result.md).

[Platform packages](../crates/platform/src/packages.rs) require document identity,
support and validation. Optional authoring, parameters, animation, hierarchy,
placement/settings, image-input and proposal capabilities advertise actual support.
Missing capability is explicit. Typed failure categories drive cancellation and
retryable pressure; error wording cannot change scheduling. Oversized requests
are distinct from temporary pressure. See [C2](maintenance/evidence/c2/result.md).

## Provider editing and public access

[App agent dispatch](../crates/app/src/agent/dispatch.rs) owns request validation,
admission, exact-byte retry identity, jobs and observation scheduling. Provider
`proposals` modules own creative parsing, preparation, bounded inspection,
reconciliation, protection and diagnostics. Generic dispatch does not parse
Motion/Compositor models. Explicit cross-document assembly remains App policy;
[Project proposals](../crates/project/src/proposals.rs) own immutable candidate
lifetime and atomic publication. Inspection receipts, base/epoch/generation checks,
protected controls and fresh inspection after rebase constrain Apply. Active artist
gestures cannot be overwritten. See [C4](maintenance/evidence/c4/result.md).

Linux `fold-cli protocol <socket>` and `fold-cli mcp <socket>` expose the same
version-2 request/schema through `fold_request`; the old `agent` command is absent.
Discovery supplies `request_schema`, `project_authoring.schema` and each provider's
`scoped_proposals[].requests`. Schemas are generated from the current Rust contracts,
not separately maintained compatibility files. [The Fold skill](../skills/fold/SKILL.md)
and its [public workflow](../skills/fold/references/public-workflow.md) describe
supported authoring, jobs, source inspection, proposal/Apply and save-copy.

Parameter inspection/resolution/sampling uses stable document/object/occurrence/
parameter/component targets and exact source clocks. M5c does not yet implement
cross-document expression bindings: discovery reports `cross_document: false`.
Full reusable-effect occurrence binding and universal UI editing remain open.
Public observations are revision/time-bound; a rendered frame does not establish
physical artist continuation or complete export acceptance.

## Creative state and evaluation

Timeline owns Sequence 5; Composition owns layer schema 3; Compositor owns native
schema 8; native Motion Construction and Scene use their distinct schema-1 types.
Grade/Transform/Blur use version 3; Merge uses version 2.
The [first curated node set](rewrite/evidence/compositor-nodes-first-set/result.md)
replaces their temporary inspectors and Gaussian-only Blur contract. Old Motion, two-layer Timeline and retired
schema generations are not executable. Unsupported records remain opaque.

Composition combines independent scene, image and nested outputs through named
provider inputs. Common layer identity, lock/visibility, parenting, timing and
animation are authoritative. Motion scene objects own procedural graphs; common
Outliner, Network and Inspector project their native identities. Composition owns
Track/Curve editing, including occurrence clocks and overrides. There is no old
Rust-asset compiler, WASM builder SDK or Solver runtime. The current bounded Rhai
Wrangle belongs to the replacement geometry/attribute evaluator (M5b).

[Prepared Compositor records](../crates/compositor/src/model.rs) are immutable,
validated views of identity-bearing native records with typed operation configs and
stable-ID indexes. Sampling and execution share that representation; unknown records
are preservable but non-executable. C8 skips empty animation-path table construction
after track/identity validation. It adds no retained cache. [Measured gains](maintenance/evidence/c8/result.md)
are local preparation gains only, not UI refresh or native latency improvements.

Motion's current reference evaluator owns time and scene/geometry semantics.
Full incremental dependency compilation, GPU fields and the broader observation
loop remain gated M6 work. Bevy runtime state stays behind the rendering adapter
and is reconstructible; it never owns documents or playback time. Text shaping
uses the Fold text service. Preview, export and headless rendering share immutable
requests and evaluation semantics, with explicit output, rational sample time,
resolution, color/alpha and cancellation. Agreement requires matching requests and
the fixture's declared tolerance.

## GPU, media and resource lifetimes

[GPU request](../crates/render/src/gpu/request.rs) owns validation, planning and
leased source preparation; `encoding` owns operation encoding and scratch/input
leases; `submission` owns admission, submission and completion publication;
`frame` owns immutable frames/readiness and worker-only readback; `statistics`
owns accounting. One retained vector pipeline serves supported vector inputs.
Explicit CPU reference execution and independently useful image kernels remain.
[C7](maintenance/evidence/c7/result.md) records vector coverage and native results.

[Host](../crates/render/src/gpu/host.rs) shares device/queue, pooled images and
per-kind accounting. Resource reuse requires GPU completion and release of leases,
not CPU reference counts alone. Cancellation/health are checked after admission and
before submission. Map callbacks record mapped state; decoding after callback
observation checks shared device failure under the host synchronization gate.
Reservations survive decoding or abandonment. Device destruction, cancellation and
preview/export contention tests protect this boundary. The inherited extrusion-depth
oracle failure remains open with its original tolerance.

Bounded preview workers retain per-consumer demand/generations; source preparation
and audio have separate lifetimes. Presentation caches retain display-transformed
pixels under byte limits. BC7 playback retention is lossy, completion-gated and
separate from working/source images. Export never consumes a viewer texture.
Cancellation and pressure handling preserve admission and account for held resources.
Logical budgets, process RSS and driver/decoder allocations are different measurements.

The pinned color runtime uses OCIO 2.4.2, ACES 1.3 Studio config 2.2.0 and
ACEScg working space. Input interpretation, viewer display and delivery transform
have separate scope. Nonlinear transforms preserve alpha via explicit unpremultiply/repremultiply.
Finite negative/above-one RGB is valid working data. Media retains source fingerprints,
verified ownership and bounded decode ranges. Native hardware support requires
qualification with the pinned helper, device and fixture; software Vulkan is not
physical NVIDIA qualification. Audio preparation runs off the callback/UI thread.

## Desktop ownership and persistence

[Shell](../crates/ui/src/shell.rs) owns one runtime record per stable viewer/panel
identity. `shell_workspaces` owns docking, instance reconciliation and save/restore;
`shell_linking` owns selection/navigation; `shell_delivery` owns the export form;
`shell_viewer` draws from explicit state. Playback workers remain in Session.
Closing/replacing/retargeting panels clears the relevant queued state; late callbacks
cannot recreate a closed binding. Focus, following, pinned outputs and linked
selection are distinct. See [C6](maintenance/evidence/c6/result.md).

Workspace v4 sidecars use generation-guarded asynchronous I/O. Empty layout means
default docking and is never passed to ImGui's zero-length C-string loader. The
empty-layout restore SIGSEGV is causally fixed; the original unknown-trigger freeze
and separate C0 unbound shutdown SIGSEGV are not thereby resolved.

[Archive persistence](../crates/project/src/persistence.rs) accepts/writes version 3
only. Unknown payload bytes, extensions, dependencies and assets survive save/reopen.
Unsupported containers fail without rewriting originals. Save uses a temporary sibling,
sync, atomic replacement and directory sync; a directory-sync failure distinguishes
visible replacement from confirmed durability. Save/delivery pin committed state.
No old-provider migration reader or workspace upgrade is shipped.

## Qualification

[C9 evidence](maintenance/evidence/c9/result.md) records current commands, ignored-test
prerequisites, native results and unresolved gates. CI checks formatting, boundaries,
scoped ownership, serial workspace tests, desktop/headless builds and Clippy.
Warnings are reported; a successful Clippy run is not a warning-free claim.
Native color/video/helper tests need their recorded dependencies and fixtures.
Offscreen GPU interaction, desktop presentation submission and physical artist
acceptance are separate. C9 cannot close while required checks or freeze acceptance
remain unresolved. M3–M5/M5a–M5c remain open and M6 remains gated.
