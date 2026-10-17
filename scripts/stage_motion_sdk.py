#!/usr/bin/env python3
"""Stage the Motion SDK with source, vendor, native and compiler inputs.

Use --precompile with all inputs to build runtime dependency libraries once.
Runtime compiles authored scripts with rustc; Cargo is used only by this producer.
"""
import hashlib
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import os
import tempfile
import tarfile
from runtime_cache import build_environment
from motion_linker import stage_linker, linker_args

ROOT = Path(__file__).resolve().parents[1]
CRATES = ("motion-fframes", "motion-canvas", "gpu-sharing", "graph",
          "animation", "foundation", "project")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_skia(archive):
    """Reject native inputs that would restore lossy GPU paint conversion."""
    with tarfile.open(archive) as bundle:
        prefix = "skia-binaries/"
        try:
            with bundle.extractfile(prefix + "fold-precision.json") as stream:
                manifest = json.load(stream)
            patch = ROOT / "resources/motion-context/skia-color-precision.patch"
            if manifest.get("version") != 1 or manifest.get("patch_sha256") != digest(patch):
                raise ValueError("Skia precision patch does not match this source tree")
            names = {entry.name.removeprefix(prefix) for entry in bundle.getmembers()
                     if entry.isfile() and (entry.name.endswith(".a")
                                           or entry.name == prefix + "bindings.rs")}
            if not names or set(manifest["files"]) != names:
                raise ValueError("Incomplete Skia precision manifest")
            for name, expected in manifest["files"].items():
                with bundle.extractfile(prefix + name) as stream:
                    if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
                        raise ValueError(f"Skia archive hash differs: {name}")
        except (KeyError, TypeError) as error:
            raise ValueError("Use scripts/build_motion_skia.py to build the precise Skia archive") from error


def install_libraries(destination, artifacts, rustc, profile):
    """Record compiler ABI, source inputs and the complete dependency library set."""
    inputs = [destination / "Cargo.toml", destination / "Cargo.lock",
              destination / "crates/motion-fframes/Cargo.lock",
              destination / ".cargo/config.toml"]
    for name in CRATES:
        crate = destination / "crates" / name
        inputs.append(crate / "Cargo.toml")
        inputs.extend(path for path in (crate / "src").rglob("*") if path.is_file())
    inputs.extend(path for path in (destination / "crates/motion-fframes/components").rglob("*") if path.is_file())
    inputs.extend(path for path in (destination / "resources/fframes-components").rglob("*") if path.is_file())
    inputs.extend(path for path in (destination / "resources/fonts").rglob("*") if path.is_file())
    inputs.extend(path for path in (destination / "native").rglob("*") if path.is_file())
    inputs.extend(path for path in (destination / "link").rglob("*") if path.is_file())
    inputs.append(destination / "toolchain/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld")
    libraries = destination / "libraries"
    libraries.mkdir()
    files = {}
    for path in sorted(artifacts.iterdir()):
        if path.suffix in (".rlib", ".so"):
            shutil.copy2(path, libraries / path.name)
            files[path.name] = digest(libraries / path.name)
    externs = {}
    for name in ("fframes", "fold_motion_fframes"):
        matches = [name_ for name_ in files if name_.startswith(f"lib{name}-") and name_.endswith(".rlib")]
        if len(matches) != 1:
            raise ValueError(f"Expected one compiled {name} library")
        externs[name] = matches[0]
    native_worker = "fold-motion-native-worker"
    command = [str(rustc), str(destination / "crates/motion-fframes/src/bin" / (native_worker + ".rs")),
               "--edition=2024", "--crate-name", "fold_motion_native_worker", "-C", "opt-level=3",
               "-L", f"dependency={libraries}", "-o", str(libraries / native_worker)]
    for name, file in externs.items():
        command.extend(["--extern", f"{name}={libraries / file}"])
    command.extend(linker_args(destination))
    subprocess.run(command, check=True, timeout=600, env=build_environment())
    files[native_worker] = digest(libraries / native_worker)
    (libraries / "manifest.json").write_text(json.dumps({
        "version": 2, "profile": profile, "native_worker": native_worker,
        "rustc": subprocess.check_output([str(rustc), "-vV"], text=True),
        "inputs": {str(path.relative_to(destination)): digest(path) for path in sorted(inputs)},
        "files": files, "externs": externs,
    }, indent=2) + "\n")


