"""Summarize the native shared probe; fail on missed phase-19 acceptance."""
import argparse
import json
import math
from pathlib import Path


def summarize(report):
    failures = []
    if not report.get('success'):
        failures.append('native workload did not complete')
    shared = report.get('shared') or {}
    events = shared.get('events', [])
    starts = {e['phase']: e['at_ms'] for e in events if e['kind'] == 'phase'}
    rows = []
    phases = {p['phase']: p for p in shared.get('phases', [])}
    for phase in (1, 2):
        if phase not in phases or phase not in starts:
            failures.append(f'missing phase {phase}')
            continue
        p = phases[phase]
        seconds = p['seconds']
        samples = [s for s in report['samples']
                   if starts[phase] <= s['start_ms'] < starts[phase] + seconds * 1000]
        # draw_ms already spans acquisition through the present call.
        times = sorted(s['draw_ms'] for s in samples)
        p95 = times[math.ceil(len(times) * .95) - 1] if times else None
        # At most two partial boundary frames, not an allowance for steady drops.
        minimum = math.floor(seconds * 30) - 2
        if p['admissions'][0] < minimum:
            failures.append(f'phase {phase}: {p["admissions"][0]} admissions; need >= {minimum}')
        if p95 is None or p95 >= 16.7:
            failures.append(f'phase {phase}: UI P95 {p95} ms')
        if any(p.get('audio_errors', [])) or any(v for v in p['max_observed_underrun_callbacks']):
            failures.append(f'phase {phase}: audio error or underrun')
        if phase == 2 and (p['skipped_between_admissions'][1] or p['admissions'][1] < 2):
            failures.append('Every-frame did not advance consecutively')
        rows.append(dict(phase=phase, seconds=seconds, admissions=p['admissions'],
                         gaps=p['skipped_between_admissions'], ui_p95_ms=p95,
                         real_time_fps=p['admissions'][0] / seconds))
    seeks = [e['through_present_call_ms'] for e in events
             if e['kind'] == 'seek' and e['warm_repeat']]
    if len(seeks) != 3 or max(seeks) >= 100:
        failures.append('warm seek acceptance failed or missing')
    exports = [e for e in events if e['kind'] == 'export']
    if len(exports) != 1 or exports[0]['frames'] != 150 or exports[0]['through_completion_observed_seconds'] >= 60:
        failures.append('150-frame export did not finish within the existing mixed-phase deadline')
    return dict(passed=not failures, failures=failures, phases=rows, warm_seeks_ms=seeks, exports=exports)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    result = summarize(json.loads(args.report.read_text()))
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['passed'] else 1)
