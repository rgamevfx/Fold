# Desktop UI glitch/freeze after M5a delivery — 9 October 2026

Follow-up: the user subsequently supplied a repeatable Camera → Light selection
sequence and an unselectable grade layer. Both selection symptoms reproduce in
the real native panels and share a selection-routing feedback loop. The repair
and validation are tracked in the [grouped selection recovery](m5a-selection-linking/result.md).
This does not establish the cause of the earlier process exit described below.

The user reported that Fold's UI glitched, froze and crashed after delivery of
`6db32c6`. They cannot recall the preceding action. Treat this as an open desktop
workflow failure; earlier offscreen, public-frame and automated evidence does not
qualify this interaction or establish a fix. M6 remains blocked.

## Evidence available

- The delivered Fold process and all other Fold processes were gone when inspected.
  `/tmp/fold-m5a-delivery-354ewy2h/desktop.log` ends after normal NVIDIA Vulkan
  startup; there is no panic, error, exit status or stack trace in that log.
- No current crash file was present in `/var/crash`; `coredumpctl` and `gdb` are
  unavailable. No kernel GPU failure appeared in the queried journal interval.
- COSMIC logged texture-import errors at 04:27 local time. No evidence yet ties
  these to Fold or establishes them as the cause. Do not turn this into a driver
  diagnosis without a reproduction.
- The screenshot runtime is unavailable; its restart attempt timed out. No live
  UI input or screenshot was taken.

## Recovery and next diagnostic loop

A separate copy of the delivered saved project is open in the normal release
Composition workspace, with `RUST_BACKTRACE=full`. The source project was not
modified. The diagnostic supervisor and artifacts live in
`/tmp/fold-freeze-diagnosis/`:

- `monitor.py` owns the child and records its exit status/signal. Every five
  seconds it measures a read-only public discovery request. Timeouts record
  per-thread wait channels. This detects dispatch stalls/process exit, not UI
  pixel corruption or every possible GPU presentation stall.
- `pid`, `desktop.log`, `events.jsonl`, and `monitor.log` identify the live child
  and record startup, responsiveness and any later failure.
- `render-loop.py` exercises the running instance with 32 read-only frame
  observations at repeating zero/reveal/held/later times. All passed in 3.3 seconds;
  reverse-seek zero-time pixels were stable and the held frame matched delivery.
  `render-loop.jsonl` retains frame hashes; this does not assert UI correctness.

The reopened instance stayed responsive during startup and this loop. The user's
exact symptom has **not** been reproduced; no causal hypothesis or code fix has
been established. If it freezes again, inspect the live process before restarting
it, plus the supervisor's logs if it exits. Avoid losing the next failing state.
