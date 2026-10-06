# Fold reference recreation trial 01

**Outcome:** failed development trial, native artifact preserved. The saved result is a working, editable gold-dot G at a representative frame. The complete reference recreation is unfinished.

## Artifacts

- Native Fold project: `/tmp/fold-phase47-sdk-01/GHATA.fold` (MCP save completed at revision 1; SHA256 `1e7951150cc898cd83326cf2b673dd2534276bc7b8cfbc22f22ae9c07313dc51`).
- Rendered evidence: `/tmp/fold-phase47-sdk-01/preview.png` (256×256, candidate generation 7 at t=0.1 s; SHA256 `1a6f09d7df119c5a8b49af26a793beb1cc7c55d7373f023d97b1b4f829258aa6`).
- Raw MCP JSON-RPC transcript: `/tmp/fold-phase47-sdk-01/mcp-transcript.jsonl`.
- Session: `ed0e88da-bcbb-4ef4-9f6a-6dec8e148a21`; committed revision 1.

## Reference analysis

Inspected the contact sheet containing all 44 prepared frames and opened full-resolution frames 0 and 8. The reference is a 1080×1080 black composition lasting 3.66 seconds (44 frames at about 12 fps). It cycles one large uppercase character through G, H, A, T, A, then back to G. The letters are built from closely spaced circular cells with warm gold dimensional shading; a small number of bright red, blue, cyan and white cells move around the contours during transitions. G sits left of center in the first/last transition; the other glyphs are near center.

## Native construction and controls

Built five editable native Text objects (G, H, A, T, A), each filled and sampled by `fold.motion.shape_grid` in circular mode. A looping time/math graph controls opacity through per-letter envelopes. Artist controls in the motion graph:

| Control | Published-control node ID | Default |
| --- | --- | --- |
| Cell spacing · px | `521104dc-ce42-4164-bad3-ec5a148508c8` | 40 px |
| Cell diameter · pitch factor | `6015583a-6998-48b2-9d23-0aed4a3e4ab4` | 0.75 |
| Palette · warm gold | `fe4742af-4255-4594-bb3d-43401dcaca28` | `[0.88, 0.66, 0.30, 1]` |
| Sequence speed · factor | `466dc316-0184-4b8c-b6a7-e7de9f38f5e2` | 1 |

The speed control scales the time input used by the opacity graph. The individual character sources and gates are separately editable. Text keyframes were probed but the live build reports `unsupported keyframe type` for Text values, so the letter order is not exposed as a single sequence field.

## Verification and limits

A candidate observation at generation 7, t=0.1 seconds, 256×256 completed and visibly shows a G made of warm-gold circular dots on black. The result resembles the reference's broad composition and dot treatment, but its beads are flat and the font is thinner than the reference. A 1080×1080 active render and a multi-time 400×400 observation failed with `vector subdivision work budget exceeded`; a single-frame 256×256 observation succeeded. The visible evidence covers only the opening G, not the whole loop. The H/A/T/A opacity sequence, cycle wrap, controls, and saved-project reopen were not visually verified. No 44-time render evidence was saved.

Missing reference features include dimensional sphere shading and moving multicolor perimeter beads. There is no single artist-editable sequence control. Time budget ended before further observation and verification. The artifact is saved unchanged as the best native result reached.

## Trial accounting

- Public MCP tools/call invocations: 57.
- Failed responses: 7 asynchronous jobs and 3 direct tool errors (10 total), including invalid graph insertion, unsupported Text keyframes, render subdivision limits, and a wrong document ID retry.
- CLI executable: `/tmp/fold-phase47-sdk-bin/fold-cli`, expected SHA256 `72d6ce6558b842e70de85d00eb0e3203796bc8698617459701f81862b0745d20`.
- Private runtime: `/tmp/fold-phase47-sdk-01/runtime`.
- Shipped public guidance read: catalog `workflow`, `motion`, and `candidates`; SDK declarations fetched with `sdk:true`.
