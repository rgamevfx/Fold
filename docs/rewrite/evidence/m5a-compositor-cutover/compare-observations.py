#!/usr/bin/env python3
"""Reproduce the historical observation evidence audit; requires Pillow."""
from pathlib import Path
from PIL import Image
import hashlib
import json

root = Path(__file__).resolve().parent.parent / "m5a-public-scene"
images = {name: Image.open(root / name).convert("RGBA") for name in (
    "first-start-missing-spheres.png", "reveal.png", "held.png"
)}
box = (0, 230, 960, 540)
regions = {name: image.crop(box).tobytes() for name, image in images.items()}
reference = regions["held.png"]
result = {"region": box, "frames": {name: {
    "sha256": hashlib.sha256(data).hexdigest(),
    "different_bytes_from_held": sum(a != b for a, b in zip(data, reference)),
    "distinct_rgba_pixels": len(set(zip(*[iter(data)] * 4))),
} for name, data in regions.items()}}
assert all(data == reference for data in regions.values())
assert result["frames"]["held.png"]["distinct_rgba_pixels"] > 100
print(json.dumps(result, indent=2))
