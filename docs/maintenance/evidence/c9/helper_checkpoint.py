"""Qualify the current helper against the available pinned development SDK."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
from checkpoint import ROOT, OUT, ENV, CARGO

SDK = Path('/home/rgame/Documents/fold-native-pipeline/native-worker/runtime')
ENV['FOLD_VIDEO_ROOT'] = str(SDK)
ENV['LD_LIBRARY_PATH'] = str(SDK / 'lib')
ENV['FOLD_VIDEO_HELPER'] = str(ROOT / 'target/debug/fold-video-helper')
ENV['FOLD_NATIVE_FIXTURE'] = '/home/rgame/Documents/fold-native-pipeline/sustained/background.mp4'
ENV['FOLD_NATIVE_SECOND_FIXTURE'] = '/home/rgame/Documents/fold-native-pipeline/sustained/overlay.mp4'
results = []

def run(name, command):
    start = time.monotonic()
    with (OUT / f'{name}.log').open('x') as log:
        result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    results.append(dict(name=name, command=command, status=result.returncode, seconds=round(time.monotonic()-start, 2)))
    (OUT / 'helper-results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(name, result.returncode, flush=True)
    return result.returncode

for mode in ['check', 'clippy', 'test']:
    run('helper-' + mode, ['cargo', mode, *CARGO, '-p', 'fold-native-video', '--all-targets', '--all-features'])
status = run('helper-build', ['cargo', 'build', *CARGO, '-p', 'fold-native-video', '--features', 'helper', '--bin', 'fold-video-helper'])
if status == 0:
    paths = [SDK / 'share/fold-video/runtime.json', *sorted((SDK / 'lib').glob('libav*.so.*')),
             Path(ENV['FOLD_VIDEO_HELPER'])]
    (OUT / 'helper-environment.json').write_text(json.dumps(dict(root=str(SDK), helper=ENV['FOLD_VIDEO_HELPER'],
        ld_library_path=ENV['LD_LIBRARY_PATH'], hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}), indent=2)+'\n')
    for case in json.loads((OUT / 'native-results.json').read_text()):
        if case['name'].startswith('native-') and case['status'] != 0 and 'native NVDEC helper missing:' in (OUT / f"{case['name']}.log").read_text():
            run('current-helper-' + case['name'], case['command'])

control = ROOT / 'target/debug/deps/color_workflow-9f37cbb6e514eab4'
(OUT / 'color-control.json').write_text(json.dumps(dict(path=str(control), sha256=hashlib.sha256(control.read_bytes()).hexdigest(),
    mtime_ns=control.stat().st_mtime_ns, provenance='docs/rewrite/evidence/m5a-selection-linking/workspace-tests-final.log'), indent=2)+'\n')
run('pre-maintenance-color-control', [str(control), 'persisted_color_undo_legacy_and_matched_cli_preview_output', '--ignored', '--exact', '--test-threads=1', '--nocapture'])
