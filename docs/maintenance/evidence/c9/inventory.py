"""Inventory explicit Rust ignores and prerequisites without enabling them."""
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[4]
OUT = Path(__file__).resolve().parent
rows = []
for path in sorted((ROOT / 'crates').rglob('*.rs')):
    source = path.read_text()
    for match in re.finditer(r'#\[ignore(?:\s*=\s*"([^"]*)")?\]', source):
        name = re.search(r'fn\s+(\w+)', source[match.end():]).group(1)
        reason = match.group(1) or 'unspecified'
        lower = reason.lower()
        if any(term in lower for term in ('fixture', 'helper', 'nvdec', 'cuda', 'performance regression', 'release native')) and 'destroys parent device' not in lower:
            group = 'native-helper-or-external-fixture'
        elif 'output device' in lower:
            group = 'physical-audio-output'
        elif any(term in lower for term in ('vulkan', 'gpu', 'capture')):
            group = 'native-gpu-or-offscreen-ui'
        elif 'ocio' in lower:
            group = 'packaged-color-runtime'
        elif 'ffmpeg' in lower:
            group = 'ffmpeg'
        else:
            group = 'unassigned'
        rows.append(dict(path=str(path.relative_to(ROOT)), line=source[:match.start()].count('\n') + 1,
                         test=name, prerequisite=reason, group=group))
(OUT / 'ignored-tests.json').write_text(json.dumps(rows, indent=2) + '\n')
print(json.dumps({group: sum(row['group'] == group for row in rows) for group in sorted({row['group'] for row in rows})}))
