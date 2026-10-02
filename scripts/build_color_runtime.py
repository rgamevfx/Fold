#!/usr/bin/env python3
"""Build Fold's private OCIO 2.4.2 runtime. Build/install paths must be absolute.

Usage: python3 scripts/build_color_runtime.py /absolute/build /absolute/package
Package layout: bin/<Fold executables>, lib/fold/*, share/fold/color/*.
No globally installed OCIO or Python OCIO package is used by Fold.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

VERSION = "2.4.2"
SHA256 = "2d8f2c47c40476d6e8cea9d878f6601d04f6d5642b47018eaafa9e9f833f3690"
ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    subprocess.run(args, check=True, timeout=1800)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", type=Path)
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    if not args.build.is_absolute() or not args.package.is_absolute():
        parser.error("build and package paths must be absolute")
    if args.package.exists():
        parser.error("package destination must not exist (no overwrite or stale resources)")
    toolchain = json.loads((ROOT / "resources/color/toolchain.json").read_text())
    for name, command in (("compiler", "c++"), ("cmake", "cmake")):
        actual = subprocess.check_output([command, "--version"], text=True).splitlines()[0]
        if actual != toolchain[name]:
            raise RuntimeError(f"Pinned {name} required: {toolchain[name]}; found {actual}")
    args.build.mkdir(parents=True, exist_ok=True)
    archive = args.build / "ocio-2.4.2.tar.gz"
    if not archive.exists():
        with urllib.request.urlopen(
            f"https://github.com/AcademySoftwareFoundation/OpenColorIO/archive/refs/tags/v{VERSION}.tar.gz",
            timeout=120,
        ) as response:
            archive.write_bytes(response.read())
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError("OCIO source checksum mismatch")
    source = args.build / f"OpenColorIO-{VERSION}"
    if not source.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(args.build, filter="data")
    install = args.build / "install"
    build = args.build / "build"
    run("cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
        "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
        "-DOCIO_BUILD_APPS=OFF", "-DOCIO_BUILD_TESTS=OFF",
        "-DOCIO_BUILD_GPU_TESTS=OFF", "-DOCIO_BUILD_PYTHON=OFF",
        "-DOCIO_INSTALL_EXT_PACKAGES=ALL")
    run("cmake", "--build", str(build), "-j", "6")
    run("cmake", "--install", str(build))
    lib = args.package / "lib/fold"
    data = args.package / "share/fold/color"
    lib.mkdir(parents=True, exist_ok=True)
    data.mkdir(parents=True, exist_ok=True)
    shutil.copy2(install / "lib/libOpenColorIO.so.2.4.2", lib / "libOpenColorIO.so.2.4")
    run("c++", "-std=c++17", "-O2", "-fPIC", "-shared",
        str(ROOT / "crates/color/native/bridge.cpp"), f"-I{install / 'include'}",
        f"-L{install / 'lib'}", "-lOpenColorIO", "-Wl,-rpath,$ORIGIN", "-Wl,--disable-new-dtags",
        "-Wl,-z,defs", "-o", str(lib / "libfold_ocio.so"))
    for name in ("config.ocio", "OCIO-LICENSE", "ACES-CONFIG-LICENSE", "toolchain.json"):
        shutil.copy2(ROOT / "resources/color" / name, data / name)
    # Include the license texts from OCIO's pinned dependency source builds.
    notices = data / "third-party"
    notices.mkdir(exist_ok=True)
    for path in sorted((build / "ext").rglob("*")):
        if (path.is_file() and path.suffix != ".zip"
                and path.name.lower().startswith(("license", "copying"))):
            name = "_".join(path.relative_to(build / "ext").parts)
            shutil.copy2(path, notices / name)
    for dependency, header in (("xxHash", "xxhash.h"), ("sampleicc", "iccProfileReader.h")):
        shutil.copy2(source / "ext" / dependency / "src/include" / header,
                     notices / f"{dependency}-licensed-header.h")
    manifest = {
        "ocio": VERSION, "source_sha256": SHA256,
        "config": "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
        "compiler": subprocess.check_output(["c++", "--version"], text=True),
        "cmake": subprocess.check_output(["cmake", "--version"], text=True),
        "files": {str(p.relative_to(args.package)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for directory in (lib, data) for p in sorted(directory.rglob("*"))
                  if p.is_file() and p.name != "manifest.json"},
    }
    (data / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
