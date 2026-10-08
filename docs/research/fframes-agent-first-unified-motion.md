# Agent-first unified Motion: fframes reference

> **RETIRED DIRECTION — 8 October 2026.** Historical research only. Follow the [Motion System Rewrite Plan](../Motion_System_Rewrite_Plan.md); recommendations below to extend or retain the former engine, Rust builders or protocols are superseded.

2026-10-08. Primary-source review and proposed requirements. This note does not implement or approve an engine replacement. Sources were read from upstream `main` through web retrieval; no single source revision was pinned. No fframes build or performance benchmark was run.

## Findings

fframes is Rust plus SVG. A `Video` supplies frame rendering through `render_frame`, which returns an SVG tree. The runtime resolves scenes and then evaluates frame requests. It caches static markup and supports CPU and Skia GPU rendering. This is a useful composition-language reference, but its per-frame tree is not a retained procedural geometry graph. [Video contract](https://github.com/dmtrKovalenko/fframes/blob/main/fframes/src/video.rs), [pipeline](https://github.com/dmtrKovalenko/fframes/blob/main/fframes/src/lib.rs)

Its creative vocabulary includes nested groups, text, shapes, fills, strokes, reusable Rust functions, numeric/color/transform animation, easing, springs, loops, staggered starts, and text measurement. These operations let an agent express design intent with familiar code. [API reference](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/references/api.md)

The agent workflow is as important as the syntax. It provides scene/time addressing, structural inspection, contact sheets, detailed frames, motion trails, draft renders, and machine-readable command output. Default inspection samples every 0.25 seconds plus scene endpoints; it does not inspect every frame by default. The skill requires visual inspection and distinguishes canvas overflow from text overflow inside its own box. [Agent workflow](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/SKILL.md)

The design guidance supplies pacing, hierarchy, typography, and review examples. This shows why an API list alone is insufficient: an agent also needs examples of good composition and a way to inspect motion. The examples are guidance, not universal rules or proof of aesthetic quality. [Design guide](https://github.com/dmtrKovalenko/fframes/blob/main/skills/fframes-video/references/design.md)

The inspected browser controller replaces a compiled video value and returns serialized SVG at a requested frame. It does not establish source-to-native-graph reconciliation. Upstream rendering speed claims are project claims, not measured evidence for Fold or a unified 3D engine. [Controller](https://github.com/dmtrKovalenko/fframes/blob/main/fframes-editor-controller/src/wasm_editor.rs), [README](https://github.com/dmtrKovalenko/fframes)

## Fold foundation

[Phase 56](../phase-56-live-rust-assets.md) already selects Rust/Wasm authoring, retained native constructions, stable controls, and explicit ownership. Code owns generated internals; artists own instance controls, connections, animation, placement, and surrounding construction. Unsupported structural edits produce conflicts. General source/manual-graph reconciliation is not implemented by that contract.

The existing [Rust SDK](../../crates/motion/src/program/sdk.rs) overloads arithmetic on `Signal` to construct retained expressions. This is a foundation to extend. Do not revert to the provisional JavaScript frontend in the earlier [authoring proposal](../mograph-authoring-overhaul.md).

## Proposed language requirements

These are recommendations for the unified 2D/3D rewrite, not claims about current capabilities.

1. **One semantic model.** Rust authoring and native nodes describe the same typed operations. Both produce retained geometry, attributes, fields, controls, and relationships. Preserve source/component identities. Do not expose Bevy ECS details as the creative language.
2. **Separate execution representation.** Lower the semantic graph into an optimized execution plan. Fuse compatible fields, retain shared buffers, and choose CPU/GPU work there. Preserve mappings back to authored operations for diagnostics. Optimized kernels need not appear as artist nodes.
3. **Symbolic fields.** Author-time Rust constructs field expressions. It does not execute arbitrary Rust for every point on every frame. A retained copy operation represents many instances without expanding one node per copy. Custom Rust/Wasm remains an explicit bounded capability with honest CPU fallback costs.
4. **Explicit meaning.** Declare geometry type, attribute type/domain, coordinate space, units, color encoding, rational time, local clocks, and randomness seeds. Keep curves/fill/stroke editable alongside meshes. Define attribute interpolation and transfer when topology changes.
5. **Creative vocabulary.** Supply text measurement and layout, paths and appearance, distribution, shared instances, camera placement, extrusion, timing, easing, stagger, and reusable components. Prefer general operations that compose over recipes tied to one reference.
6. **Discoverable contracts.** Generate machine-readable operation schemas, SDK definitions, documentation, and inspector metadata from package-owned definitions. Include defaults, supported domains, limits, examples, and diagnostics. Distinguish supported operations from proposals.
7. **Predictable revision.** Keep stable component and parameter IDs. Preserve artist-owned values, keys, and connections through source updates. Return specific ownership/interface conflicts. Avoid promises to reconstruct original loops or functions from arbitrary graph edits.
8. **Complete feedback.** Support reference inspection, a brief with named moments, staged construction, a semantic diff, representative frames, motion inspection, revision, and one undoable publication. Tie observations to an exact snapshot and time. Diagnose hidden/empty output where evidence permits; report uncertainty elsewhere.
9. **Visible execution costs.** Explain CPU fallbacks, geometry expansion, uploads/readbacks, memory, invalidation, and timing by authored component. Measure cold compilation, edit-to-preview latency, warm playback, and export separately. GPU use alone is not a performance result.

## Acceptance experiment

Give a fresh agent a short reference and a blank project. Require editable vector text, strokes, repeated mesh geometry, shared attribute animation, and a camera in one construction. Permit public discovery and observation tools only. Have an artist change exposed controls, then ask the agent to revise timing without losing those changes.

Review the entire animation and loop/transition boundaries. Verify native editability, meaningful controls, save/reopen, undo, deterministic seeking within declared numeric tolerances, and preview/export agreement. Measure repair count, tool calls, source-edit latency, frame-time distribution, and memory on modest hardware. Include new briefs beyond the supplied example before claiming general agent success.

The reusable lesson from fframes is the combination of a coherent vocabulary and an effective observation loop. Fold should apply that lesson to its retained 2D/3D model and compiled execution plan.
