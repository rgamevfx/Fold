//! Compositor-owned typed graphs, node commands, and plan compilation.
//! Reference other documents through shared IDs and capabilities, not peer models.
//! Mutations go through project transactions; evaluation uses immutable snapshots.
