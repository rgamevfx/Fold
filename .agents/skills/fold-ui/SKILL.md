---
name: fold-ui
description: "Use when creating or changing Fold UI: panels, toolbars, inspectors, controls, artist-facing names, or workflows. Apply before implementation and during visual review, including UI added as part of a larger feature."
---

# Fold UI: artist-first, quiet chrome

Deliver creative power with few steps and little visual noise. The artwork is primary; the interface supports it. Existing clutter is not a design precedent.

## Product grounding

Source: `docs/Fold_Application_Product_Architecture.md`, especially §§2, 10, 13–15 and 18. It is a product/architecture proposal, not evidence that a capability is implemented. Consult the relevant section when changing workflow or interaction semantics; this skill does not replace its contracts.

- Editing, Compositing, and Motion are peer workspaces over one project and selection model (§§3, 14). Keep shared viewer, inspector, transport, and navigation patterns consistent.
- Basic motion design starts on the canvas: Content → Distribution → Driver → Influence → Response. Reveal the same editable construction in the graph when precision is useful; basic work must not require graph wiring (§13).
- Direct controls and shortcuts preserve editable intent. Cross-workspace operations retain timing and references rather than flattening content (§§2, 10).
- Property stacks distinguish the authored base, contributions, and evaluated result. Keep animation targets and scope understandable: Whole object, Each copy, Each glyph, Each path point (§13).
- Use the application-owned UI SDK, shared property metadata, semantic theme tokens, icons, and command/edit lifecycle. A drag previews changes and commits one undo entry; cancellation restores the prior state (§§4, 15).

The compactness rules below are additional product taste guidance, not quotations from the architecture.

## Design rules

1. **Quiet chrome.** Panel toolbars use one row by default, two at most. Move secondary actions into menus, contextual controls, or the inspector. At narrow widths, use overflow rather than stacking more toolbar rows. Keep the canvas, graph, timeline, or viewer dominant.
2. **Purposeful visibility.** Each visible element must support the current task, navigation, or meaningful state. Put internal IDs and raw diagnostics in diagnostic views; legal notices in About / Third-party licenses as required. Put gesture instructions and explanations in tooltips or help. Routine success messages need not occupy permanent panel space.
3. **Recognizable controls.** Prefer shared icon buttons for familiar actions, with tooltips giving the action name and available shortcut. Use concise text when an icon would be ambiguous. Keep meaningful parameter labels visible: tooltips replace explanations, not basic comprehension. Provide keyboard access and clear focus.
4. **Artist language.** Use established editing, VFX, and animation terms, with consistent names across workspaces. Translate schema fields into readable labels with useful units and precision. Present alignment as alignment controls, not instructions for decoding numeric values. Display precision must not reduce stored precision or authoritative time accuracy.
5. **Fast paths, retained depth.** Offer sensible defaults and direct actions for common outcomes; retain primitive operations through expandable detail, menus, or the graph. Convenience controls edit the same underlying construction. Keep advanced controls discoverable rather than deleting creative capability to simplify the screen.
6. **Context and hierarchy.** Group related properties; align labels and values. Keep frequent controls prominent and occasional actions secondary. Prefer a compact animation affordance and parameter menu to repeated rows of Animate/Publish buttons. Make driven/keyframed state and the actual edit target clear.
7. **Compact, not cramped.** Remove redundant labels, headings, and containers before shrinking text, spacing, or hit targets. Use readable type, DPI-aware shared components, restrained semantic color, and non-color state cues. Preserve numeric alternatives to dragging.
8. **Honest state.** Keep source/local-time context, relevant scope, missing dependencies, pending work, and preview quality/approximation status visible when applicable (§14). Essential warnings and errors must not depend on hovering. Minimalism must not make a stale or degraded preview appear current.

## Apply to each UI change

1. Identify the artist's task, common shortest workflow, and the existing shared component or interaction pattern to reuse. Decide which controls are primary, contextual, and advanced before adding widgets.
2. Implement within that hierarchy and the product contracts. A new engine capability does not automatically earn a toolbar button or inspector section. Stay within the requested change rather than redesigning unrelated panels.
3. Inspect the affected UI at normal and narrow panel sizes. Check toolbar height, readable labels, tooltips, focus, selection/animation state, and access to advanced controls. Exercise the common workflow and relevant undo/cancel behavior. Report anything not visually or interactively verified.

Finish only when every added visible element has a task or state purpose, primary controls can be understood without hovering over everything, and the fast path preserves editable depth.
