#!/usr/bin/env python3
"""Build a relocatable Linux reference package, including desktop, CLI and OCIO.

Usage: python3 scripts/package_fold.py BUILD PACKAGE --motion-skia ARCHIVE
       --motion-ffmpeg ARCHIVE --motion-toolchain SYSROOT
Uses the locked Cargo graph and the pinned native compiler/CMake versions.
Includes the Motion SDK and isolated LGPL-only native-video helper/runtime. The separate FFmpeg
executable, NVIDIA driver (for native decode), Vulkan, audio and system C/C++
libraries remain declared OS prerequisites.
"""
import hashlib
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from stage_motion_sdk import stage as stage_motion_sdk

ROOT = Path(__file__).resolve().parents[1]


def crate_sources(root):
    """Source inputs only; isolated native workspaces have their own target trees."""
    sources = []
    for directory, children, files in os.walk(root):
        children[:] = [name for name in children if name not in ("target", ".git")]
        sources.extend(
            Path(directory) / name for name in files
            if name == "Cargo.toml" or name.endswith((".rs", ".wgsl"))
        )
    return sorted(sources)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", type=Path)
    parser.add_argument("package", type=Path)
    parser.add_argument("--motion-skia", type=Path, required=True)
    parser.add_argument("--motion-ffmpeg", type=Path, required=True)
    parser.add_argument("--motion-toolchain", type=Path, required=True)
    args = parser.parse_args()
    build, package = args.build, args.package
    if not build.is_absolute() or not package.is_absolute() or package.exists():
        raise SystemExit("absolute build path and a new absolute package path required")
    if not all(path.is_file() for path in (args.motion_skia, args.motion_ffmpeg,
        args.motion_toolchain / "bin/cargo", args.motion_toolchain / "bin/rustc")):
        raise SystemExit("Motion native archives and compiler toolchain must exist")
    subprocess.run([sys.executable, str(ROOT / "scripts/build_color_runtime.py"), str(build), str(package)], check=True, timeout=3600)
    sdk = package / "share/fold/motion-sdk"
    stage_motion_sdk(sdk, precompile=True, vendor=True, skia=args.motion_skia,
                     ffmpeg=args.motion_ffmpeg, toolchain=args.motion_toolchain)
    video_root = build / "video-runtime"
    subprocess.run([sys.executable, str(ROOT / "scripts/build_video_runtime.py"), "--work", str(build / "video-build"), "--prefix", str(video_root)], check=True, timeout=1800)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "fold-native-video", "--features", "helper", "--bin", "fold-video-helper"], cwd=ROOT, env=dict(os.environ, FOLD_VIDEO_ROOT=str(video_root)), check=True, timeout=1800)
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "fold-app", "--features", "desktop", "--bins"], cwd=ROOT,
                   env=dict(os.environ, FOLD_MOTION_SDK_RELATIVE="../share/fold/motion-sdk/crates/motion-fframes"),
                   check=True, timeout=1800)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT))
    binary = Path(metadata["target_directory"]) / "release"
    (package / "bin").mkdir()
    manifest_path = package / "share/fold/color/manifest.json"
    manifest = json.loads(manifest_path.read_text())
    sdk_manifest = json.loads((sdk / "sources.json").read_text())
    manifest["motion_sdk"] = {key: value for key, value in sdk_manifest.items() if key != "files"}
    for name, digest in sdk_manifest["files"].items():
        manifest["files"][f"share/fold/motion-sdk/{name}"] = digest
    manifest["files"]["share/fold/motion-sdk/sources.json"] = hashlib.sha256((sdk / "sources.json").read_bytes()).hexdigest()
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
    sources += crate_sources(ROOT / "crates")
    sources += [ROOT / "crates/color/native/bridge.cpp"]
    sources += sorted((ROOT / "scripts").glob("*.py"))
    sources += sorted((ROOT / "resources/licenses").glob("*"))
    manifest["sources"] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Built {package}/bin/fold-desktop and fold-cli")


if __name__ == "__main__":
    main()
