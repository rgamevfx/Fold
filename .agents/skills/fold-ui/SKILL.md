---
name: fold-ui
description: "Use when changing Fold UI: panels, controls, labels or artist workflows, including UI within larger features."
---

# Artist-first UI

Artwork is primary. Existing clutter is not a design precedent. Use Dear ImGui
through the shared UI SDK, semantic theme tokens, property metadata and command
lifecycle. For cross-workspace semantics, consult the [system map](../../../docs/architecture.md).

## Design rules

- **Quiet chrome:** one toolbar row by default, two at most. Use overflow at
  narrow widths; move secondary actions to menus, contextual controls or inspectors.
- **Direct editing:** common work starts on the canvas or working surface. Graphs
  expose the same editable construction, not a second model. Keep numeric
  alternatives to dragging and retain advanced operations behind compact controls.
- **Readable controls:** use shared icons for familiar actions, action/shortcut
  tooltips, visible parameter labels and established VFX/animation terms. Preserve
  stored precision and exact time independently of display precision.
- **Hierarchy:** group related properties, align labels/values and distinguish
  authored, animated and evaluated state. Show the actual edit target and time
  context. Prefer one animation affordance/menu to repeated Animate/Publish rows.
- **Accessible density:** remove redundant labels/containers before shrinking
  text or hit targets. Preserve keyboard access, focus, non-color cues and DPI scaling.
- **Useful feedback:** controls and artwork show ordinary edits. Add notices only
  for a failure, consequence, required action or meaningful background completion.
  Routine feedback must not resize the working surface. Show stale/degraded
  preview state and essential warnings without relying on hover. Internal IDs
  and edit-session diagnostics belong in diagnostic views.

## Apply and review

1. Identify the artist task, shortest common workflow and shared component to reuse.
   Decide what is primary, contextual and advanced before adding widgets.
2. Use the shared preview/commit/cancel lifecycle: a drag is one Undo entry.
   New engine capability does not automatically earn a toolbar button.
3. Review normal and narrow sizes: toolbar height, labels, tooltips, focus,
   selection/animation state and access to advanced controls. Exercise the common
   workflow, Undo and cancellation. Report visual or interaction checks not performed.

Every visible addition must serve a task or meaningful state. Screenshots and
widget tests do not establish native interaction or speed acceptance.
