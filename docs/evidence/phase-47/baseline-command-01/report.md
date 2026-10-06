# Fold command-authoring baseline trial

## Trial record

- Trial start: 2026-10-06 02:37:49 UTC.
- Trial end: 2026-10-06 03:08:54 UTC (final evidence/report capture). Wall elapsed: 31m05s. Per parent instruction, the OCIO runtime repair wait was excluded from active budget; exact excluded duration was not captured, so active elapsed cannot be reconstructed precisely.
- Public Fold MCP tool calls: 118 JSON-RPC request IDs across the saved `run-*.jsonl` transcripts. MCP `initialize` handshakes are not counted. This is below the 160-call cap.
- Model identity: unavailable from the interface.
- Runtime interruption: initially the pinned renderer could not load `/tmp/lib/fold/libfold_ocio.so`; parent supplied the pinned OCIO 2.4.2 runtime under `/tmp/lib/fold` and `/tmp/share/fold/color`. Waiting for this setup was excluded from active time by instruction. No binary or artwork change was supplied.
- Server: private `XDG_RUNTIME_DIR=/tmp/fold-phase47-baseline-command-01/runtime` (mode 0700); own Fold server only. Unix socket creation initially failed with `Operation not permitted` and required sandbox escalation.

## Artifact and public-tool workflow

- Saved editable project: `circular-letter-sequence.fold` (candidate 1, generation 12, saved revision 2, `dirty:false`).
- Full-cycle rendered sample contact sheet: `rendered-preview-contact-sheet.png`; individual images `sample-0p0s.png`, `sample-0p7s.png`, `sample-1p3s.png`, `sample-2p1s.png`, `sample-2p9s.png`, and `sample-3p5s.png`.
- MCP helper, discovery output, and complete per-call request/response transcripts are retained in this directory (`mcp.py`, `discover.py`, `wire.py`, `discovery-transcript.jsonl`, `run-*.jsonl`, plus `calls.json`).
- The animation is a native Motion graph with six editable text-object branches for G/H/A/T/A/G. Each has a circular Shape Grid, gold/bronze Fill/Stroke styling and scalar opacity keyframes; document timing is 366 frames at 100 fps (3.66 s). The branches and values can be edited in the graph. It does not expose a reusable global control panel for sequence, cell size/spacing, palette, or speed; those values remain distributed across nodes and timeline settings.
- Empty-project bootstrap deviation: no blank project initializer was available through the discovered public server. A black 1x1 PPM was imported through the documented CLI to create a project file before launching the server. It remains as an unused image-sequence document in the project; the authored animation itself is native Motion artwork.

## Reference and verification

The supplied reference inventory contains 44 frames, spanning 0–3580 ms in 80/90 ms intervals (3.66 s total). It shows one centered uppercase stippled letter at a time, cycling G → H → A → T → A → G, with gold/bronze dots and brief red/cyan/blue transition accents on black.

After the runtime repair, six 256×256 renders at 0.0, 0.7, 1.3, 2.1, 2.9, and 3.5 seconds completed and were saved above. All six show the same small low-centered A-like glyph plus detached G-shaped fragments at the upper-left. Thus the letter sequence and transitions did not visibly animate; composition and letter forms are materially wrong; no transition fringe or visible shading was verified. The render samples prove the renderer works, but they do not satisfy the visual or motion brief. Larger and multi-frame observations also hit `vector subdivision work budget exceeded`, constraining full-cycle rendered inspection. No numeric image-difference score was computed; the verified differences are visual and are shown in the rendered sample sheet alongside the reference contact sheet in `/tmp/fold-phase47-reference/`.

## Errors and corrections

- `agent serve` with no project showed no sessions; specifying a nonexistent file was rejected. Bootstrapped the project via one-frame PPM import, then started the private server.
- Socket creation returned `Operation not permitted`; retried with sandbox escalation.
- Before runtime repair, renders failed because the pinned OCIO shared library was missing. After repair, individual rendering succeeded.
- Attempted to keyframe text directly; public API rejected `Text` keyframes as unsupported. Switched to six text branches gated with scalar opacity keyframes.
- One candidate batch used a stale revision and conflicted; re-read state and retried against the current candidate revision.
- `Set Color/Opacity` using `Instances` returned no elements; corrected the domain to `Children`, then tried `Glyphs` where appropriate.
- Shape Grid placement/instance evaluation remained incorrect. Several upstream/downstream graph adjustments failed to yield correctly positioned letters.
- Larger render requests, full strips, and multi-frame observations returned `vector subdivision work budget exceeded`; reduced to 256×256 single-time samples and restored cell size to 400 to obtain individual images.
- A few script invocation/JSON parsing mistakes and attempts to execute the binary as Python produced no project mutation; all are visible in the preserved transcripts.

## Trial outcome

The project saves and contains editable native Fold graph construction, but the reference animation was not recreated successfully. Motion, letter sequencing, and the requested consolidated reusable controls remain unverified or absent. Preserve this baseline as an unsuccessful authoring trial rather than presenting it as a successful recreation.
