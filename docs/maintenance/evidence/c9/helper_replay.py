"""Run the helper with its package-local RPATH; never override system ffprobe libs."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from checkpoint import ROOT, OUT, ENV

SDK = Path('/home/rgame/Documents/fold-native-pipeline/native-worker/runtime')
package = Path(tempfile.mkdtemp(prefix='fold-c9-native-package-'))
(package / 'bin').mkdir()
(package / 'lib').symlink_to(SDK / 'lib', target_is_directory=True)
helper = package / 'bin/fold-video-helper'
shutil.copy2(ROOT / 'target/debug/fold-video-helper', helper)
ENV.pop('LD_LIBRARY_PATH', None)
ENV.update(FOLD_VIDEO_ROOT=str(SDK), FOLD_VIDEO_HELPER=str(helper),
           FOLD_NATIVE_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/background.mp4',
           FOLD_NATIVE_SECOND_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/overlay.mp4')
(OUT / 'helper-package.json').write_text(json.dumps(dict(package=str(package), lib=str(SDK / 'lib'),
    helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(), loader='package-local $ORIGIN/../lib RPATH; LD_LIBRARY_PATH unset'),indent=2)+'\n')
results = []
for case in json.loads((OUT / 'native-results.json').read_text()):
    if not case['name'].startswith('native-') or case['status'] == 0 or 'native NVDEC helper missing:' not in (OUT / f"{case['name']}.log").read_text():
        continue
    name = 'packaged-helper-' + case['name']
    start = time.monotonic()
    with (OUT / f'{name}.log').open('x') as log:
        result = subprocess.run(case['command'], cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    results.append(dict(name=name, command=case['command'], status=result.returncode, seconds=round(time.monotonic()-start,2)))
    (OUT / 'helper-replay-results.json').write_text(json.dumps(results,indent=2)+'\n')
    print(name, result.returncode, flush=True)
control = ROOT / 'target/debug/deps/color_workflow-9f37cbb6e514eab4'
command = [str(control), 'persisted_color_undo_legacy_and_matched_cli_preview_output', '--ignored', '--exact', '--test-threads=1', '--nocapture']
start = time.monotonic()
with (OUT / 'color-control-final.log').open('x') as log:
    result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=90)
results.append(dict(name='color-control-final',command=command,status=result.returncode,seconds=round(time.monotonic()-start,2)))
(OUT / 'helper-replay-results.json').write_text(json.dumps(results,indent=2)+'\n')
print('color-control-final', result.returncode, flush=True)
