# Particle Attraction

Open [particle-attraction.fold](particle-attraction.fold) in the updated release desktop. This nine-second, 1080×1920, 25 fps Motion scene was authored through live MCP tools from [the Instagram reference](https://www.instagram.com/p/DcRZOZqo0Wj/). It contains 1,000 colored particles gathering from the edges into editable text.

[Watch the preview](../docs/media/particle-attraction-preview.mp4) · [Representative frames](../docs/media/particle-attraction-frames.png)

Select **Attract Particles** in the Motion layer list:

- **Attractor → Edit** opens the text source. Replace “3” or select another filled object in the Attractor picker.
- **Initial points → Edit Scatter Points** controls count, area size, center, edge distribution, position seed, and Spread. The example uses a 1000×1720 area centered at (540,940), with ±38 px spread on each axis. Its Points input can be rewired to another distribution.
- **Attraction** and **Turbulence** have keys on the MoGraph layer track. Attraction rises from 0 to 1 over the first 0.4 seconds; turbulence settles to zero by 3 seconds. Move these keys to change the action's timing.
- **Response**, **Speed variation**, and **Damping** control gathering and settling. The example uses 0.85 Hz, 0.8 variation, and damping ratio 1.
- **Particle size** is 8 px. **Color → Edit Random Palette** exposes editable palette stops and a separate color seed. Stepped intervals set color probabilities; smooth interpolation can create gradients.

To create another setup, select a filled shape or text object and choose **Tools → Animate → Attract Particles**. The action builds an ordinary editable group using Scatter Points, Particle Targets, Particle Step, Solver, and Copy to Points. Open its graph to customize particle geometry or forces.

The attractor is sampled at Solver start. Shape edits rebuild the simulation; animated target motion after the start is not followed. There are no collisions or continuous emission in this first tool. The font and trajectories are a procedural interpretation of the reference.

Live reuse was checked by changing the target to “8,” count to 600, color seed to 42, and attraction duration to two seconds, then rendering and undoing that probe. See [the agent workflow](../skills/fold/references/particles.md) for public operations and [dev-plan.md](../dev-plan.md) for implementation and validation status.
