# Independent artwork trial — NOV

The new animation is saved in `demos/cell-type-rust.fold`, paired with exact Rust source, an artist guide, a native rendered PNG and a 24-frame/6-second GIF preview. It was created only through the public Fold MCP in isolated blank/saved-demo sessions. Only the Fold skill, its public authoring references, source reference images, supplied public transport helper and live public catalogs were used. No implementation crates, project archive contents, private application APIs or other agents' notes were read. No app code or skill files were edited.

Requested model: GPT-Sol-6.1. The task supplied this requested identity; transport/tool metadata did not expose resolved model identity, so it is unverified.

## Budget verdict

**The initial budgeted trial failed loop continuity.** It finished save/reopen at 147 Fold public tool invocations, within the 150-call limit, in approximately 6 minutes after harness startup. It rendered a visibly new editable `NOV` animation, but the letter wrapped from the hidden top to a partially visible bottom at 6 seconds. Equal 0/6-second frames did not prove continuity. `final-5.96.png` and `final-6.png` retain this failure. The original candidate/public transcript and source proposals remain in evidence; the demo project was subsequently replaced by the polished final revision.

At the parent agent's explicit request, useful artwork work continued. **The completed artifact used 193 total public tool invocations, 43 beyond the requested 150-call budget.** It is not a budgeted-trial pass. Calls 148–193 belong to the remedial continuation; calls above 150 explicitly carry `over_invocation_budget` in the authoritative transcript, except initial continuation records whose phase is identified by request numbers 36 onward. Total elapsed public trial span was approximately 14.4 minutes, within the 30-minute time budget. Aggregate synchronous tool response wait was 2.645 seconds; asynchronous rendering/compilation is separate. Each public call has individual elapsed seconds. Observation results include per-frame native render timing.

## Construction and iteration

Inspected actual GIF frames 0, 25, 50, 75, 100, 125 and 150. This supplied distributed-time visual evidence beyond the provided contact sheet. The initial source made a new `RAY` conveyor, with shared circular-cell geometry, pink/green/blue/white/gray colors, dark surroundings and a dot grid. A public font discovery/import embedded DejaVu Sans Bold; source revision thickened the initial thin typography and brought neighboring letters into view.

Artist edits changed text `RAY` → `NOV`, radius 13 → 11.5 px and pink color. Rendered behavior changed visibly. A subsequent source revision changed defaults to `SUN`/14 px while the actual instance retained `NOV`/11.5/custom pink. Pre/post revision rendered 0-second PNGs were pixel-identical. Inspection retained the same source graph, definition, instance and Composition identities.

The initial palette Wrangle overwrote each instance's descendant paints, flattening some depth. The repaired version uses five reusable native shaded cell sources, with per-palette visibility selection. Colored faces now retain dark offset shadows and light highlights. The conveyor now wraps in a symmetric offscreen interval: `(phase - 1.5) * -pitch + 444`, with phase period 3 and speed 0.5. At the verified format/size, both discontinuous wrap positions lie outside the visible viewport. The native dot field and breathing keys also repeat across the 6-second loop.

Added an ordinary Published Control connected to speed, plus ordinary Keyframes connected to radius. A further Rust source revision preserved both external connections. Keys at 0, 1, 3, 4, 6, 7, 9, 10 and 12 seconds use Ease interpolation and values 11.5/13 px. These are native source-graph keys; Composition UI animation-track editing was not exercised.

## Final verification

- Rendered the final Composition at 0, 1, 2, 3, 4, 5, 5.96, 6, 6.04, 7, 8 and 11.96 seconds. `image-167.png` is the 4-column × 3-row contact sheet; each rectangle/time is supplied in `response-52.json`. `polished-*.png` crops use those rectangles. Final 0/6-second frames are pixel-identical. Near-wrap frames show continuous travel without the initial appearing bottom fragment.
- Rendered 1 second at native 500 × 888 in `image-169.png`. The saved/reopened render `reopen-image-184.png` is pixel-identical.
- Inspected saved/reopened controls, text, palette, radius keyframes and speed/radius connections. All persisted. Native graph output rendered after reopen without rerunning author-time source.
- The original single candidate commit was undone at revision 1 and redone at revision 2. Public results show revision 2/3; the restored artwork was saved and reopened. The repaired final candidate was a separate commit, saved at revision 4 and reopened.
- The animated preview was produced from 24 public Composition samples at quarter-second spacing, using two 12-time observations. It is a preview, not a full-rate export or playback-performance proof.

## Failures and limitations

Every Fold public invocation/result is in `public-transcript.jsonl`. Failed helper attempts are retained in response files. The first local image extraction attempted unavailable `python`, then succeeded with `python3`. Requests 2/3 failed before reaching MCP because the reused transport helper's `call(method, ...)` collided with a catalog `method` argument; later discovery used guide/type requests and the final harness calls `tool` directly. Public call 58 failed with `unknown authoring type` for `fold.composition.document` catalog discovery; common Composition construction/render nevertheless succeeded through documented commands.

Public job 170 failed because the speed control-response verification had only one exact sample time. The corrected two-time probe completed but returned **inconclusive**, because the asset exceeds bounded geometry sampling (2,478 instances and thousands of retained groups/paths). This is not a speed-response pass; text, radius and palette response were visually exercised separately.

Initial helper polling at 50 ms consumed too many invocations; later jobs were explicitly polled once after intervening work or sensible delays. The final harness response sidecars occasionally contain an incorrect local image path derived from transport serial rather than aggregate count. The authoritative transcript strips images to the correct written paths; `image-map.json` maps aggregate invocation numbers. No image payloads are omitted from disk. A contact-sheet crop initially assumed one row, was detected from metadata, and was corrected using supplied rectangles.

Desktop viewer capture, inspector interaction, real-time playback, full-rate video export and Composition UI timeline-track edits are unverified. Extreme artist size/pitch changes can expose wrap positions; other speed values need corresponding duration/key changes. The setup uses three Unicode scalar characters, not arbitrary word layout. No workspace or crate checks were run because this task authored artwork through public tools without changing application code.
