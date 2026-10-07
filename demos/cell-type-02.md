# LIFT / bead conveyor

A native 500 × 888 Composition. Orange, cyan, and citrus beads spell LIFT, rising over a dark dot field. Retained circles have shadow, rim, face, and glint geometry. A small native horizontal sway moves the complete layer. The six-second document contains two matching three-second loops at 25 fps.

Open `cell-type-02.fold`, select the Rust asset layer, and edit its native group controls:

- **Word**, **Type size**, **Cell spacing**, and **Bead radius** change the typography.
- **Row spacing** changes the repeated row interval.
- **Coral**, **Cyan**, and **Citrus** change the three alternating bead paints.
- **Rise speed** is connected to the root graph's **Conveyor speed** Published Control. Edit that driver’s `default` rather than replacing its connection. The saved value is 100 px/s; row spacing is 300 px, giving a three-second conveyor loop.
- The Composition layer has seven native `position_x` keys across 0–6 seconds, making two repeated sway cycles.

Use the inspector's **Edit Rust source** and **Compile and apply** to revise generated geometry. `cell-type-02.rs` is the final authored source. Its GLOW/160 px/80 px/s defaults intentionally differ from the saved instance's artist edits LIFT/190 px/linked 100 px/s: the test proved that recompilation preserved those edits, the driver connection, and key identities. Defaults affect new instances.

`cell-type-02.png` is a full-resolution intermediate frame. `cell-type-02.gif` is the complete three-second loop at 25 fps. The saved project has been reopened and rendered through the public Fold CLI/MCP transport. Evidence and known limits are in `docs/evidence/phase-56/trial-02/report.md`.

Changing word length, size, spacing, or speed may require checking clipping and loop timing. The overscan was verified for the saved LIFT layout, not every possible text/control value.
