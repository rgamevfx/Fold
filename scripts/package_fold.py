#!/usr/bin/env python3
"""Build a relocatable Linux reference package, including desktop, CLI and OCIO.

Usage: python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package
Uses the locked Cargo graph and the pinned native compiler/CMake versions.
FFmpeg, Vulkan, audio and system C/C++ libraries remain declared OS prerequisites.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    build, package = map(Path, sys.argv[1:])
    if not build.is_absolute() or not package.is_absolute() or package.exists():
        raise SystemExit("absolute build path and a new absolute package path required")
    subprocess.run([sys.executable, str(ROOT / "scripts/build_color_runtime.py"), str(build), str(package)], check=True, timeout=3600)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "fold-app", "--features", "desktop", "--bins"], cwd=ROOT, check=True, timeout=1800)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT))
    binary = Path(metadata["target_directory"]) / "release"
    (package / "bin").mkdir()
    manifest_path = package / "share/fold/color/manifest.json"
    manifest = json.loads(manifest_path.read_text())
    for name in ("fold-cli", "fold-desktop"):
        shutil.copy2(binary / name, package / "bin" / name)
        manifest["files"][f"bin/{name}"] = hashlib.sha256((package / "bin" / name).read_bytes()).hexdigest()
    manifest["revision"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    manifest["dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT))
    manifest["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip()
    sources = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    sources += sorted((ROOT / "crates").rglob("*.rs"))
    sources += sorted((ROOT / "crates").rglob("*.wgsl"))
    sources += sorted((ROOT / "crates").rglob("Cargo.toml"))
    sources += [ROOT / "crates/color/native/bridge.cpp"]
    manifest["sources"] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Built {package}/bin/fold-desktop and fold-cli")


if __name__ == "__main__":
    main()
