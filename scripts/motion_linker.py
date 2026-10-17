"""Package Motion's native link inputs; installed script compilation uses rust-lld."""
from pathlib import Path
import shutil
import subprocess

TARGET = 'x86_64-unknown-linux-gnu'
LIBRARIES = {'c': 'libc.so.6', 'm': 'libm.so.6', 'dl': 'libdl.so.2',
             'pthread': 'libpthread.so.0', 'rt': 'librt.so.1', 'util': 'libutil.so.1',
             'gcc_s': 'libgcc_s.so.1', 'stdc++': 'libstdc++.so.6',
             'fontconfig': 'libfontconfig.so.1', 'freetype': 'libfreetype.so.6',
             'atomic': 'libatomic.so.1'}


def stage_linker(destination):
    linker = destination / f'toolchain/lib/rustlib/{TARGET}/bin/rust-lld'
    if not linker.is_file():
        raise ValueError('The Rust toolchain must include rust-lld for Linux x86_64')
    root = destination / 'link'
    root.mkdir()
    for target, source in [(name, name) for name in ('Scrt1.o', 'crti.o', 'crtn.o')] + [
            (f'lib{name}.so', file) for name, file in LIBRARIES.items()]:
        path = Path(subprocess.check_output(['c++', '-print-file-name=' + source], text=True).strip())
        if not path.is_file():
            raise ValueError(f'Missing native link input {source}; install the C/C++ development tools')
        # Materialize actual ELF objects, not host-absolute symlinks/linker scripts.
        shutil.copy2(path.resolve(), root / target)
    notices = root / 'licenses'
    notices.mkdir()
    for package in ('libc6', 'libgcc-s1', 'libstdc++6', 'libfontconfig1', 'libfreetype6', 'libatomic1'):
        source = Path('/usr/share/doc') / package / 'copyright'
        if not source.is_file():
            raise ValueError(f'Missing redistribution notice: {source}; use the Ubuntu 24.04 build baseline')
        shutil.copy2(source, notices / f'{package}-copyright')


def linker_args(destination):
    root = destination / 'link'
    result = ['-C', 'linker=' + str(destination / f'toolchain/lib/rustlib/{TARGET}/bin/rust-lld'),
              '-C', 'linker-flavor=ld.lld', '-L', 'native=' + str(root)]
    for argument in [str(root / name) for name in ('Scrt1.o', 'crti.o', 'crtn.o')] + [
            '--dynamic-linker=/lib64/ld-linux-x86-64.so.2']:
        result += ['-C', 'link-arg=' + argument]
    return result
