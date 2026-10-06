# Rust Orbit

Open [fold-rust-orbit.fold](fold-rust-orbit.fold) in the Motion workspace. It is an editable 640×360, 24 fps example built entirely through Fold's public agent API.

- **Radius** and **Speed** are published on the Rust Orbit group in the graph. Speed is revolutions per second; zero holds the distribution still.
- **Linear Points → Count** changes the number of dots. The Rust source redistributes them using `index / count`.
- Enter **Rust Orbit**, then select **Rust Wrangle** to edit its inline source. **Create Controls** discovers literal `chf/chv/chc/chb` calls; **Apply** compiles the draft without rebuilding or restarting Fold. Existing channels and animation are retained.
- The Ellipse and its Fill remain native editable nodes. Copy to Points consumes the generated positions; the scene transform places the result at the composition center.

The source adds a three-lobed radius variation and writes a numeric `phase` attribute. Its channel references and time are live; no animation frames are baked. The 5-second document is a half revolution at the default Speed 0.1; the complete motion period is 10 seconds.

Runtime compilation currently requires Linux `prlimit`, `rustc`, and the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`). Execution uses Fold's bounded Wasm interface. This first version edits element translation, color, opacity and numeric attributes; topology and neighbor queries remain native-node operations.

The [executable public-tool example](../skills/fold/scripts/wrangle.py) builds this setup in an isolated session and verifies control changes, failed proposals, hot source edits, undo and out-of-order rendering. [Phase 45](../docs/phase-45.md) records validation and runtime measurements.

![Rust Orbit at half a second](../docs/media/phase-45-rust-orbit.png)
