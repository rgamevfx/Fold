#!/usr/bin/env python3
"""Read-only C0 heartbeat for an explicitly selected Fold process/socket.

Writes exclusive JSONL evidence. Never edits, restarts, signals, or owns the app.
Dispatch responsiveness is not proof of healthy UI pixels or GPU presentation.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time


def identity(pid):
    # Process names may contain spaces/parentheses; starttime is field 22.
    return (Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()[19]


def state(pid):
    root = Path("/proc") / str(pid)
    result = {"threads": {}}
    try:
        result["status"] = [line for line in (root / "status").read_text().splitlines()
                            if line.startswith(("State:", "VmRSS:", "Threads:"))]
        for task in sorted((root / "task").iterdir()):
            try:
                result["threads"][task.name] = (task / "wchan").read_text().strip()
            except OSError:
                pass
    except OSError as error:
        result["unavailable"] = str(error)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--socket", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=20)
    args = parser.parse_args()
    if not 1 <= args.samples <= 3600:
        parser.error("samples must be in 1..3600")
    started = time.monotonic()
    process = identity(args.pid)
    with args.output.open("x") as output:
        for index in range(args.samples):
            entry = {"sample": index, "elapsed_s": time.monotonic() - started, "pid": args.pid}
            try:
                if identity(args.pid) != process:
                    entry["event"] = "pid_reused"
                    output.write(json.dumps(entry) + "\n")
                    break
            except OSError:
                entry["event"] = "process_exited"
                output.write(json.dumps(entry) + "\n")
                break
            before = time.monotonic()
            try:
                response = subprocess.run(
                    [str(args.cli.resolve()), "protocol", str(args.socket)],
                    input='{"version":2,"command":{"method":"discover"}}',
                    capture_output=True, text=True, timeout=2,
                )
                entry.update(event="dispatch_reply", latency_ms=(time.monotonic() - before) * 1000,
                             exit_code=response.returncode)
                if response.returncode == 0:
                    value = json.loads(response.stdout)
                    entry.update(epoch=value.get("epoch"), revision=value.get("result", {}).get("revision"),
                                 error=value.get("error"))
                else:
                    entry["stderr"] = response.stderr[-2048:]
            except subprocess.TimeoutExpired:
                entry.update(event="dispatch_timeout", latency_ms=(time.monotonic() - before) * 1000)
            except (OSError, ValueError) as error:
                entry.update(event="probe_error", error=str(error))
            entry["process"] = state(args.pid)
            output.write(json.dumps(entry) + "\n")
            output.flush()
            if index + 1 < args.samples:
                time.sleep(1)


if __name__ == "__main__":
    main()
