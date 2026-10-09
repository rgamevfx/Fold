# Shared service and registration dispositions

Baseline `66ebb2ece465189a2f8dd3e415b38f63a62bdd6a`. This supplements the direct-reference
inventory with callers hidden behind feature-neutral interfaces. All removals below
belong to M1; replacement Motion registrations belong to M2–M4.

| Surface | Independent consumer evidence | Disposition |
| --- | --- | --- |
| `project/{model,coordinator,persistence,candidate}` | App, Composition, Timeline, compositor; project tests passed | Retain generic immutable snapshots, transactions, opaque persistence. New protocol must supply its own conflict/lifetime semantics. |
| `platform/authoring.rs` encode/decode/patch/schema helpers | Timeline `automation.rs`, compositor `automation.rs`, Composition `automation.rs` | Retain neutral encode/decode/patch mechanics. Delete old TypeScript generation, batch-reference example conversion, UUID reference schema expansion and unused schema inlining. Typed provider schemas retain literal UUID identities; compositor examples use concrete UUIDs. |
| `platform/image_source.rs` | Timeline `sequence_video.rs`, compositor `compile.rs`, Composition `compile.rs` | Retain image/output resolution and explicit source time. It uses baked textures independently of Motion. |
| `platform/image_bake.rs`, render `baked::Texture` | `image_source::compile` can resolve bake references for independent image consumers | Retain shared baked texture representation/decoding; delete app's Motion-specific bake command, its sole-use GPU BakeEncoder and Motion UI bindings. Audit shared command IDs and descriptions for obsolete authoring. |
| render `vector`, `vector_geometry`, `vector_instances`, GPU vector shaders | Compositor `shapes.rs` produces `Drawing`; its ImageOp::Vector executes through these caches/rasterizers | Retain compositor vector service and dependencies. New Motion must use the qualified M4 adapter, never use this retained service as an old-motion fallback. |
| render `textured::Draw`, `baked::Batch`, ImageOp::Textured/BakedTextured and GPU textured paths | Production construction only in Motion `geometry/render.rs`, `nodes/image.rs`, `nodes/image/bake.rs` | Delete specialized draw/instance/driver preparation and associated branches, caches, shaders and tests. Preserve `baked::Texture` and independently needed image sampling. |
| animation curves/interpolation | Timeline and Composition plus UI channel controls | Retain shared interpolation; remove old Motion keyed-parameter/animation ownership with the crate. |
| platform package/provider registry and app jobs | All creative packages, media/viewer/delivery | Retain explicit provider services; replace old protocol/candidate glue and remove concrete Motion models from app consumers. |
| project organization and app composition/layer services | Composition and Timeline | Retain non-motion mutations; remove Motion type branches, defaults, Rust asset creation and motion precompose delegation. |

## Indirect layer context closure

`composition/graph.rs` supplied old Motion object hierarchies through
`platform/layer_graph::EvaluationContext` and `VideoCompile::resolve_graph`.
The only provider implementing that contract was old Motion. Delete the context,
resolver, object synchronization and graph-child execution, along with old
object-selection fixtures. Composition continues to resolve ordinary document
outputs and edit provider animation; compositor image sockets retain `ImageInput`.
Removed `graph_children` data is retained by the existing flattened layer extension
bag. Unsupported document payloads remain opaque at the project boundary; no
replacement provider interprets old Motion object metadata.

## Dynamic registration closure

`app::packages::builtins` registers Composition, Timeline, compositor, Motion and
app commands. Remove the Motion registration entirely at M1. Its closure includes
both the ordinary document/video provider and `SolidProvider` (`fold.motion.solid`),
prepared cache, simulation runtime, panels, inspector, edit/change commands and
motion precompose command. Solid is an old Motion provider too; do not retain it
as a cutover placeholder. Rewrite `app/tests/foundation_checkpoint.rs` to use an
independent test provider rather than importing it.

App command registration removes `rust_asset::CREATE` and the Motion-specific
`image_bake::run`. Preserve transition, settings, read-source and composition/
placement commands after replacing concrete Motion branches. Inspect UI creation
menus and panel registration separately; provider absence must not leave actions
that report successful creation or blank rendering.

## Protocol, distribution and dependencies

All 19 method dispositions are in [contracts](contracts.md). Delete JavaScript
host/SDK (`automation/code.rs`, `skills/fold/sdk`), Rust compiler/SDK, Wrangles and
Solver. Embedded `include_str!` guides in `automation/mod.rs`, startup instructions
in `automation/mcp.rs`, CLI help/dispatch, Python agent-check scripts and active
README examples must change with their transports. Neither generic method names
nor retained non-motion users justify a v1 compatibility adapter.

`wasmtime`, Motion's `syn`, `rustybuzz`, `unicode-bidi` and app `rquickjs` are old
motion/SDK dependencies. Remove their direct declarations and let Cargo regenerate
the lockfile; do not remove a transitive crate still used independently. Parley
later supplies its own shaping closure. `tiny-skia` and `lyon_tessellation` remain
needed by compositor vectors. `block_compression` remains needed by shared baked
textures. App image encoding/sha2 dependencies require caller checks after removal.

The installed Fold skill entry is `.agents/skills/fold/SKILL.md`, in addition to
`skills/fold/SKILL.md`. Replace both and their shipped references under the Fold
skill workflow when public tools qualify. `scripts/package_fold.py` packages app
binaries and shared runtime/license resources, not a separate motion plugin;
compiled removal therefore controls shipped engine removal. Its source manifest
and packaged licenses must be re-audited after Cargo cutover.

## Demo archives

[demo-audit.json](demo-audit.json) records parsed document type counts for every
shipped `.fold` archive. All 14 contain `fold.motion.document`; all retire in M1,
including Timeline-named mixed projects. Associated creation scripts, guides and
rendered previews retire with them. Shared source media/fonts may remain if an
independent consumer is documented. User projects outside these repository fixtures
are untouched. M1 must construct fresh non-motion smoke projects through supported
services rather than pretending these mixed fixtures are independent.

## Cutover verification

Searches are discovery evidence, not sufficient deletion acceptance. After edits,
check manifests and resolved graph, dynamic registrations, included strings, every
transport schema and actual old-document open/save. Run workspace boundaries,
format/check/test/Clippy and native/headless non-motion smoke. A retained shared
service cannot execute an old motion payload. The M1 unsupported-provider test
must inspect diagnostics and preserved bytes, not merely assert that loading succeeds.
