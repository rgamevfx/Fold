#!/usr/bin/env python3
"""Summarize render/tests/efficiency.rs logs; first 30 frames are cold/warm-up.

Usage: summarize_efficiency.py run.log [run.log ...]
Outputs independent cold and warm distributions, never merges repeated runs.
"""
import json
import math
from pathlib import Path
import re
import sys


def distribution(values):
    values = sorted(values)
    return dict(mean=sum(values) / len(values), p50=values[math.ceil(len(values) * .5)-1],
                p95=values[math.ceil(len(values) * .95)-1], maximum=values[-1])


def summarize(path):
    groups = {}
    for line in Path(path).read_text().splitlines():
        if not line.startswith('PROFILE '):
            continue
        _, kind, cached, frame, rest = line.split(' ', 4)
        key = f'{kind}/{"repeated" if cached == "true" else "advancing"}/{"cold" if int(frame) < 30 else "warm"}'
        values = {k: float(v) for k, v in re.findall(r'(\w+)(?:=|: )(\d+(?:\.\d+)?)', rest)}
        groups.setdefault(key, []).append(values)
    return {key: {'frames': len(rows), 'metrics': {
        name: distribution([row[name] for row in rows]) for name in rows[0]
    }} for key, rows in groups.items()}


if __name__ == '__main__':
    print(json.dumps({path: summarize(path) for path in sys.argv[1:]}, indent=2))
