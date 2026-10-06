# Fresh artwork-agent acceptance trial (first release build)

Completed using only fold-cli public tools and catalog-delivered guidance/SDK declarations. No Rust, repository demo, SDK file, or existing helper script was read. Harness, saved project and evidence are exclusively under this directory. Namespace: `fresh-agent-badge`.

## Outcome

Native 960×540, 240-frame/24fps Motion → Composite → Sequence saved as `badge.fold`. Motion has an editable text/rounded-background badge, 52 copies spaced [-9,0] with 0.024-second stagger, separate Y oscillator, and separate burgundy-to-blue color ramp. Native evaluated inspection counted exactly 52 instances and 52 paths. Public construction query identified separate source, oscillator, and ramp links.

Candidate preview returned `previewing:true` at initial generation 1 and corrected generation 2 (Sequence output for the latter). Headless operation means the actual UI panel, Apply/Discard buttons, and preview capture were not visually verified. Ten full-resolution Sequence frames were rendered; eight were actually opened and visually inspected: initial 0/⅓/⅔ seconds, corrected ⅓ second, artist 0, gentler 0, and repeat 0/⅓ (eight images total). Corrected/final frames have a full green canvas, rounded badges, readable front text, and visibly distinct wave phases. Ramp renders vivid magenta/burgundy-to-blue under ACES display transform, rather than an exact display-space burgundy match.

Artist simulation changed text to `Artist edit`, source name to `Artist renamed badge`, and badge transform to [735,245]. A separate follow-up changed only oscillator minimum/maximum from ±60 to ±20. Exact original construction source was run again. All 12 returned handles remained identical; full inspected Motion data after gentler and repeated construction compared exactly equal. Text, renamed object, stored position, canvas correction, palette, count, spacing, and softened oscillator were preserved. No objects or nodes duplicated (5 scene objects and 19 main graph nodes in each inspected stage).

The first process exited with status 143 unexpectedly after initial save; the next call failed connection refused. Reopening `badge.fold` in a new private session retained all documents and the namespace, and subsequent author calls loaded all stable handles without supplying handles explicitly. This establishes save/reopen identity persistence for the initial construction. Final artist-edited revision 4 was saved successfully and state reported dirty=false, project_epoch=1. Final revision was not separately reopened.

## Friction / failed attempts

- `agent --help` returned usage and exit 1; adequate discovery but not conventional help success.
- Local environment has `python3`, not `python`; first harness invocation failed before any API call, then corrected.
- Catalog workflow successfully routes to `live-tools`, but initial catalog guide list and related topics omit `live-tools`. Workflow text also contains older advice to pass returned handles, whereas live-tools correctly describes automatic namespace persistence.
- Querying Motion catalog for `info` returned large operation schemas without a typed canvas-info value schema. Used documented `.info` example successfully.
- Rectangle object creation defaults to translation [240,216], even with full-canvas dimensions. Three first frames revealed black margins; public inspection identified the translation, and explicit role transform [0,0] corrected it. Raw rectangle coordinate documentation does not mention scene-object default placement.
- First image decoder guessed image.base64; actual public CLI image payload is image.data + mimeType. Corrected locally; no API failure.
- Initial headless serve process terminated 143. Cause unknown; not attributed to Fold. One state request failed with socket connection refused. Recovery through saved project succeeded.
- Reopened serve project initially reported path=null and dirty=true despite loading saved documents. Explicit save fixed this; no data-loss observation.
- Moving hidden badge source preserves its transform but does not move visible copies. The copy source is local, so placement of copies is independent. This is a workflow distinction an artist should be told; only source-position preservation, not visible-copy relocation, was verified.
- No author/candidate request validation failures occurred; all artwork mutations succeeded first try.

## Evidence and metrics

- `initial-0.png`, `initial-1.png`, `initial-2.png`: initial canvas placement defect and full wave.
- `corrected.png`: corrected full green canvas.
- `artist-0.png`, `artist-1.png`: edited text, original strength.
- `gentler-0.png`, `gentler-1.png`, `repeat-0.png`, `repeat-1.png`: ±20 wave, preserved edited text; repeat images visually inspected.
- `artist-inspection.json`, `gentler-inspection.json`, `repeat-inspection.json`: structural preservation.
- `construction-inspection.json`, `counts.json`, `original-handles.json`, `final-state.json`: drivers, 52 instances, stable IDs, saved status.
- `calls.jsonl`: 277 logged public API calls, including 221 jobs polls; 56 non-poll calls. Includes one failed connection. Two session listings and the initial unlogged catalog call add 3 public invocations (280 total). Two serve launches are process setup, not API calls.
- Logged discovery-to-final-check time: 178.13 seconds. Initial setup/help/session discovery occurred before the log; total wall time roughly 5 minutes. No render throughput claim; rapid 100ms harness polling inflated tool-call count.

Not verified: desktop interaction, undo/redo behavior, loop endpoints, exported movie, exact display-space palette matching, final-revision reopen, analytical motion checks, deleted target recovery. Public evaluated query explicitly lists unsupported paint/layout bounds, parameter contributions, pixel visibility, and provenance. None blocked this artwork trial.
