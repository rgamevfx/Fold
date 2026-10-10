"""Resolve native environment setup failures and compare new findings with C0."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
from checkpoint import ROOT, OUT, ENV

ENV.update(FOLD_VIDEO_HELPER='/home/rgame/Documents/fold-native-pipeline/release-package/bin/fold-video-helper',
           FOLD_NATIVE_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/background.mp4',
           FOLD_NATIVE_SECOND_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/overlay.mp4')
results = []

def run(label, command):
    start = time.monotonic()
    with (OUT / f'{label}.log').open('x') as log:
        result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    results.append(dict(name=label, command=command, status=result.returncode,
                        seconds=round(time.monotonic()-start, 2),
                        binary_sha256=hashlib.sha256(Path(command[0]).read_bytes()).hexdigest(),
                        fixture=ENV['FOLD_NATIVE_FIXTURE'], helper=ENV['FOLD_VIDEO_HELPER']))
    (OUT / 'followup-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(label, result.returncode, flush=True)
    return result.returncode

for case in json.loads((OUT / 'native-results.json').read_text()):
    if not case['name'].startswith('native-') or case['status'] == 0:
        continue
    log = (OUT / f"{case['name']}.log").read_text()
    if 'native NVDEC helper missing:' in log or case['command'][1] in {
        'persisted_color_undo_legacy_and_matched_cli_preview_output',
        'native::tests::timeout_crash_and_inflight_cancel_reap_helpers'}:
        status = run('retry-' + case['name'], case['command'])
        if status != 0 and case['command'][1] == 'native::tests::timeout_crash_and_inflight_cancel_reap_helpers':
            ENV['FOLD_NATIVE_FIXTURE'] = '/home/rgame/Documents/fold-native-pipeline/native-worker/fractional64.mp4'
            run('small-fixture-' + case['name'], case['command'])
            ENV['FOLD_NATIVE_FIXTURE'] = '/home/rgame/Documents/fold-native-pipeline/sustained/background.mp4'

# This exact executable is recorded in C0/preview-gpu.log and predates C1.
control = ROOT / 'target/debug/deps/motion_spatial-d348458dcce3abdd'
control_record = dict(path=str(control), sha256=hashlib.sha256(control.read_bytes()).hexdigest(),
                      mtime_ns=control.stat().st_mtime_ns,
                      provenance='docs/maintenance/evidence/c0/preview-gpu.log')
(OUT / 'c0-control.json').write_text(json.dumps(control_record, indent=2)+'\n')
for case in json.loads((OUT / 'native-results.json').read_text()):
    if case['status'] != 0 and 'motion_spatial-' in case['command'][0]:
        run('c0-control-' + case['name'], [str(control), *case['command'][1:]])
