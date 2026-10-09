#!/usr/bin/env python3
"""C0 isolated desktop capture: public seed edit, passive timings and heartbeat.

Usage: native_capture.py NEW_OUTPUT_DIRECTORY
Requires the release native-probe desktop and a built debug fold-cli.
"""
import hashlib
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--desktop", type=Path, default=ROOT / "target/release/fold-desktop")
    args = parser.parse_args()
    destination = args.output.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    cli = ROOT / "target/debug/fold-cli"
    fixture = ROOT / "docs/rewrite/evidence/m5b-controls-baseline.fold"
    original_hash = hashlib.sha256(fixture.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="fold-c0-native-") as scratch:
        scratch = Path(scratch)
        copy = scratch / "baseline.fold"
        shutil.copyfile(fixture, copy)
        # Current workspace v4, pinned to the frozen fixture's public main output.
        # This is isolated navigation state, never an artwork mutation.
        output = {"document": "24b305ec-9c75-4b52-8793-9e589eeb921f",
                  "output": "e4c7bf6d-f197-4899-875b-927b8be58e22"}
        workspace = {"version": 4, "project": str(copy), "panels": {}, "editors": {},
                     "viewers": {"1": {"binding": {"Pinned": output}, "last_output": output,
                                        "editor": None, "time": {"numerator": 0, "denominator": 1},
                                        "divisor": 1}},
                     "focused_editor": None, "layout": "", "next_id": 2,
                     "monitored_viewer": 1}
        association = 0xcbf29ce484222325
        for byte in str(copy).encode():
            association = ((association ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
        sidecars = scratch / "state/fold/workspaces"
        sidecars.mkdir(parents=True)
        (sidecars / f"{association:016x}.json").write_text(json.dumps(workspace))
        env = dict(os.environ, XDG_STATE_HOME=str(scratch / "state"),
                   FOLD_PASSIVE_PROBE=str(destination / "native.json"),
                   FOLD_COLOR_ROOT="/home/rgame/Documents/fold-native-pipeline/release-package",
                   RUST_BACKTRACE="full")
        # Do not inherit a different automatic probe or software-render override.
        for name in ("FOLD_NATIVE_PROBE", "FOLD_SHARED_PROBE"):
            env.pop(name, None)
        record = {"fixture_sha256": original_hash, "events": [],
                  "desktop": str(args.desktop.resolve()),
                  "desktop_sha256": hashlib.sha256(args.desktop.read_bytes()).hexdigest()}
        with (destination / "desktop.log").open("x") as log:
            child = subprocess.Popen([str(args.desktop.resolve()), "--protocol-dir", str(scratch), "--project", str(copy)], env=env, stdout=log, stderr=subprocess.STDOUT)
            watcher = None
            try:
                deadline = time.monotonic() + 35
                socket = None
                while time.monotonic() < deadline and child.poll() is None:
                    socket = next(scratch.glob("fold-session-*/protocol.sock"), None)
                    if socket:
                        break
                    time.sleep(0.05)
                if socket is None:
                    raise RuntimeError("No private protocol socket; inspect desktop.log")

                def request(command):
                    until = time.monotonic() + 10
                    while time.monotonic() < until:
                        answer = subprocess.run([str(cli), "protocol", str(socket)], input=json.dumps({"version": 2, "command": command}), capture_output=True, text=True, check=True, timeout=5)
                        value = json.loads(answer.stdout)
                        record["events"].append({"method": command["method"], "unix_ms": time.time_ns() / 1e6,
                                                 "state": value.get("result", {}).get("state"), "error": value.get("error")})
                        if "error" in value:
                            raise RuntimeError(value["error"])
                        if value.get("result", {}).get("state") != "running":
                            return value
                        command = {"method": "job_status", "handle": value["result"]["job"], "wait_ms": 100}
                    raise TimeoutError("Public job exceeded capture deadline")

                watcher = subprocess.Popen([sys.executable, str(ROOT / "scripts/maintenance_watch.py"), "--pid", str(child.pid), "--socket", str(socket), "--cli", str(cli), "--output", str(destination / "heartbeat.jsonl"), "--samples", "20"])
                while True:
                    discovery = request({"method": "discover"})
                    documents = discovery["result"]["documents"]
                    if documents:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Project open did not complete")
                    time.sleep(0.05)
                (destination / "discovery.json").write_text(json.dumps(discovery, indent=2) + "\n")
                document = next(d["id"] for d in documents if d["type"] == "fold.motion.construction")
                scope = {"document": document, "definition": None}
                source = request({"method": "inspect_document", "epoch": discovery["epoch"], "revision": discovery["result"]["revision"], "scope": scope})["result"]["source"]
                changed, count = re.subn(r"^seed (\d+);$", lambda m: f"seed {int(m[1]) + 1};", source, flags=re.M)
                if count != 1:
                    raise RuntimeError("Expected one public seed declaration")
                def guard():
                    current = request({"method": "discover"})
                    return {"epoch": current["epoch"], "base": current["result"]["revision"], "request_id": str(uuid.uuid4()), "scope": scope}
                # Allow startup frame presentation before an isolated semantic edit.
                time.sleep(2)
                candidate = request({"method": "propose", "guard": guard(), "edit": {"kind": "source", "expected": source, "text": changed, "mapping": {}}})["result"]["handle"]
                review = request({"method": "inspect_candidate", "handle": candidate})["result"]
                (destination / "candidate.json").write_text(json.dumps(review, indent=2) + "\n")
                if review.get("source") != changed:
                    raise RuntimeError("Candidate source differs from the requested seed-only edit")
                apply_guard = guard()
                record["apply_start_unix_ms"] = time.time_ns() / 1e6
                applied = request({"method": "apply", "guard": apply_guard, "handle": candidate, "receipt": review["receipt"]})
                record["apply_done_unix_ms"] = time.time_ns() / 1e6
                record["applied"] = applied
                record["exit_code"] = child.wait(timeout=30)
            except Exception as error:
                record["failure"] = str(error)
                if isinstance(error, subprocess.CalledProcessError):
                    record["request_stderr"] = error.stderr
            finally:
                if child.poll() is None:
                    # Only this disposable child is cleaned up. Never attach/kill an existing Fold.
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()
                    record["supervisor_terminated_disposable_child"] = True
                if watcher is not None:
                    watcher.wait(timeout=6)
                record["exit_code"] = child.returncode
                record["original_unchanged"] = hashlib.sha256(fixture.read_bytes()).hexdigest() == original_hash
                (destination / "run.json").write_text(json.dumps(record, indent=2) + "\n")
    if record.get("failure") or record["exit_code"] != 0 or not record["original_unchanged"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
