#!/usr/bin/env python3
"""Generate the phase-11 verified-profile fixtures and run release headless probes.

Run from the repository root with an absolute NEW directory outside the repo.
Build execution_probe and stage_probe first; see docs/phase-11.md. Each subprocess
has a timeout. Neither filesystem caches nor other applications are disturbed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def run(command, output, timeout=180):
    with output.open("x") as log:
        subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                       check=True, timeout=timeout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    root = args.destination
    if not root.is_absolute() or root.exists() or Path.cwd() in root.parents:
        parser.error("destination must be a new absolute directory outside the repo")
    root.mkdir(parents=True)
    commands = []
    for name, frequency in [("background", 440), ("overlay", 660)]:
        command = [
            "ffmpeg", "-nostdin", "-n", "-v", "error", "-f", "lavfi", "-i",
            "testsrc2=size=1920x1080:rate=30:duration=1", "-f", "lavfi", "-i",
            f"sine=frequency={frequency}:sample_rate=48000:duration=1",
            "-vf", "setsar=1" if name == "background" else "hue=h=90,setsar=1",
            "-c:v", "libx264", "-threads", "1", "-preset", "veryfast", "-crf", "18",
            "-pix_fmt", "yuv420p", "-color_range", "tv", "-colorspace", "bt709",
            "-color_trc", "bt709", "-color_primaries", "bt709", "-c:a", "aac",
            "-ac", "2", "-shortest", str(root / f"{name}.mp4"),
        ]
        commands.append(command)
        run(command, root / f"{name}-generate.log")
    for name, command in [
        ("execution", ["target/release/examples/execution_probe",
                       str(root / "background.mp4"), str(root / "overlay.mp4"),
                       str(root / "baseline.fold"), "10"]),
        ("stages", ["target/release/examples/stage_probe",
                    str(root / "background.mp4"), str(root / "encoded.mp4")]),
    ]:
        run([sys.executable, "scripts/measure_process.py", "--report",
             str(root / f"{name}-resources.json"), "--scratch",
             str(root / f"{name}-scratch"), "--timeout", "180", "--", *command],
            root / f"{name}.log", timeout=200)
    environment = {}
    for name, command in {
        "kernel": ["uname", "-a"], "cpu": ["lscpu"], "memory": ["free", "-b"],
        "gpu": ["nvidia-smi"], "codec": ["ffmpeg", "-version"],
        "rust": ["rustc", "-Vv"], "revision": ["git", "rev-parse", "HEAD"],
        "working_tree": ["git", "status", "--short"],
    }.items():
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        environment[name] = {"code": result.returncode, "stdout": result.stdout,
                             "stderr": result.stderr}
    (root / "manifest.json").write_text(json.dumps({
        "commands": commands, "environment": environment,
        "sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                   for p in root.glob("*.mp4")},
    }, indent=2))
    print(root)


if __name__ == "__main__":
    main()
