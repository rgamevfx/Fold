"""Isolated ProRes thread-scaling probe, not an end-to-end Fold benchmark.

Run: python3 thread_probe.py > thread-probe.json
Reads the original clip; emits raw native planes to /dev/null. Includes process
startup and OS file caching. Fixed input/output/filter flags match Fold's native
software path at frame zero, except the consumer never backpressures the pipe.
"""
import json
import pathlib
import statistics
import subprocess
import time

clip = pathlib.Path.home() / "Videos/Cutdown_02_VFX_v01.mov"
rows = []
for trial, order in enumerate(([1, 2, 4, 8], [8, 4, 2, 1], [2, 8, 1, 4])):
    for threads in order:
        command = [
            "ffmpeg", "-v", "error", "-nostdin", "-threads", str(threads),
            "-filter_threads", "1", "-noautorotate", "-ss", "0", "-i", str(clip),
            "-map", "0:v:0", "-an", "-vf", "select=gte(n\\,0),format=yuva444p12le",
            "-frames:v", "360", "-fps_mode", "passthrough", "-threads:v", "1",
            "-f", "rawvideo", "-pix_fmt", "yuva444p12le", "pipe:1",
        ]
        begin = time.perf_counter()
        subprocess.run(command, stdout=subprocess.DEVNULL, check=True)
        rows.append({"trial": trial, "threads": threads,
                     "seconds": time.perf_counter() - begin})
result = {
    "ffmpeg": subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0],
    "clip": str(clip), "frames": 360, "rows": rows,
    "median_seconds": {str(n): statistics.median(r["seconds"] for r in rows if r["threads"] == n)
                       for n in [1, 2, 4, 8]},
}
print(json.dumps(result, indent=2))
