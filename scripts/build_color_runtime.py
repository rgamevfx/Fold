#!/usr/bin/env python3
"""Build Fold's private OCIO 2.4.2 runtime. Build/install paths must be absolute.

Usage: python3 scripts/build_color_runtime.py /absolute/build /absolute/package
Package layout: bin/<Fold executables>, lib/fold/*, share/fold/color/*.
No globally installed OCIO or Python OCIO package is used by Fold.
"""
import argparse
import os
import platform
import tempfile
import fcntl
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


def prepare_yaml_patch(source):
    module = source / "share/cmake/modules/install/Installyaml-cpp.cmake"
    text = module.read_text()
    marker = "        ExternalProject_Add(yaml-cpp_install\n"
    patch = ('            PATCH_COMMAND ${CMAKE_COMMAND} -DSOURCE_DIR=<SOURCE_DIR> -P '
             f'"{ROOT / "resources/color/patch-yaml-cpp.cmake"}"\n')
    if patch in text:
        return
    if text.count(marker) != 1:
        raise RuntimeError("Unexpected OCIO yaml-cpp build module; review the header backport")
    module.write_text(text.replace(marker, marker + patch))


def produce(build_dir, package):
    build_dir.mkdir(parents=True, exist_ok=True)
    archive = build_dir / "ocio-2.4.2.tar.gz"
    if not archive.exists():
        with urllib.request.urlopen(
            f"https://github.com/AcademySoftwareFoundation/OpenColorIO/archive/refs/tags/v{VERSION}.tar.gz",
            timeout=120,
        ) as response:
            temporary = archive.with_suffix(".download")
            temporary.write_bytes(response.read())
            temporary.replace(archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError("OCIO source checksum mismatch")
    source = build_dir / f"OpenColorIO-{VERSION}"
    if not source.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(build_dir, filter="data")
    prepare_yaml_patch(source)
    install = build_dir / "install"
    build = build_dir / "build"
    run("cmake", "-S", str(source), "-B", str(build), "-G", "Ninja",
        "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={install}",
        "-DOCIO_BUILD_APPS=OFF", "-DOCIO_BUILD_TESTS=OFF",
        "-DOCIO_BUILD_GPU_TESTS=OFF", "-DOCIO_BUILD_PYTHON=OFF",
        "-DOCIO_INSTALL_EXT_PACKAGES=ALL", "-DCMAKE_INSTALL_LIBDIR=lib")
    run("cmake", "--build", str(build), "-j", str(min(6, int(os.environ.get("NUM_JOBS", "6")))))
    run("cmake", "--install", str(build))
    lib = package / "lib/fold"
    data = package / "share/fold/color"
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
        "files": {str(p.relative_to(package)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for directory in (lib, data) for p in sorted(directory.rglob("*"))
                  if p.is_file() and p.name != "manifest.json"},
    }
    (data / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def fingerprint():
    digest = hashlib.sha256()
    for path in [Path(__file__), ROOT / "crates/color/native/bridge.cpp",
                 *sorted((ROOT / "resources/color").glob("*"))]:
        if path.is_file():
            digest.update(path.read_bytes())
    for command in ("c++", "cmake"):
        digest.update(subprocess.check_output([command, "--version"]))
    digest.update(platform.machine().encode())
    return digest.hexdigest()


def valid_cache(package, identity):
    try:
        manifest = json.loads((package / "share/fold/color/manifest.json").read_text())
        return (manifest.get("build_identity") == identity and bool(manifest["files"])
                and all(hashlib.sha256((package / name).read_bytes()).hexdigest() == expected
                        for name, expected in manifest["files"].items()))
    except (OSError, ValueError, KeyError):
        return False


def ensure(cache):
    """Serialize producers and publish only complete, content-verified packages."""
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / "lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        identity = fingerprint()
        package = cache / identity / "package"
        if not valid_cache(package, identity):
            package.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(dir=package.parent) as staging:
                staged = Path(staging) / "package"
                produce(package.parent / "build", staged)
                manifest_path = staged / "share/fold/color/manifest.json"
                manifest = json.loads(manifest_path.read_text())
                manifest["build_identity"] = identity
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
                if package.exists():
                    shutil.rmtree(package)
                staged.rename(package)
        return package


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", type=Path)
    parser.add_argument("package", type=Path, nargs="?")
    parser.add_argument("--ensure", action="store_true", help="prepare a reusable Cargo cache")
    args = parser.parse_args()
    if not args.build.is_absolute() or (args.package and not args.package.is_absolute()):
        parser.error("build and package paths must be absolute")
    for tool in ("c++", "cmake", "ninja"):
        if shutil.which(tool) is None:
            parser.error(f"required build tool missing: {tool}")
    if platform.system() != "Linux":
        parser.error("the native color runtime currently supports Linux")
    if args.ensure:
        if args.package:
            parser.error("--ensure takes only the cache directory")
        package = ensure(args.build)
        (args.build / "package-path").write_text(str(package))
    else:
        if args.package is None or args.package.exists():
            parser.error("a new package destination is required")
        produce(args.build, args.package)


if __name__ == "__main__":
    main()
