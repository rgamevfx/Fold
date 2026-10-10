"""Explicit prerequisite-qualified tests from the final workspace executables.

Run on the host, serially, after checkpoint.py. Does not play audio or enable
fixture/helper/performance tests without their external prerequisites.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import time
from checkpoint import ROOT, OUT, ENV, CARGO


def run(name, command, results, timeout=600):
    start = time.monotonic()
    with (OUT / f'{name}.log').open('x') as log:
        try:
            result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT, timeout=timeout)
            status = result.returncode
        except subprocess.TimeoutExpired:
            status = 'timeout'
    results.append(dict(name=name, command=command, status=status, seconds=round(time.monotonic()-start, 2)))
    (OUT / 'native-results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(name, status, flush=True)


if __name__ == '__main__':
    results = []
    capture_started = time.time_ns()
    run('host-vulkan', ['vulkaninfo', '--summary'], results)
    hardware = (OUT / 'host-vulkan.log').read_text()
    if 'NVIDIA GeForce GTX 1070' not in hardware:
        raise SystemExit('Required reference GPU is unavailable; no hardware qualification claimed')
    if not Path(ENV.get('FOLD_VIDEO_HELPER', '')).is_file():
        raise SystemExit('Set FOLD_VIDEO_HELPER to the current helper in its package-local runtime layout; see helper_replay.py')
    ENV.update(FOLD_NATIVE_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/background.mp4',
               FOLD_NATIVE_SECOND_FIXTURE='/home/rgame/Documents/fold-native-pipeline/sustained/overlay.mp4',
               FOLD_M5_RENDER_DIR=str(OUT / 'integrated-frames'),
               FOLD_M5_EVIDENCE_DIR=str(OUT / 'integrated-public'))
    # Rebuild/replay C0 against the final source identity after the focused
    # retirement assertion was corrected during workspace compilation.
    targets = ['current_contracts', 'project_ingest', 'project_placement',
               'nle_workflow', 'common_composition', 'agent_project', 'agent_handoff',
               'project_lifecycle', 'viewer_transport', 'agent_integrated',
               'motion_integrated', 'motion_spatial', 'panel_linking', 'agent_frames']
    command = ['cargo', 'test', *CARGO, '-p', 'fold-app', '--features', 'native-probe', '--no-fail-fast']
    for target in targets:
        command += ['--test', target]
    run('final-c0-workflows', command + ['--', '--test-threads=1'], results, timeout=2400)
    inventory = json.loads((OUT / 'ignored-tests.json').read_text())
    eligible = {row['test'] for row in inventory if row['group'] in {
        'native-gpu-or-offscreen-ui', 'packaged-color-runtime', 'ffmpeg', 'native-helper-or-external-fixture'}
        and row['test'] not in {'render_efficiency_profile', 'alternating_viewers_preserve_two_source_decode_locality'}}
    log = (OUT / 'workspace-tests.log').read_text() + (OUT / 'final-c0-workflows.log').read_text()
    binaries = sorted(set(re.findall(r'Running .*?\((target/debug/deps/[^)]+)\)', log)))
    # Replay only sandbox failures not already covered by the final C0 replay.
    recovered = set(re.findall(r'Running .*?\((target/debug/deps/[^)]+)\)', (OUT / 'final-c0-workflows.log').read_text())) if results[-1]['status'] == 0 else set()
    current = None
    failures = []
    for line in (OUT / 'workspace-tests.log').read_text().splitlines():
        if match := re.search(r'Running .*?\((target/debug/deps/[^)]+)\)', line):
            current = match.group(1)
        if match := re.match(r'test (.+) \.\.\. FAILED$', line):
            if current not in recovered:
                failures.append((current, match.group(1)))
    for index, (binary, name) in enumerate(failures):
        run(f'environment-retry-{index:03d}', [str(ROOT / binary), name, '--exact', '--test-threads=1', '--nocapture'], results)
    selected = []
    for binary in binaries:
        listing = subprocess.check_output([str(ROOT / binary), '--ignored', '--list'], cwd=ROOT, env=ENV, text=True)
        names = [name for name in re.findall(r'^(.+): test$', listing, re.M) if name.split('::')[-1] in eligible]
        binary_hash = hashlib.sha256((ROOT / binary).read_bytes()).hexdigest() if names else None
        for name in names:
            selected.append(dict(binary=binary, test=name, sha256=binary_hash))
    (OUT / 'native-selection.json').write_text(json.dumps(selected, indent=2) + '\n')
    for index, case in enumerate(selected):
        run(f'native-{index:03d}', [str(ROOT / case['binary']), case['test'], '--ignored', '--exact', '--test-threads=1', '--nocapture'], results)

    run('desktop-capture', ['xvfb-run', '-a', 'python3', 'docs/maintenance/native_capture.py',
                           str(OUT / 'desktop'), '--desktop', str(ROOT / 'target/debug/fold-desktop')], results, timeout=120)
    captures = []
    capture_roots = [Path('/tmp') / name for name in ('fold-selection-diagnosis', 'fold-common-composition-ui', 'fold-shuffle-ui', 'fold-timeline-ui', 'fold-settings-ui', 'fold-node-ui', 'fold58-settings-ui')]
    capture_roots += list(Path('/tmp').glob('fold-motion-*.ppm'))
    for path in capture_roots:
        if not path.exists():
            continue
        files = path.rglob('*.ppm') if path.is_dir() else [path]
        for file in files:
            if file.suffix == '.ppm' and file.stat().st_mtime_ns >= capture_started:
                destination = OUT / 'captures' / file.relative_to('/tmp')
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(file, destination)
                captures.append(dict(path=str(destination.relative_to(OUT)), sha256=hashlib.sha256(destination.read_bytes()).hexdigest()))
    (OUT / 'captures.json').write_text(json.dumps(captures, indent=2)+'\n')
