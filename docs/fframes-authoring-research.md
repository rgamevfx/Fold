# fframes authoring research

Investigated 2026-10-07 for Fold's mograph authoring overhaul. Primary documentation and source were inspected through GitHub/raw source. The repository tree API resolved `main` to `30b48f3da60040e0fd7c811fdcd5b159e70a5b02`; source page reads used `main`, so this is a discovery revision, not a claim that every web-cached file was pinned to it. No renderer build, artwork run, or performance benchmark was performed. The clone did not complete and the source archive was incomplete.

## Decision

**Recommendation:** take substantial inspiration from fframes' composition language and review loop, but do not replace Fold's document and renderer with fframes on the evidence gathered. fframes makes frame-producing code pleasant to compose. It does not establish that arbitrary author code can become a persistent, independently art-directable node graph with stable identity, undo, and reconciliation after manual edits. That missing bridge is the central Fold requirement.

## What agents actually author

fframes is **Rust plus SVG**, not a TypeScript/React video DSL. `Video` is a `Sync + Sized` Rust trait with size/FPS constants, duration, audio, optional scenes, and a `render_frame` method returning `Svgr`. Scenes are referenced Rust objects. This is executable frame logic, not a command stream for mutating a saved graph. [Video trait and scene resolution](https://github.com/dmtrKovalenko/fframes/blob/main/fframes/src/video.rs)

The conceptual contract is:

```text
video data + frame/time → SVG tree → rasterizer → frame
```

The library's minimal example computes animated opacity inside `render_frame`, then interpolates that value and the video's title into a nested SVG/text tree. Frame evaluation may run concurrently; documentation requires pure evaluation with data prepared beforehand. Static SVG subtrees can be hashed at compile time and cached; dynamic expressions remain evaluated. CPU rendering uses tiny-skia, while the separate Skia backend provides GPU rendering and shader layers. These are pipeline facts, not independently measured performance results. [Pipeline and minimal example](https://github.com/dmtrKovalenko/fframes/blob/main/fframes/src/lib.rs)

Composition comes from ordinary Rust data, functions, and collections. SVG children can be another `Svgr` or a `Vec<Svgr>`, so helpers and iteration naturally produce nested reusable artwork. SVG groups provide hierarchy; definitions provide gradients, clipping, masks, and filters. The animation API supports numeric values, colors, transforms, easing curves, springs, looping, and runtime start times for staggering. Text APIs measure width, fit text, and break lines. These reduce the amount of geometry and timing an agent must invent manually. They do not imply a general automatic layout engine. [API reference](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/references/api.md)

Small API sketch, paraphrased from that reference rather than a tested standalone program:

```text
opacity = sample(ease_out(0 → 1, 0s → 0.5s), frame)
return SVG.Group(opacity, children = items.map(render_card))
```

The useful semantic property is that editing the declared artwork changes the next evaluated result. Fold should preserve that property in its author-time compilation even though its output is a native document rather than per-frame SVG.

## Agent effectiveness is a feedback system

The shipped agent workflow is a skill plus each generated project's CLI. It prescribes a repeating sequence: inspect timing, run structural diagnostics, view a contact sheet, inspect detailed frames, inspect motion trails, and make draft renders. Named scene/time addressing avoids manual frame arithmetic. Its diagnostics distinguish technical validity from appearance; the skill explicitly requires opening rendered PNGs and notes that canvas-bound checks do not prove text fits its intended box. There is also audio analysis and snapshot comparison. [Agent workflow](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/SKILL.md)

This inspection found no MCP-specific entry in the recursive repository file listing. That is a limited observation, not proof that no integration exists elsewhere. The documented primary agent interface is local source editing plus CLI commands. The README's render-time and GPU speedup numbers are project claims, not a transferable Fold benchmark. [Repository and README](https://github.com/dmtrKovalenko/fframes)

**Recommendation:** copy the complete creation–inspection–revision loop, including scene/range targeting, contact sheets, temporal diagnostics, and actionable layout failures. A larger low-level tool catalog alone will not supply this loop.

## What the editor does and does not establish

The inspected WASM controller owns a compiled `Video`, resolves scene/audio metadata, accepts media, can replace the complete video value, and returns serialized SVG for requested frames. Its code demonstrates interactive parameterized playback, but does not expose a persistent graph of editable artwork nodes or a reconciliation algorithm between source declarations and artist property edits. Do not interpret its browser timeline as evidence that this problem is already solved. [WASM editor controller](https://github.com/dmtrKovalenko/fframes/blob/main/fframes-editor-controller/src/wasm_editor.rs)

**Inference:** a wholesale integration could give Fold a procedural video layer, but an opaque layer would weaken the required editability. Recovering arbitrary Rust control flow and frame-dependent SVG into stable native authoring objects is a separate compiler/product problem. Selective inspiration is therefore lower risk than a renderer rewrite.

## Recommended Fold adaptation

These are design recommendations, not existing fframes capabilities:

1. Let agent code return a nested declarative composition with stable keys, groups, layout, text, reusable components, and typed motion. Compile it at author time into Fold-owned native nodes and relationships.
2. Keep expressions representing time-varying properties as explicit native animation/expression data. Avoid requiring arbitrary user code to run on every preview/export frame.
3. Keep the native document authoritative. Record the last generated value, current native value, and new declaration for each owned property; reconcile changes and surface conflicts rather than silently overwriting artist changes or ignoring new constructor arguments.
4. Preserve component boundaries and expose intentional controls. Show a meaningful artist-facing group and parameters, while retaining inspectable internals; avoid generating hundreds of disconnected anonymous utility nodes.
5. Make a small set of end-to-end author/inspect/revise operations the primary MCP surface. Keep low-level graph operations as an escape hatch.

The host language is secondary to these semantics. Fold's existing bounded JavaScript evaluator offers a practical first authoring host, with typed declarations/documentation layered over it. Rust builders could also emit the same author-time composition representation, preserving the project's Rust logic rule. That alternative needs an explicit compiler/cache/process or WASM isolation strategy and longer edit/build feedback; ordinary compiled native Rust is not automatically a bounded scripting environment. Neither approach requires adopting fframes' per-frame renderer.

## Licensing and reuse

The repository license is MIT and permits reuse subject to preserving its copyright and permission notice. This makes selective source reuse plausible, but is not a dependency-by-dependency clearance. [Repository license](https://github.com/dmtrKovalenko/fframes/blob/main/LICENSE.txt)

The library documentation separately identifies FFmpeg linkage and codec feature flags acknowledging GPL, nonfree, or version-3 terms. Therefore the repository's MIT license should not be read as the license of every enabled codec/dependency combination. Fold should assess any proposed imported crate and feature set before incorporation. [Feature and linkage documentation](https://docs.rs/fframes/latest/fframes/)

A useful first implementation test is a native editable title sequence with reusable cards, a typography hierarchy, staggered entrances, and a manual artist color adjustment that survives a source-driven timing revision. Measure author iterations and time to a polished result, not only whether graph construction succeeds.
