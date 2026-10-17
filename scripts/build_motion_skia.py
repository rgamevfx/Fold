#!/usr/bin/env python3
"""Build the pinned Skia archive with precise GPU vertex colors.

Supply the m153-0.101.2 source tarball and the matching rust-skia binary archive.
The latter supplies its distribution key and notices, not implementation binaries.
The source and Cargo output directories can be reused after an interrupted build.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
from runtime_cache import build_environment

ROOT = Path(__file__).resolve().parents[1]
PATCH = ROOT / "resources/motion-context/skia-color-precision.patch"
SOURCE_SHA256 = "30c8b51a0a32fae63562c303bdc91605519a9598951422610e3fb8685b3c4f6f"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build(source_archive, reference_archive, source, destination, offline, target=None):
    if digest(source_archive) != SOURCE_SHA256:
        raise ValueError("Expected the pinned rust-skia m153-0.101.2 source archive")
    if destination.exists() or destination.name != reference_archive.name:
        raise ValueError("Use a new output archive with the reference archive's name")
    with tempfile.TemporaryDirectory(prefix="fold-skia-") as temporary:
        temporary = Path(temporary)
        reference_source = temporary / "source"
        reference_source.mkdir()
        with tarfile.open(source_archive) as archive:
            archive.extractall(reference_source, filter="data")
        entries = list(reference_source.iterdir())
        if len(entries) != 1 or not entries[0].is_dir():
            raise ValueError("Expected one source directory")
        prepared = entries[0]
        subprocess.run(["patch", "--batch", "-p1", "-i", str(PATCH)],
                       cwd=prepared, check=True)
        if source.exists():
            for path in prepared.rglob("*"):
                if path.is_file() and digest(path) != digest(source / path.relative_to(prepared)):
                    raise ValueError(f"Existing source differs: {path.relative_to(prepared)}")
        else:
            shutil.copytree(prepared, source)
        if not offline:
            subprocess.run(["python3", "tools/git-sync-deps"], cwd=source, check=True,
                           env=dict(build_environment(), GIT_SYNC_DEPS_SKIP_EMSDK="1"))
        command = ["cargo", "build", "--manifest-path",
                   str(ROOT / "crates/motion-fframes/Cargo.toml"), "--release", "--lib",
                   "--offline", "--message-format=json-render-diagnostics"]
        if target is not None:
            command.extend(["--target-dir", str(target)])
        process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True,
                                   env=dict(build_environment(), SKIA_SOURCE_DIR=str(source)))
        output = None
        for line in process.stdout:
            event = json.loads(line)
            if (event.get("reason") == "build-script-executed"
                    and "#skia-bindings@" in event.get("package_id", "")):
                output = Path(event["out_dir"]) / "skia"
        if process.wait() != 0 or output is None:
            raise RuntimeError("Native Skia build failed or produced no output")
        distribution = temporary / "distribution"
        distribution.mkdir()
        with tarfile.open(reference_archive) as archive:
            archive.extractall(distribution, filter="data")
        binaries = distribution / "skia-binaries"
        files = {}
        for path in binaries.iterdir():
            if path.suffix == ".a" or path.name == "bindings.rs":
                shutil.copy2(output / path.name, path)
                files[path.name] = digest(path)
        if "libskia.a" not in files or "bindings.rs" not in files:
            raise ValueError("Incomplete reference archive")
        (binaries / "fold-precision.json").write_text(json.dumps({
            "version": 1, "source": "rust-skia/skia m153-0.101.2",
            "source_sha256": SOURCE_SHA256, "patch_sha256": digest(PATCH),
            "files": files,
        }, indent=2) + "\n")
        shutil.copy2(PATCH, binaries / PATCH.name)
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tarfile.open(destination, "w:gz") as archive:
            archive.add(binaries, arcname="skia-binaries")
    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_archive", type=Path)
    parser.add_argument("reference_archive", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--source-dir", type=Path, required=True)
    parser.add_argument("--offline", action="store_true", help="Use already fetched native dependencies")
    args = parser.parse_args()
    build(args.source_archive.resolve(), args.reference_archive.resolve(),
          args.source_dir.resolve(), args.destination.resolve(), args.offline)
