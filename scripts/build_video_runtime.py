#!/usr/bin/env python3
"""Build the pinned LGPL-only NVDEC decoder runtime outside the repository.

Usage: build_video_runtime.py --work /absolute/scratch --prefix /absolute/runtime
Requires make, C compiler, pkg-config. CPU assembler is disabled (NVDEC only). No system FFmpeg libraries are used.
Installed shared libraries remain replaceable; source archives and licenses are
installed alongside them for downstream packaging/source-offer compliance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

SOURCES = [
    ("ffmpeg-6.1.1.tar.xz", "https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz",
     "8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968"),
    ("nv-codec-headers.tar.gz", "https://codeload.github.com/FFmpeg/nv-codec-headers/tar.gz/refs/tags/n12.1.14.0",
     "2fefaa227d2a3b4170797796425a59d1dd2ed5fd231db9b4244468ba327acd0b"),
]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--prefix", type=Path, required=True)
    args = parser.parse_args()
    work, prefix = args.work.resolve(), args.prefix.resolve()
    repo = Path(__file__).resolve().parents[1]
    if work.is_relative_to(repo) or prefix.is_relative_to(repo):
        parser.error("build and runtime must live outside the repository")
    work.mkdir(parents=True, exist_ok=True)
    prefix.mkdir(parents=True, exist_ok=True)
    for name, url, digest in SOURCES:
        archive = work / name
        if not archive.exists():
            urllib.request.urlretrieve(url, archive)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != digest:
            raise RuntimeError(f"source hash mismatch: {archive}")
        with tarfile.open(archive) as tar:
            tar.extractall(work, filter="data")
    env = dict(os.environ, PKG_CONFIG_PATH=str(prefix / "lib/pkgconfig"))
    def run(command, cwd):
        subprocess.run(command, cwd=cwd, env=env, check=True)
    headers = work / "nv-codec-headers-n12.1.14.0"
    run(["make", "install", f"PREFIX={prefix}"], headers)
    source = work / "ffmpeg-6.1.1"
    flags = [f"--prefix={prefix}", "--disable-everything", "--disable-autodetect",
             "--disable-static", "--enable-shared", "--disable-programs", "--disable-doc", "--disable-x86asm",
             "--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-network",
             "--disable-avdevice", "--disable-avfilter", "--disable-swscale", "--disable-swresample",
             "--enable-avcodec", "--enable-avformat", "--enable-avutil", "--enable-decoder=h264",
             "--enable-parser=h264", "--enable-demuxer=mov", "--enable-protocol=file",
             "--enable-ffnvcodec", "--enable-cuda", "--enable-nvdec", "--enable-hwaccel=h264_nvdec"]
    run([str(source / "configure"), *flags], source)
    run(["make", f"-j{min(os.cpu_count() or 1, 8)}"], source)
    run(["make", "install"], source)
    licenses = prefix / "share/fold-video/licenses"
    licenses.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / "COPYING.LGPLv2.1", licenses)
    shutil.copy2(source / "LICENSE.md", licenses / "FFmpeg-LICENSE.md")
    shutil.copy2(headers / "include/ffnvcodec/dynlink_cuda.h", licenses / "nv-codec-headers-license.h")
    sources = prefix / "share/fold-video/sources"
    sources.mkdir(parents=True, exist_ok=True)
    for name, _, _ in SOURCES:
        shutil.copy2(work / name, sources)
    shutil.copy2(__file__, sources)
    (prefix / "share/fold-video/runtime.json").write_text(json.dumps({
        "ffmpeg": "6.1.1", "abi": {"avcodec": 60, "avformat": 60, "avutil": 58},
        "license": "LGPL-2.1-or-later", "configure": flags, "sources": SOURCES,
    }, indent=2) + "\n")

if __name__ == "__main__":
    main()
