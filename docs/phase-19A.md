# Phase 19A — sustained shared playback correction

## Goal and boundaries

Close the throughput failures measured in [phase 19](phase-19.md), preserving its
full-quality 1080p30 target, UI P95 below 16.7 ms and warm seeks below 100 ms.
The GTX 1070 desktop probe admitted 882 distinct images in 30 seconds after
warm-up; mixed viewers plus ingest/export admitted 199 Real-time and 131
Every-frame images over 57.25 seconds. Audio had zero observed underruns and UI
P95 stayed below 12 ms, but video throughput did not meet acceptance.

Keep exact time, immutable sources/snapshots, explicit modes/quality, normal audio
and shared allocation ownership. Do not hide uncached execution behind range
preparation or revise targets implicitly. Device isolation requires native runs
outside the sandbox; preserve raw evidence and failed observations.

## Tasks

1. Measure per-consumer queue wait, retained-range reuse/reopen, codec RPC and
   output-readiness intervals under alternating viewer times and pinned export.
   Distinguish inclusive host/service time from physical codec/GPU execution.
2. Investigate decoder range locality, serialized codec waits holding graphics
   admission, and export decode/readback/encoding contention. Select a fix from
   evidence; no cause is established solely by the current hundreds-of-ms codec
   service intervals beside 5–7 ms shader work.
3. Keep long decode/source preparation off the UI and prevent it from withholding
   current-frame graphics capacity. Bound any staging queues and reserve memory
   before work; retain fair export/preparation service and cancellation ownership.
4. Verify fresh-frame and mixed-mode presentation, audio loops/handover, independent
   seek/edit cancellation, pressure recovery and pinned preview/export equivalence.
   Keep Every-frame slow without gaps and Real-time clock-driven without silent
   quality/effect changes.
5. Rerun the release native desktop probe and report cold/warm conditions, actual
   admissions/gaps, underruns, export progress, queue/cache counters, transfer bytes
   and logical/process-tree/driver/scratch peaks. Add direct visual normal/narrow
   UI checks when the computer-use provider is available.

## Acceptance

The original phase-19 performance checkpoint must pass on the reference machine.
Cached replay, correct slow playback, reduced quality and short headless averages
are separate evidence. If targets still miss, name remaining corrective work or
obtain an explicit target revision; leave the checkpoint unchecked. Full workspace,
Clippy and packaging qualification remain the vertical-slice checkpoint obligation.

## Status

Planned corrective work, required by the measured phase-19 target miss. No
implementation or revised-target approval is claimed here.
