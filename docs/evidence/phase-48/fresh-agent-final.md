# Final-build fresh public artwork acceptance

PASS for the exercised authoring, preservation and persistence workflow. This run started from an empty isolated project against the rebuilt `target/release/fold-cli`, never opened the prior-run project, and only used public catalog/author/candidate/inspect/render/save operations. It reused my own public-transport harness and artwork source from the first trial; no implementation or repository SDK/demo/helper files were read.

Saved artwork: `badge.fold`. Stable namespace: `fresh-agent-badge`. The native pipeline is Motion → Composite → Sequence, 960×540, 240 frames at 24fps, green canvas, rounded text badge, 52 copies spaced [-9,0], stagger 0.024s, independent oscillator and independent palette. Evaluated inspection counted 52 instances and 52 paths; construction inspection identifies distinct source/oscillator/ramp bindings.

A public candidate preview succeeded at generation 2. Three initial Sequence frames (0, ⅓, ⅔ seconds) were rendered and actually viewed. All show the complete green canvas, readable front badge, and visibly changing wave. Artist simulation set text to `Artist edit`, renamed the source `Artist renamed badge`, and moved its stored transform to [735,245]. Follow-up changed only oscillator bounds from ±60 to ±20. Repeating the exact initial construction preserved every returned handle and the complete inspected Motion data, with 19 main graph nodes and 5 scene objects unchanged. Two final frames were actually viewed and showed readable replacement text and the smaller wave.

Final revision 4 saved successfully (dirty=false, epoch=2). The server was deliberately stopped, the final saved project reopened, and construction repeated privately. All 12 handles matched the original returned identities; full Motion inspection matched the pre-reopen inspection exactly. A reopened Sequence frame at ⅓ second was rendered and actually viewed. Verification candidate was discarded. All isolated servers from both trials were stopped; unrelated MCP processes were untouched.

## Metrics

- 104 logged API calls: 57 jobs polls, 11 candidate, 10 render, 8 state, 6 author, 6 inspect, 4 catalog, 2 save. Two CLI session listings outside the log bring total public invocations to 106; process launches/stops are separate setup.
- 57.05 seconds from first logged discovery to final discard. Construction + edit preservation + save + initial checks finished in 15.91 logged seconds; intervening visual review/reopen accounts for the remainder. Full setup/reporting wall time is a few minutes. Poll interval 0.5s.
- No failed public requests, no retries, no unexpected process exits in this final run.
- Ten rendered frames; six actually visually inspected: initial-0/1/2.png, repeat-0/1.png, reopened.png.

## Discovery/friction and limits

Public catalog again exposes SDK v2 and the live-tools guide; workflow routes successfully to that guide. This was a repeat trial, so syntax discovery benefited from the first trial. The same canvas transform correction was intentionally applied as a separate explicit edit, using the public transform role. No first-frame black-margin mistake was repeated.

Reopened `agent serve PATH` still reports path=null, dirty=true even though saved content loads intact. Hidden badge-source position is preserved but visible copy placement remains independent because the source reference is local. The rendered palette is vivid magenta/burgundy-to-blue under the advertised ACES display transform; exact display-space burgundy matching was not tested.

Headless candidate preview returned success, but desktop preview chrome, Apply/Discard interaction, and capture were not visually tested. Also untested: undo/redo, deleting bound targets, loop boundaries, video export and analytical motion verification. Public evaluated inspection declares paint/layout bounds, parameter contributions, pixel visibility and provenance unsupported. None blocked the brief.

Evidence: `calls.jsonl`, `construction-inspection.json`, `counts.json`, `original-handles.json`, `artist-inspection.json`, `gentler-inspection.json`, `repeat-inspection.json`, `reopened-inspection.json`, `final-state.json`, `reopened-state.json`, and PNGs named above. The earlier trial's independent report remains `/tmp/fold-phase48-fresh-agent/report.md`.
