"""Atomic, verified native-runtime caches shared by Cargo and the packager."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def build_environment():
    # Nested Cargo builds have their own target tree and job budget. Inheriting
    # the parent jobserver/target settings can deadlock a Cargo build script.
    env = dict(os.environ)
    for name in ('CARGO_MAKEFLAGS', 'MAKEFLAGS', 'MFLAGS', 'CARGO_TARGET_DIR',
                 'CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS', 'RUSTC_WRAPPER',
                 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET'):
        env.pop(name, None)
    env['CARGO_BUILD_JOBS'] = str(min(4, int(env.get('CARGO_BUILD_JOBS', '4'))))
    env['CARGO_INCREMENTAL'] = '0'
    return env


def fingerprint(paths, commands=()):
    value = hashlib.sha256()
    for path in sorted(set(paths)):
        value.update(str(path.relative_to(ROOT)).encode())
        value.update(digest(path).encode())
    for command in commands:
        value.update(subprocess.check_output(command))
    return value.hexdigest()


def valid(package, identity):
    try:
        manifest = json.loads((package / '.fold-runtime.json').read_text())
        return (manifest['identity'] == identity and bool(manifest['files'])
                and all(digest(package / name) == expected
                        for name, expected in manifest['files'].items()))
    except (OSError, ValueError, KeyError, TypeError):
        return False


def ensure(cache, identity, producer):
    cache.mkdir(parents=True, exist_ok=True)
    with (cache / 'lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        package = cache / identity / 'package'
        if not valid(package, identity):
            package.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(dir=package.parent) as temporary:
                staged = Path(temporary) / 'package'
                producer(cache / 'work', staged)
                files = {str(p.relative_to(staged)): digest(p)
                         for p in sorted(staged.rglob('*')) if p.is_file()}
                (staged / '.fold-runtime.json').write_text(json.dumps(
                    {'identity': identity, 'files': files}, indent=2) + '\n')
                if package.exists():
                    shutil.rmtree(package)
                staged.rename(package)
        return package


def download(directory, entry):
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / entry['filename']
    if path.exists() and digest(path) == entry['sha256']:
        return path
    with tempfile.NamedTemporaryFile(dir=directory, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            with urllib.request.urlopen(entry['url'], timeout=120) as response:
                shutil.copyfileobj(response, stream)
            stream.close()
            if digest(temporary) != entry['sha256']:
                raise ValueError(f"Download checksum mismatch: {entry['filename']}")
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)
    return path
