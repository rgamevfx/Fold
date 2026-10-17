#!/usr/bin/env python3
"""Qualify a relocated package with no checkout, build caches, network or system compiler."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def command(package, scratch):
    args = ['bwrap', '--die-with-parent', '--unshare-net', '--unshare-pid',
            '--proc', '/proc', '--dev-bind', '/dev', '/dev', '--ro-bind', '/sys', '/sys',
            '--ro-bind', '/etc', '/etc', '--dir', '/usr']
    # Mount runtime libraries/driver data only. No host root, checkout, home,
    # Cargo cache, native headers, compiler executables or development SDK.
    for name in ('/usr/lib', '/usr/lib64', '/lib', '/lib64', '/usr/share'):
        path = Path(name)
        if path.is_symlink():
            args += ['--symlink', os.readlink(path), name]
        elif path.is_dir():
            args += ['--ro-bind', name, name]
    return args + ['--tmpfs', '/tmp', '--dir', '/opt',
            '--ro-bind', str(package), '/opt/fold', '--bind', str(scratch), '/tmp/work',
            '--chdir', '/tmp/work', '--clearenv', '--setenv', 'HOME', '/tmp/work/home',
            '--setenv', 'XDG_CACHE_HOME', '/tmp/work/cache', '--setenv', 'XDG_RUNTIME_DIR', '/tmp/work',
            '--setenv', 'PATH', '/no-system-tools', '/opt/fold/bin/fold-runtime-check', '--isolated']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package', type=Path)
    args = parser.parse_args()
    if not shutil.which('bwrap'):
        parser.error('Install bubblewrap to run the isolated package acceptance test')
    package = args.package.resolve()
    if not (package / 'bin/fold-runtime-check').is_file():
        parser.error('Package is missing bin/fold-runtime-check')
    with tempfile.TemporaryDirectory(prefix='fold-isolated-runtime-') as directory:
        scratch = Path(directory)
        (scratch / 'home').mkdir()
        (scratch / 'cache').mkdir()
        subprocess.run(command(package, scratch), check=True, timeout=900)


if __name__ == '__main__':
    main()
