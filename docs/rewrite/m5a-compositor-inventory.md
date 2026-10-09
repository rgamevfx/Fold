# M5a compositor replacement inventory

The 28 pre-cutover operation/version entries below are all retained behind schema 8
native records. `fold_graph::model::Node` owns operation/version and stable typed
parameters; shared edges own image and mask connections. Shared tracks own
component animation and reusable overrides. `OperationConfig` is a transient
validated processing/Inspector projection, never persisted on a node. The prior
schema 7 static enum and Parameters-tag edit contract are removed.
See the [cutover evidence](evidence/m5a-compositor-cutover/result.md).

| Operation | Version | Fixed inputs | Output | M5a disposition |
| --- | --- | --- | --- | --- |
| Read | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Solid | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Transform | 1 | Image | Image | Native record/source; processing regressions retained |
| Crop | 1 | Image | Image | Native record/source; processing regressions retained |
| Blur | 1 | Image | Image | Native record/source; processing regressions retained |
| Grade | 1 | Image | Image | Native record/source; processing regressions retained |
| Mask | 1 | None / dynamic | Mask | Native record/source; processing regressions retained |
| ApplyMask | 1 | Image, Mask | Image | Native record/source; processing regressions retained |
| Merge | 1 | Image, Image | Image | Native record/source; processing regressions retained |
| Output | 1 | Image | Image | Native record/source; processing regressions retained |
| Shuffle | 1 | Image, Image | Image | Native record/source; processing regressions retained |
| RemoveChannels | 1 | Image | Image | Native record/source; processing regressions retained |
| Grade | 2 | Image | Image | Native record/source; processing regressions retained |
| Merge | 2 | Image, Image | Image | Native record/source; processing regressions retained |
| Transform | 2 | Image | Image | Native record/source; processing regressions retained |
| Blur | 2 | Image | Image | Native record/source; processing regressions retained |
| Exposure | 1 | Image | Image | Native record/source; processing regressions retained |
| Invert | 1 | Image | Image | Native record/source; processing regressions retained |
| Clamp | 1 | Image | Image | Native record/source; processing regressions retained |
| Premult | 1 | Image | Image | Native record/source; processing regressions retained |
| Unpremult | 1 | Image | Image | Native record/source; processing regressions retained |
| Rectangle | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Ellipse | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Input | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Effect | 1 | None / dynamic | Image | Native record/source; processing regressions retained |
| Dissolve | 1 | Image, Image | Image | Native record/source; processing regressions retained |
| Relative Transform | 1 | Image | Image | Native record/source; processing regressions retained |
| Shader | 1 | None / dynamic | Image | Native record/source; processing regressions retained |

The catalog contains 28 operation/version entries. Shader and Effect inputs are dynamic, supplied by their declared image interfaces.

## Removal and replacement audit

| Former authority/path | Replacement and verification |
| --- | --- |
| `Node.parameters: Parameters`, schema 7 | Shared versioned `Node.operation` and `Parameter` records; all 28 defaults round-trip in `native_authoring` |
| Schemas 1–7 execution/readers | Provider admits schema 8 only; generic Project preserves opaque payloads; unknown operation versions reject execution |
| Parameters-tag `add`/`create_node`/`replace` | Native `record` contract, discovered examples with deterministic example IDs; retired payload rejection test |
| Whole enum edits with no source scope | Canonical native source, optional node scope, expected-value conflict and protection; public `compositor_source` proposal/rebase/inspection/Apply |
| Static mask node UUID | Shared Edge ID/target/extensions retained on rewire; protected effect identity and values checked |
| Persisted host effect override map | Native Instance with Parameter/Track/Key IDs; host structure is a temporary shared Inspector/evaluation projection |
| Instance animation absent from Composition | Provider channels include instance occurrence/control/component targets; curve edits retain definition and identities |
| Independent rebase value merge | Shared conservative `fold-graph::reconcile` for properties, maps, nodes and parameters; provider semantic validation/protection stays feature-owned |

Every current `automation::Edit` path is retained on native records: scoped Source,
shader interface migration, create/replace/node patch, insert, duplicate, layout,
input color, connect/disconnect, remove/output/info and publication. The former
`Add` and Parameters payload are absent; there is no legacy dispatch alias.
Network creates from the same 28-entry registry defaults as discovery. Inspector,
provider animation, presets, nested Read, reusable publication and processing all
read admitted native records. Compositor has no Motion dependency.

Named channels, EXR and color metadata, alpha/premultiplication, effect mask/mix,
fractional media time, half-open ranges, dependencies, transforms, promoted
controls and shader interfaces retain their specialized processing implementations.
The workspace regression suites requalify those paths; no processing operation
is deferred. Earlier performance/physical acceptance limitations are unchanged.

## Existing regression fixtures to carry across the cutover

- `crates/compositor/tests/{animation,reusable,agent_graph,graph_authoring,channels}.rs` cover current authoring/evaluation behavior; they now author and evaluate schema 8 records.
- `crates/app/tests/common_composition.rs` covers mixed layers, explicit inputs, processing, animation, timing, nesting and undo.
- `crates/app/tests/motion_video.rs` covers Motion publication and compositor reads through exact named outputs.
- `native_authoring.rs` adds source/identity/unknown-version/protection/rebase/instance-curve regressions. `agent_project.rs` exercises public scoped source, disjoint rebase, inspection, undo/redo and save/reopen.
