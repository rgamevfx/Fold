# Phase 40 — Scene tree drag and drop

Dragging an object onto the center of a group row parents it to that group. Dropping on a row edge inserts it before or after that object, including moving between groups. A temporary Scene root target and empty space below the tree let artists move children back to root.

The tree outlines a valid group target and draws an insertion line for a sibling target. Hovering a group for 600 ms expands it. Escape or releasing outside a valid target leaves project data unchanged. Parenting preserves current world placement through the existing affine offset; keys and procedural construction remain editable. Existing Move into group menu actions use the same placement operation.

Objects, text, paths and nested groups use the existing scene hierarchy. Modifiers remain reorderable within their owner. Self/descendant cycles and locked destinations are rejected; full Motion validation also rejects procedural reference cycles. Each successful drop is one transaction and one undo step. Scene children remain live inputs, so a new path placed in Badge immediately participates in every Duplicator copy.

Implementation stays in the shared animation tree and Motion authoring package. Hovering performs only hierarchy checks; evaluation and project cloning happen on drop. No new project schema or dependencies.

Validation:

- Twelve focused shared animation-editor tests pass, including actual ImGui mouse press/move/release, hover expansion, one release response and Escape cancellation at 900 px and 390 px widths.
- Focused Motion regressions cover translated-group placement, cross-parent insertion, root removal, atomic cycle/lock rejection, serialization and single-step undo.
- The Badge/Duplicator regression confirms adding one path adds exactly one drawing to each of four copies.
- Native-renderer normal/narrow captures reviewed, including the live group-drop outline and temporary Scene root target. Live desktop gestures remain unverified. Workspace checks and Clippy are deferred.
- Commit checkpoint: 73 Motion tests and 78 shared UI tests pass. Four opt-in tests are ignored in the default run; the Motion native-renderer test passed separately as recorded above.
- Release desktop build, touched-file rustfmt check and final diff whitespace check pass.

Launched the rebuilt desktop with `/tmp/fold-badge-color-ramp.fold` on the shared GTX 1070 Vulkan backend at the user’s request. The existing project required no conversion. Phases 34–40 were subsequently accepted as complete by the user; the verification limits above remain applicable.
