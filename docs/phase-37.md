# Phase 37 — Count animation and editable tool interfaces

Count was authored through the shared integer property widget, which forcibly changed every inserted or edited key to Hold. A UI regression reproduced 0 at the halfway point between 0 and 120. Removing that override restores default Linear interpolation and preserves deliberately authored interpolation. Generator count validation also rejected fractional sampled values; valid counts now round to the nearest whole copy at generation, keeping finite/range checks.

## Workflow

- Right-click Copies → Interpolation → Linear repairs an existing Hold curve. The menu applies to the property's whole curve (or one component's curve when opened on that component). Saved Hold animation is not silently migrated.
- Right-click a property → Expose input reveals its actual socket without changing its value or creating another evaluator. Visibility persists on the node, with undo. Connected sockets remain visible.
- Inside a group, right-click a literal property → Promote to group input publishes that property through the existing group authoring path.
- Right-click a group node → Edit contents or Edit node interface. Interface editing is also available from the inspector menu and the internal Network's Interface menu.
- The interface editor renames the shared definition/controls, edits defaults and sections, and adds Number, Vector, Color, Toggle or Text inputs. Each added input has a stable ID and corresponding internal Group Input node; wire it into the construction explicitly. Make unique remains available before changing only one instance.
- Math adds Round (nearest, ties away from zero), Floor, Ceil and Truncate (toward zero), evaluated componentwise. Unary operations ignore the second operand and hide it in the inspector. Outputs remain compatible numeric values; a separate integer socket type is unnecessary.
- Example: Wave (Amplitude 60, Center 60) → Math / Floor → exposed Copies produces 0–120 copies. Count retains its supported nonnegative range; Map Range can constrain a more general driver.

## Verification

- UI reproduction failed before the fix (`Some(0)` instead of `Some(60)`) and passes afterward.
- 60 Motion tests pass, plus a new focused test covering socket visibility, added interface inputs, stable names/IDs, reload and undo (61 total).
- 76 shared UI tests pass; three native-render tests are ignored by the default run.
- Focused evaluation covers fractional/interpolated counts, Wave → Math → Count, and all four rounding modes on negative/positive vector components.
- Final release desktop build passes. All 18 Motion library tests, including the explicit ImGui/WGPU render test, pass after the final UI changes. Reviewed normal/narrow inspectors, the actual right-click Copies menu (`/tmp/fold-count-menu-37.png`) and group interface editor (`/tmp/fold-group-interface-37.png`). The first render found a missing popup ID on the plain-text group title; explicit scoped popup IDs fix that assertion. Native desktop interaction acceptance remains unverified.
- Full workspace checks and Clippy remain deferred. The running app and unsaved user animation are not restarted or modified.
