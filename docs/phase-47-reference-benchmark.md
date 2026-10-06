# Phase 47 reference recreation benchmark

Status: developmental trials running; the first command trial failed. No qualifying pass is claimed. See [trial review](evidence/phase-47/baseline-command-01/review.md). Required by [phase 47](phase-47.md).

## Reference and task

Recreate [mcp-test.gif](media/mcp-test.gif) from an empty Fold project as native, editable procedural artwork through public Fold MCP tools. The supplied GIF is the visual target, not a source layer to play back.

Pinned source: `/home/rgame/Downloads/mcp-test.gif`, copied unchanged into `docs/media/mcp-test.gif` for reproducibility.

- SHA-256: `10861b01d6484d2db2d8d96be5ab5e5e865493ea81d0026bd0d80f73864fe682`
- Dimensions: 1080 × 1080.
- Frames: 44, with a total encoded duration of 3,660 ms. Preserve the individual frame durations when inspecting the reference; do not assume a uniform frame rate.
- GIF loop metadata: 0 (indefinite repetition).

Reference observations: a black square background; large central letterforms assembled from regularly spaced circular cells; gold/yellow rims and highlights over darker cell interiors; brighter bands and small red/blue/cyan accents during letter transitions. C, H, A, and T shapes are visible in the sample. The full 44-frame contact sheet, exact timings and grading tolerances were frozen before artwork trials in [the reference evidence](evidence/phase-47/reference/rubric.json). All frame timestamps are graded without phase alignment; the narrow intermediate column and final return are included. These observations do not prescribe a graph construction or animation algorithm.

## Mandatory agent configuration

The implementation agent must launch the actual artwork trials as fresh **GPT-6 Luna, high reasoning effort** subagents. A parent implementation agent or a stronger model building the artwork does not satisfy acceptance.

With the available collaboration tool, use these exact settings for every independent trial:

```json
{
  "task_name": "fold_reference_trial_01",
  "model": "gpt-6-luna",
  "reasoning_effort": "high",
  "fork_turns": "none",
  "message": "<the fixed task brief below, reference access, isolated Fold session ID, and predeclared budget>"
}
```

Use a unique task name for subsequent trials. Record the actual resolved model identity and reasoning setting where available, not only the requested values. If this model/configuration or reference viewing is unavailable, record a blocked/unperformed trial; do not substitute another model or claim a pass.

Give the subagent only the fixed brief, reference viewing access, isolated session identity, budget, and the publicly shipped Fold tools/skill/documentation. Do not fork the implementation conversation or provide Rust source, private fixtures, solution graphs, hidden graders, or instructions for reproducing an engineer's construction. Generic shipped examples are allowed; a reference-specific prebuilt solution is not.

For the required bootstrap test, do not preload the Fold skill or tell the subagent where a repository copy lives. The server's initialization guidance and tool descriptions must let it discover the shipped reference/skill content through public capabilities. Log what guidance was actually advertised, fetched, and used. A configured-node query or other helper only counts as available if this fresh subagent can discover and invoke it.

Use separate blank projects and sessions for simultaneous trials. The harness may prepare reference frames with exact timestamps and connect the session, but must not prepopulate artwork, select a solution, supply project-specific code, or coach the subagent. Do not pass hidden grading feedback into a measured trial. Ordinary public Fold diagnostics and the subagent's own visual comparisons are allowed.

When a trial exposes missing capabilities, record the failure before engineering changes. Improve the general interface or shipped guidance, then restart with a fresh subagent and project. Keep assisted exploratory runs separate from acceptance results. Any delegation by the artwork subagent must use the same Luna/high configuration and count against the same trial budget.

## Fixed task brief

> Starting from the supplied empty Fold project, recreate the animation in mcp-test.gif as closely as possible using native editable Fold artwork. Inspect the complete reference, including its timing and transitions. Match the composition, letter sequence, circular-cell treatment, colors, shading, movement, and repeating behavior. Build and edit the project through Fold MCP tools; use shipped Fold documentation and skills as needed. Do not use the GIF or extracted frames as visible artwork, a baked video, an image sequence, or a frame-by-frame raster substitute. Produce a reusable procedural construction with controls for the letter sequence, cell size/spacing, palette, and playback speed or cycle duration. Inspect and refine your result across the full cycle, verify that the controls work, save the project, and provide a rendered preview plus a concise account of what was verified and any remaining differences.

Reference-only frame extraction and image comparison are allowed. All project mutations must use the public MCP authoring interface, including its code-authoring capability when available. Do not write project internals, invoke private engine endpoints, or use repository scripts to bypass missing tools. The saved result must generate new frames from native geometry/materials/animation, retain editable text or an editable letter-sequence source, and respond to its exposed controls.

