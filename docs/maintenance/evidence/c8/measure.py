"""Run the frozen CPU profile serially after building maintenance_profile.

Usage: python3 docs/maintenance/evidence/c8/measure.py before|after|after-repeat
No builds or tests should overlap these processes.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

DEST = Path(__file__).resolve().parent
ROOT = DEST.parents[3]
BINARY = ROOT / 'target/release/examples/maintenance_profile'
label = sys.argv[1]
assert label in ('before', 'after', 'after-repeat')
env = dict(os.environ, FOLD_COLOR_ROOT='/home/rgame/Documents/fold-native-pipeline/release-package',
           FOLD_VIDEO_ROOT='/home/rgame/Documents/fold-native-pipeline/release-package')
for run in range(1, 4):
    stem = DEST / f'{label}-{run}'
    with stem.with_suffix('.json').open('w') as output, stem.with_suffix('.log').open('w') as log:
        subprocess.run(['/usr/bin/time', '-v', str(BINARY)], cwd=ROOT, env=env,
                       stdout=output, stderr=log, check=True)
(DEST / f'{label}-binary.json').write_text(json.dumps({
    'sha256': hashlib.sha256(BINARY.read_bytes()).hexdigest(),
    'source_sha256': json.loads((DEST / f'{label}-1.json').read_text())['source_sha256'],
}, indent=2) + '\n')
