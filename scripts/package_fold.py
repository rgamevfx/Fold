#!/usr/bin/env python3
"""Build a relocatable Linux reference package, including desktop, CLI and OCIO.

Usage: python3 scripts/package_fold.py /absolute/build-dir /absolute/new-package
Uses the locked Cargo graph and the pinned native compiler/CMake versions.
Includes the isolated LGPL-only native-video helper/runtime. The separate FFmpeg
executable, NVIDIA driver (for native decode), Vulkan, audio and system C/C++
libraries remain declared OS prerequisites.
"""
import hashlib
import json
import os
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
    video_root = build / "video-runtime"
    subprocess.run([sys.executable, str(ROOT / "scripts/build_video_runtime.py"), "--work", str(build / "video-build"), "--prefix", str(video_root)], check=True, timeout=1800)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "fold-native-video", "--features", "helper", "--bin", "fold-video-helper"], cwd=ROOT, env=dict(os.environ, FOLD_VIDEO_ROOT=str(video_root)), check=True, timeout=1800)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "fold-app", "--features", "desktop", "--bins"], cwd=ROOT, check=True, timeout=1800)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT))
    binary = Path(metadata["target_directory"]) / "release"
    (package / "bin").mkdir()
    manifest_path = package / "share/fold/color/manifest.json"
    manifest = json.loads(manifest_path.read_text())
    for name in ("fold-cli", "fold-desktop", "fold-video-helper"):
        shutil.copy2(binary / name, package / "bin" / name)
        manifest["files"][f"bin/{name}"] = hashlib.sha256((package / "bin" / name).read_bytes()).hexdigest()
    for pattern in ("libavcodec.so*", "libavformat.so*", "libavutil.so*"):
        for source in sorted((video_root / "lib").glob(pattern)):
            shutil.copy2(source, package / "lib" / source.name, follow_symlinks=False)
    shutil.copytree(video_root / "share/fold-video", package / "share/fold-video")
    for root in (package / "lib", package / "share/fold-video"):
        for path in sorted(root.rglob("*")):
            if path.is_file():
                manifest["files"][str(path.relative_to(package))] = hashlib.sha256(path.read_bytes()).hexdigest()
    manifest["video_runtime"] = json.loads((package / "share/fold-video/runtime.json").read_text())
    notices = package / "share/fold/licenses"
    notices.mkdir(parents=True)
    for source in sorted((ROOT / "resources/licenses").iterdir()):
        if not source.is_file():
            continue
        target = notices / source.name
        shutil.copy2(source, target)
        manifest["files"][str(target.relative_to(package))] = hashlib.sha256(target.read_bytes()).hexdigest()
    manifest["revision"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    manifest["dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT))
    manifest["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip()
    sources = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    sources += sorted((ROOT / "crates").rglob("*.rs"))
    sources += sorted((ROOT / "crates").rglob("*.wgsl"))
    sources += sorted((ROOT / "crates").rglob("Cargo.toml"))
    sources += [ROOT / "crates/color/native/bridge.cpp"]
    sources += sorted((ROOT / "scripts").glob("*.py"))
    sources += sorted((ROOT / "resources/licenses").glob("*"))
    manifest["sources"] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Built {package}/bin/fold-desktop and fold-cli")


if __name__ == "__main__":
    main()