## Controlled authoring-interface comparison

Before selecting the final interface, run the same reference task with two configurations: typed command authoring and compositional SDK code authoring. Use fresh Luna/high subagents and blank projects for both, with at least five trials per configuration. Keep the reference, brief, model settings, semantic engine/build, available operations, observation/verification tools, and total time/token/tool-call budgets fixed. Only the authoring surface and its necessary public usage documentation differ. Counterbalance run order and record runtime conditions.

Report success, first successful render, total elapsed time, tokens/tool calls, schema errors, corrective iterations, intervention, fidelity, and editability separately for each configuration. If equivalent capability coverage is not yet possible, label the comparison confounded and identify the differences; do not claim a language advantage. Do not combine successful trials across interfaces to satisfy the release gate. After choosing the shipped interface and freezing its guidance, rerun the five-of-five flagship gate on that configuration.

This comparison borrows the broad-code-authoring and compact-observation pattern from [MCP for Blender](https://github.com/ahujasid/mcp-for-blender/blob/main/src/blender_mcp/server.py). It does not require installing Blender or running a cross-product benchmark.

## Trial procedure and evidence

1. Before measured runs, freeze the reference hash, full-frame timing inventory, visual rubric, public brief, tool/skill/build versions, and resource/time/token/tool-call budgets. Preserve the source framing; define any permitted phase alignment or font tolerance explicitly.
2. Launch a clean Luna/high subagent into an isolated blank project, with no preloaded Fold skill. Verify that it can discover the authoring and observation workflow from the connected application's public guidance. Capture its tool transcript and errors, corrections, timing, token/tool usage, and any intervention. Record an unsuccessful outcome if the budget expires.
3. Let the subagent inspect, construct, render, and refine using the public interface. Candidate evaluation must not mutate another project or disrupt artist playback.
4. Collect the native project, rendered full-cycle preview, timestamped comparison frames, control identities, and the subagent's verification report. Exercise the unified public observation request for an animation strip plus selected evaluated measurements from one snapshot/candidate; check target/field filtering, bounded response size, adjustable image detail, and unchanged playhead/viewer state. Render the output at all 44 reference timestamps and additional samples around transitions and the wrap; do not grade selected stills alone.
5. Independently inspect structure, control response, save/reopen, undo/redo, and deterministic out-of-order evaluation. Compare both native output and exported frames under matched settings. Allow declared simulation checkpoint/replay behavior where relevant.
6. Run a fixed follow-up edit, such as replacing one letter and changing cell spacing and duration. Verify that the construction responds without rebuilding the entire project or losing its treatment. Include a native artist adjustment followed by another targeted Luna/high edit.
7. Repeat at least five times from clean state. Report every trial, including failures and assisted attempts. Do not cherry-pick the best render or reuse a prior trial's solution.

## Grading and completion

Freeze detailed tolerances before measured trials. The implementation agent cannot relax them after observing its results. Use numerical comparison for measurable motion/geometry and independent visual review for fidelity. Record reviewer identity and disagreements; the acting subagent's self-assessment is not the acceptance decision.

| Dimension | Required evidence |
|---|---|
| Construction | Blank starting project; native procedural elements; no reference-image playback or baked frame substitution |
| Composition and typography | Matching framing, scale, letter silhouettes/order, cell layout, and black background across the reference |
| Appearance | Gold cell treatment, dark interiors, highlight behavior, colored accents, and edge quality |
| Motion | Correct sequence, dwell/transition timing, trajectories and changing appearance; match the 3.66-second encoded cycle and assess the actual reference seam |
| Editability | Working sequence, cell size/spacing, palette, and speed/duration controls; follow-up changes preserve the treatment |
| Reliability | Saved/reopened result agrees; undo/redo and frame-order checks pass; resource limits and failures are reported honestly |
| Autonomy | Required Luna/high configuration, shipped tools/guidance only, no implementation access or coaching, declared budget respected |
| Discovery and observation | Fresh bootstrap without a preloaded Fold skill; callable advertised capabilities; timestamped visual and semantic evidence from the same snapshot; no disruption to artist viewing state |

All dimensions are required; good appearance cannot compensate for baked output or broken controls. A static approximation cannot pass motion checks. Render evidence must identify its exact project/candidate, time, dimensions, quality, and environment.

The flagship gate requires **five successful independent Luna/high trials out of five**, including the follow-up edit and persistence checks. This small sample is an operational release gate, not statistical proof of a 95% population success rate. The wider phase-47 suite separately measures task-run success and repeatability. Report fidelity differences and unresolved judgments explicitly; do not claim pixel identity or a seamless reference unless measured.
