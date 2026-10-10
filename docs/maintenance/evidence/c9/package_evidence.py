"""Losslessly package completed generated evidence; leave fresh writer outputs alone."""
import gzip
import hashlib
import json
from pathlib import Path
import time

OUT = Path(__file__).resolve().parent
INDEX = OUT / 'artifact-formats.json'
records = json.loads(INDEX.read_text()) if INDEX.exists() else []
for path in sorted(OUT.rglob('*')):
    if not path.is_file() or time.time() - path.stat().st_mtime < 30:
        continue
    if path.suffix == '.json' and path.stat().st_size > 1024 * 1024:
        original = path.read_bytes()
        compressed = gzip.compress(original, mtime=0)
        assert gzip.decompress(compressed) == original
        destination = path.with_suffix('.json.gz')
        with destination.open('xb') as file:
            file.write(compressed)
        records.append(dict(original=str(path.relative_to(OUT)), path=str(destination.relative_to(OUT)),
                            original_sha256=hashlib.sha256(original).hexdigest(),
                            sha256=hashlib.sha256(compressed).hexdigest(), lossless='identical bytes after decompression'))
        path.unlink()
    elif path.suffix == '.ppm':
        from PIL import Image
        destination = path.with_suffix('.png')
        assert not destination.exists()
        raw_hash = hashlib.sha256(path.read_bytes()).hexdigest()
        with Image.open(path) as original:
            original.save(destination)
            with Image.open(destination) as decoded:
                assert decoded.mode == original.mode and decoded.size == original.size
                assert decoded.tobytes() == original.tobytes()
        records.append(dict(original=str(path.relative_to(OUT)), path=str(destination.relative_to(OUT)),
                            original_sha256=raw_hash, sha256=hashlib.sha256(destination.read_bytes()).hexdigest(),
                            lossless='identical decoded pixels and dimensions'))
        path.unlink()
INDEX.write_text(json.dumps(records, indent=2)+'\n')
print(f'{len(records)} lossless artifact conversions recorded')
