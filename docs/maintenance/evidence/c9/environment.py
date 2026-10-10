import hashlib
import json
import platform
import shutil
import subprocess
from pathlib import Path
from checkpoint import ROOT, OUT, ENV

baseline = json.loads((OUT.parent / 'c0/environment.json').read_text())
runtime = Path(ENV['FOLD_COLOR_ROOT'])
record = dict(base=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              initial_worktree='clean', platform=platform.platform(),
              configuration={k:v for k,v in ENV.items() if k.startswith(('CARGO_', 'FOLD_'))},
              runtime_hashes={p:hashlib.sha256((runtime/p).read_bytes()).hexdigest() for p in baseline['runtime_hashes']},
              disk_free_bytes=shutil.disk_usage(ROOT).free)
record['runtime_matches_c0'] = record['runtime_hashes'] == baseline['runtime_hashes']
for command in (['rustc','-Vv'], ['cargo','-V'], ['ffmpeg','-version']):
    record[command[0]] = subprocess.check_output(command, text=True)
(OUT / 'environment.json').write_text(json.dumps(record, indent=2)+'\n')
