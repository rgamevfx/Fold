#!/usr/bin/env python3
"""Linux sampled process-tree RSS and scratch-disk probe (not allocator accounting).

Usage: measure_process.py --report report.json --scratch /new/scratch -- command args
RSS is summed across the process tree and may double-count shared pages. Sampling
can miss short peaks. Scratch must be dedicated to this run; existing files are
never removed. Child stdout/stderr are inherited. No shell is used.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def tree_rss(root):
    processes = {}
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        try:
            status = {}
            for line in (path / 'status').read_text().splitlines():
                key, _, value = line.partition(':')
                status[key] = value.strip()
            processes[int(path.name)] = (int(status['PPid']), int(status.get('VmRSS', '0 kB').split()[0]) * 1024)
        except (OSError, ValueError, KeyError):
            continue
    members = {root}
    while True:
        children = {pid for pid, (parent, _) in processes.items() if parent in members}
        if children <= members:
            break
        members |= children
    return sum(processes.get(pid, (0, 0))[1] for pid in members)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.report.exists() or args.scratch.exists():
        parser.error('provide a command and new report/scratch paths')
    args.scratch.mkdir(parents=True)
    started = time.monotonic()
    child = subprocess.Popen(command, env={**os.environ, 'TMPDIR': str(args.scratch.resolve())}, start_new_session=True)
    peak_rss = peak_disk = samples = 0
    timed_out = False
    try:
        while child.poll() is None:
            peak_rss = max(peak_rss, tree_rss(child.pid))
            disk = 0
            for path in args.scratch.rglob('*'):
                try:
                    if path.is_file():
                        disk += path.stat().st_size
                except OSError:
                    pass
            peak_disk = max(peak_disk, disk)
            samples += 1
            if time.monotonic() - started > args.timeout:
                timed_out = True
                break
            time.sleep(0.05)
    finally:
        if child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        code = child.wait()
        with args.report.open('x') as report:
            json.dump(dict(command=command, exit_code=code, timed_out=timed_out,
                           wall_seconds=time.monotonic() - started, samples=samples,
                           sampled_tree_rss_bytes=peak_rss, sampled_scratch_bytes=peak_disk,
                           nominal_sample_interval_ms=50), report, indent=2)
    return code if code >= 0 else 1


if __name__ == '__main__':
    raise SystemExit(main())
