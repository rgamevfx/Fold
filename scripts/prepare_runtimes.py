#!/usr/bin/env python3
"""Prepare pinned Motion/RIFE runtimes. Called automatically by Cargo and packaging."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

from runtime_cache import ROOT, build_environment, download, ensure, fingerprint
from stage_motion_sdk import CRATES, stage, validate_skia
from build_motion_skia import build as build_skia


def source_files(directory):
    return [path for path in directory.rglob('*') if path.is_file()
            and not any(part in ('target', '.git', '__pycache__') for part in path.relative_to(directory).parts)]


def ensure_rife(cache):
    inputs = [ROOT / 'scripts/build_rife_runtime.py', ROOT / 'scripts/prepare_runtimes.py',
              ROOT / 'scripts/runtime_cache.py', *source_files(ROOT / 'resources/rife')]
    identity = fingerprint(inputs, [('c++', '--version'), ('cmake', '--version')])
    def produce(work, package):
        subprocess.run([sys.executable, str(ROOT / 'scripts/build_rife_runtime.py'),
                        '--work', str(work), '--prefix', str(package)], check=True,
                       env=build_environment())
    return ensure(cache / 'rife', identity, produce)


def ensure_motion(cache):
    rustc = os.environ.get('RUSTC', 'rustc')
    sysroot = Path(subprocess.check_output([rustc, '--print', 'sysroot'], text=True).strip())
    inputs = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock']
    for name in ('prepare_runtimes.py', 'runtime_cache.py', 'stage_motion_sdk.py',
                 'build_motion_skia.py', 'motion_linker.py'):
        inputs.append(ROOT / 'scripts' / name)
    for name in CRATES:
        base = ROOT / 'crates' / name
        inputs += [base / 'Cargo.toml', *source_files(base / 'src')]
        if (base / 'Cargo.lock').exists():
            inputs.append(base / 'Cargo.lock')
    for name in ('crates/motion-fframes/components', 'resources/fonts', 'resources/fframes-components'):
        inputs += source_files(ROOT / name)
    inputs += [ROOT / 'resources/motion-context' / name for name in
               ('runtime-inputs.json', 'skia-color-precision.patch')]
    identity = fingerprint(inputs, [(rustc, '-vV'), ('c++', '--version'), ('cmake', '--version')])
    def produce(work, package):
        work.mkdir(parents=True, exist_ok=True)
        pins = json.loads((ROOT / 'resources/motion-context/runtime-inputs.json').read_text())
        archives = {name: download(cache / 'downloads', pins[name])
                    for name in ('skia_source', 'skia_reference', 'ffmpeg')}
        env = build_environment()
        subprocess.run(['cargo', 'fetch', '--locked', '--manifest-path',
                        str(ROOT / 'crates/motion-fframes/Cargo.toml')], check=True, env=env)
        # The patch/source identity is checked by the existing producer. Keep its
        # expensive native output independent of application/SDK source revisions.
        native_identity = fingerprint([ROOT / 'scripts/build_motion_skia.py',
            ROOT / 'resources/motion-context/skia-color-precision.patch',
            ROOT / 'resources/motion-context/runtime-inputs.json'], [('c++', '--version')])
        native = work / native_identity
        patched = native / 'patched' / archives['skia_reference'].name
        if patched.exists():
            validate_skia(patched)
        else:
            build_skia(archives['skia_source'], archives['skia_reference'],
                       native / 'skia-source', patched, False, native / 'skia-target')
        stage(package, vendor=True, skia=patched, ffmpeg=archives['ffmpeg'],
              toolchain=sysroot, precompile=True)
    return ensure(cache / 'motion', identity, produce)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('kind', choices=('motion', 'rife', 'all'))
    parser.add_argument('cache', type=Path)
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        parser.error('automatic native runtime builds currently support Linux x86_64')
    cache = args.cache.resolve()
    for name, producer in (('motion', ensure_motion), ('rife', ensure_rife)):
        if args.kind in (name, 'all'):
            package = producer(cache)
            (cache / f'{name}-path').write_text(str(package))
            print(f'{name}: {package}', flush=True)


if __name__ == '__main__':
    main()
