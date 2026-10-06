# Fold reference trial — command run 02

- Native project: `/tmp/fold-phase47-command-02/GoldDotMorph.fold` (saved by public MCP at revision 1).
- Preview from committed candidate artwork generation 10: `/tmp/fold-phase47-command-02/candidate-g10-t0.png`.
- Earlier six-time sequence contact sheet: `/tmp/fold-phase47-command-02/contact-g7.png`.
- Raw MCP JSON-RPC request/response transcript: `/tmp/fold-phase47-command-02/mcp-transcript.jsonl`.
- Transport helper scripts and typed batch JSON are preserved alongside the project. CLI SHA256 was verified as `72d6ce6558b842e70de85d00eb0e3203796bc8698617459701f81862b0745d20`.

## Construction

The project contains editable native Motion text objects for G, H, A and T. Each text source feeds a circular Shape Grid; per-letter color keyframes switch each layer through the visible sequence G→H→A→T→A→G. Gold rim strokes sit over dark dot fills. Cell spacing, dot scale, text size, fill/rim colors, document frame count/rate, and all timing keys are direct editable graph/document values. No source GIF, extracted frame, or baked media is in the project.

The 6-frame contact sheet sampled 0.0, 0.8, 1.5, 2.2, 3.2 and 3.5 seconds at candidate generation 7 and showed the expected letters in sequence. A final candidate-generation-10 render at time 0 succeeded and is saved above. A larger 780 px / 36 px cell attempt exceeded Fold's vector subdivision work budget; the retained renderable setup uses 700 px text and 40 px cells.

## Incomplete or unverified

- The cycle was not sampled at all 44 source frame times in the final saved revision. Final saved-state render/reopen verification was not completed.
- The letter changes are hard opacity/color cuts, not a continuous geometric morph. The reference's colored transition accents, tiny peripheral sparks, and luminous gradient/beveled dot shading are only partially represented by the gold rim over a dark center.
- The native values are editable in the Motion graph and document settings, but they are not consolidated into one artist-facing published control panel. In particular, there is no single sequence string/speed control; users edit the per-letter sources and keyframes.
- The project was intended for a 3.66-second loop. A typed frame-rate/duration update to 183 frames at 50 fps validated, but its info-change receipt reported no graph-node changes and the result was not re-inspected after save. Exact saved duration and loop playback behavior remain unverified.

## Errors and corrections

- Text-typed Keyframes were rejected as unsupported; the sequence uses scalar/color keyframe visibility on separate text objects instead.
- An initial edit connected opacity before creating the node (`missing node`); order was corrected.
- Set Opacity on the Instances domain failed because the evaluated text/grid branch had no such elements; that path was abandoned for Fill color alpha keyframes.
- Vector subdivision exceeded the work budget at 780 px / 36 px cells (with strokes); reduced to 700 px / 40 px cells, which rendered successfully.
- One malformed JSON-RPC observation was rejected as invalid JSON and resent correctly.