def stage(destination, vendor=False, skia=None, ffmpeg=None, toolchain=None, precompile=False):
    if precompile and not (vendor and skia and ffmpeg and toolchain):
        raise ValueError("Precompilation requires vendored sources, native archives and a toolchain")
    if bool(skia) != bool(ffmpeg) or (skia and not vendor):
        raise ValueError("Both native archives and --vendor are required together")
    for archive, prefix in ((skia, "skia-binaries-"), (ffmpeg, "ffmpeg-")):
        if archive and (not archive.is_file() or not archive.name.startswith(prefix)
                        or not archive.name.endswith(".tar.gz")):
            raise ValueError(f"Invalid {prefix} archive")
    if skia:
        validate_skia(skia)
    if toolchain and not all((toolchain / "bin" / name).is_file() for name in ("rustc", "cargo")):
        raise ValueError("Toolchain must contain Cargo and rustc binaries")
    if toolchain:
        notices = toolchain / "share/doc/rust"
        if not (notices / "licenses").is_dir() or not all(
            (notices / name).is_file() for name in ("COPYRIGHT.html", "COPYRIGHT-library.html")
        ):
            raise ValueError("Toolchain must be a Rust sysroot with redistribution notices")
    if not destination.is_absolute() or destination.exists():
        raise ValueError("An absolute, new SDK directory is required")
    destination.mkdir(parents=True)
    manifest = (ROOT / "Cargo.toml").read_text()
    end = manifest.index("[workspace.package]")
    members = [f"crates/{name}" for name in CRATES if name != "motion-fframes"]
    manifest = ("[workspace]\nresolver = \"3\"\nmembers = "
                + json.dumps(members) + "\n\n" + manifest[end:])
    (destination / "Cargo.toml").write_text(manifest)
    shutil.copy2(ROOT / "Cargo.lock", destination / "Cargo.lock")
    for name in CRATES:
        source = ROOT / "crates" / name
        target = destination / "crates" / name
        target.mkdir(parents=True)
        shutil.copytree(source / "src", target / "src")
        if name == "motion-fframes":
            shutil.copytree(source / "components", target / "components")
        manifest = (source / "Cargo.toml").read_text()
        # The installed compiler consumes these crates as dependencies. Do not
        # pull editor/test-only path dependencies into the production SDK.
        manifest = re.sub(r"(?ms)^\[dev-dependencies\]\n.*?(?=^\[|\Z)", "", manifest)
        (target / "Cargo.toml").write_text(manifest)
        if (source / "Cargo.lock").exists():
            shutil.copy2(source / "Cargo.lock", target / "Cargo.lock")
    shutil.copytree(ROOT / "resources/fonts", destination / "resources/fonts")
    shutil.copytree(ROOT / "resources/fframes-components", destination / "resources/fframes-components")
    # Resolve the production graph before recording input hashes: removing dev
    # dependencies can prune the copied lock without changing pinned versions.
    subprocess.run([
        "cargo", "metadata", "--manifest-path",
        str(destination / "crates/motion-fframes/Cargo.toml"),
        "--offline", "--filter-platform", "x86_64-unknown-linux-gnu",
        "--format-version", "1",
    ], check=True, stdout=subprocess.DEVNULL, timeout=60, env=build_environment())
    if vendor:
        result = subprocess.run([
            "cargo", "vendor", "--offline", "--locked",
            "--versioned-dirs", "--manifest-path",
            str(destination / "crates/motion-fframes/Cargo.toml"),
            str(destination / "vendor"),
        ], check=True, capture_output=True, text=True, timeout=600, env=build_environment())
        if not result.stdout.strip():
            raise ValueError("Cargo vendor did not emit source configuration")
        config = destination / ".cargo/config.toml"
        config.parent.mkdir()
        config.write_text(result.stdout.replace(
            json.dumps(str(destination / "vendor")), json.dumps("vendor")))
    for kind, archive in (("skia", skia), ("ffmpeg", ffmpeg)):
        if archive:
            target = destination / "native" / kind
            target.mkdir(parents=True)
            shutil.copy2(archive, target / archive.name)
    if toolchain:
        target = destination / "toolchain"
        (target / "bin").mkdir(parents=True)
        for name in ("cargo", "rustc"):
            shutil.copy2(toolchain / "bin" / name, target / "bin" / name)
        shutil.copytree(toolchain / "lib", target / "lib", symlinks=True)
        notices = toolchain / "share/doc/rust"
        shutil.copytree(notices / "licenses", target / "licenses")
        for name in ("COPYRIGHT.html", "COPYRIGHT-library.html"):
            shutil.copy2(notices / name, target / "licenses" / name)
    if precompile:
        stage_linker(destination)
        rustc = destination / "toolchain/bin/rustc"
        with tempfile.TemporaryDirectory(prefix="fold-sdk-dependencies-") as working:
            target = Path(working) / "target"
            subprocess.run([
                str(destination / "toolchain/bin/cargo"), "build", "--release", "--lib",
                "--offline", "--locked", "--config", str(destination / ".cargo/config.toml"),
                "--manifest-path", str(destination / "crates/motion-fframes/Cargo.toml"),
                "--target-dir", str(target),
            ], env=dict(build_environment(), RUSTC=str(rustc), CARGO_HOME=str(Path(working) / "cargo"),
                SKIA_BINARIES_URL=f"file://{destination}/native/skia/skia-binaries-{{key}}.tar.gz",
                FFMPEG_BINARIES_URL=f"file://{destination}/native/ffmpeg/ffmpeg-{{key}}.tar.gz",
                FFMPEG_BINARIES_CACHE=str(Path(working) / "ffmpeg")), check=True, timeout=1800)
            install_libraries(destination, target / "release/deps", rustc, "release")
    files = {
        str(path.relative_to(destination)): digest(path)
        for path in sorted(destination.rglob("*")) if path.is_file()
    }
    (destination / "sources.json").write_text(json.dumps({
        "status": "precompiled_libraries" if precompile else ("native_build_inputs" if skia else ("vendored_sources" if vendor else "source_inputs_only")),
        "bridge": "crates/motion-fframes",
        "files": files,
        "requires": (["Linux x86_64 runtime libraries and Vulkan driver", "relocation qualification"]
                     if precompile else
                     ([] if vendor else ["locked upstream dependency bundle"])
                     + ([] if toolchain else ["Rust compiler and Cargo"])
                     + ["system C/C++ linker and development libraries", "Clang/libclang",
                        "relocation qualification"]
                     + ([] if skia else ["native build archives"])),
    }, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--vendor", action="store_true")
    parser.add_argument("--skia", type=Path)
    parser.add_argument("--ffmpeg", type=Path)
    parser.add_argument("--toolchain", type=Path)
    parser.add_argument("--precompile", action="store_true")
    args = parser.parse_args()
    stage(args.destination, args.vendor, args.skia, args.ffmpeg, args.toolchain, args.precompile)
