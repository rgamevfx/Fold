"""Reproduce C9's serial workspace checkpoint with per-command durable logs."""
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[4]
OUT = Path(__file__).resolve().parent
ENV = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
           FOLD_COLOR_ROOT='/home/rgame/Documents/fold-native-pipeline/release-package',
           FOLD_TEST_COLOR_ROOT='/home/rgame/Documents/fold-native-pipeline/release-package',
           FOLD_VIDEO_ROOT='/home/rgame/Documents/fold-native-pipeline/release-package',
           FOLD_SLICE_CAPTURE_DIR=str(OUT / 'public-slice'),
           FOLD_M5_HANDOFF_DIR=str(OUT / 'public-handoff'))
CARGO = ['--offline', '--locked']
COMMANDS = [
    ('format', ['cargo', 'fmt', '--all', '--check']),
    ('boundaries', ['python3', 'scripts/check_boundaries.py']),
    ('ownership', ['python3', 'scripts/check_proposal_ownership.py']),
    ('python', ['python3', '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py']),
    ('workspace-check', ['cargo', 'check', *CARGO, '--workspace', '--all-targets', '--features', 'fold-app/native-probe']),
    ('workspace-tests', ['cargo', 'test', *CARGO, '--workspace', '--features', 'fold-app/native-probe', '--no-fail-fast', '--', '--test-threads=1']),
    ('clippy', ['cargo', 'clippy', *CARGO, '--workspace', '--all-targets', '--features', 'fold-app/native-probe']),
    ('headless-check', ['cargo', 'check', *CARGO, '-p', 'fold-app', '--no-default-features', '--all-targets']),
    ('final-build', ['cargo', 'build', *CARGO, '-p', 'fold-app', '--features', 'native-probe', '--bins']),
]

if __name__ == '__main__':
    results = []
    for name, command in COMMANDS:
        start = time.monotonic()
        with (OUT / f'{name}.log').open('x') as log:
            result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT)
        results.append(dict(name=name, command=command, status=result.returncode, seconds=round(time.monotonic()-start, 2)))
        (OUT / 'checkpoint.json').write_text(json.dumps(results, indent=2) + '\n')
        print(name, result.returncode, flush=True)
