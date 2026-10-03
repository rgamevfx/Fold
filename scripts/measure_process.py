#!/usr/bin/env python3
"""Linux sampled process-tree RSS and scratch-disk probe (not allocator accounting).

Usage: measure_process.py --report report.json --scratch /new/scratch -- command args
RSS is summed across the process tree and may double-count shared pages. Sampling
can miss short peaks. Scratch must be dedicated to this run; existing files are
never removed. Child stdout/stderr are inherited. No shell is used.
Optional --gpu samples NVIDIA memory for the same process tree once per second;
driver process counters are separate from allocator accounting and may overlap.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def tree_usage(root):
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
    return sum(processes.get(pid, (0, 0))[1] for pid in members), members


def gpu_memory(members):
    output = subprocess.check_output([
        'nvidia-smi', '--query-compute-apps=pid,used_gpu_memory',
        '--format=csv,noheader,nounits',
    ], text=True, stderr=subprocess.PIPE, timeout=2)
    total = 0
    for line in output.splitlines():
        pid, memory = line.split(',')
        if int(pid.strip()) in members:
            total += int(memory.strip()) * 1024 * 1024
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('--gpu', action='store_true', help='sample NVIDIA process-tree memory once per second')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.report.exists() or args.scratch.exists():
        parser.error('provide a command and new report/scratch paths')
    args.scratch.mkdir(parents=True)
    started = time.monotonic()
    child = subprocess.Popen(command, env={**os.environ, 'TMPDIR': str(args.scratch.resolve())}, start_new_session=True)
    peak_rss = peak_disk = samples = 0
    peak_gpu = None
    gpu_samples = 0
    gpu_error = None
    next_gpu = started
    timed_out = False
    try:
        while child.poll() is None:
            rss, members = tree_usage(child.pid)
            peak_rss = max(peak_rss, rss)
            if args.gpu and time.monotonic() >= next_gpu:
                next_gpu = time.monotonic() + 1
                try:
                    peak_gpu = max(peak_gpu or 0, gpu_memory(members))
                    gpu_samples += 1
                except (OSError, ValueError, subprocess.SubprocessError) as error:
                    gpu_error = str(error)
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
                           sampled_gpu_process_bytes=peak_gpu, gpu_samples=gpu_samples,
                           gpu_error=gpu_error, nominal_gpu_sample_interval_ms=1000 if args.gpu else None,
                           nominal_sample_interval_ms=50), report, indent=2)
    return code if code >= 0 else 1


if __name__ == '__main__':
    raise SystemExit(main())
