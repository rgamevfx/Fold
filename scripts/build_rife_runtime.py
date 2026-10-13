#!/usr/bin/env python3
"""Build Fold's pinned Linux Vulkan RIFE runtime and model package."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
RIFE = 'a7532fc3f9f8f008cd6eecd6f2ffe2a9698e0cf7'
NCNN = 'b4ba207c18d3103d6df890c0e3a97b469b196b26'
GLSLANG = '86ff4bca1ddc7e2262f119c16e7228d0efb67610'
MODEL = {'flownet.bin': 'f334ed2260149ce0188a6dcf049844e8b0cdd912e01cbcfb63553157d2508958',
         'flownet.param': '724569596bcd1e7b9fa50455c604777ebed99746d2ef40aa86e31b5725f1053c'}

def run(args, **kwargs):
    subprocess.run([str(v) for v in args], check=True, **kwargs)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--prefix', type=Path, required=True)
    p.add_argument('--source', type=Path, help='Existing checkout at the pinned revisions')
    a = p.parse_args()
    work, prefix = a.work.resolve(), a.prefix.resolve()
    work.mkdir(parents=True, exist_ok=True)
    source = a.source.resolve() if a.source else work / 'source'
    if not source.exists():
        run(['git', 'clone', '--filter=blob:none', '--no-checkout', 'https://github.com/nihui/rife-ncnn-vulkan.git', source])
        run(['git', '-C', source, 'sparse-checkout', 'set', 'src', 'models/rife-v4.6'])
        run(['git', '-C', source, 'checkout', RIFE])
        run(['git', '-C', source, 'submodule', 'update', '--init', 'src/ncnn'])
        run(['git', '-C', source / 'src/ncnn', 'submodule', 'update', '--init', 'glslang'])
    for path, revision in [(source, RIFE), (source/'src/ncnn', NCNN), (source/'src/ncnn/glslang', GLSLANG)]:
        actual = subprocess.check_output(['git', '-C', str(path), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != revision:
            raise RuntimeError(f'Unexpected source revision: {path}: {actual}')
    for name, digest in MODEL.items():
        if hashlib.sha256((source/'models/rife-v4.6'/name).read_bytes()).hexdigest() != digest:
            raise RuntimeError(f'RIFE model checksum mismatch: {name}')
    # Narrow pinned-source patch: enable Linux external buffers on ncnn's device.
    # This is a required GPU-exchange capability, not a staging/CPU fallback.
    gpu = source/'src/ncnn/src/gpu.cpp'
    text = gpu.read_text()
    marker = '    deviceCreateInfo.enabledExtensionCount = enabledExtensions.size();'
    patch = '''    // Fold external-buffer exchange requires this Vulkan device extension.
    {
        uint32_t count = 0;
        vkEnumerateDeviceExtensionProperties(info.physical_device(), 0, &count, 0);
        std::vector<VkExtensionProperties> available(count);
        vkEnumerateDeviceExtensionProperties(info.physical_device(), 0, &count, available.data());
        bool found = false;
        for (const auto& e : available)
            if (strcmp(e.extensionName, "VK_KHR_external_memory_fd") == 0) found = true;
        if (found) enabledExtensions.push_back("VK_KHR_external_memory_fd");
    }
'''
    if '// Fold external-buffer exchange' not in text:
        if text.count(marker) != 1:
            raise RuntimeError('Pinned ncnn patch target changed')
        gpu.write_text(text.replace(marker, patch+marker))
    build = work/'build'
    run(['cmake', '-S', ROOT/'resources/rife', '-B', build, '-G', 'Ninja',
         '-DCMAKE_BUILD_TYPE=Release', f'-DRIFE_SOURCE={source}', f'-DCMAKE_INSTALL_PREFIX={prefix}',
         '-DNCNN_AVX512=OFF', '-DNCNN_AVX512VNNI=OFF', '-DNCNN_AVX512BF16=OFF', '-DNCNN_AVX512FP16=OFF'])
    run(['cmake', '--build', build, '--target', 'fold_rife', '-j', min(os.cpu_count() or 1, 8)])
    (prefix/'lib/fold').mkdir(parents=True, exist_ok=True)
    shutil.copy2(build/'libfold_rife.so', prefix/'lib/fold/libfold_rife.so')
    dest = prefix/'share/fold/rife'
    dest.mkdir(parents=True, exist_ok=True)
    for name in MODEL:
        shutil.copy2(source/'models/rife-v4.6'/name, dest/name)
    for name, path in [('RIFE-LICENSE', source/'LICENSE'), ('ncnn-LICENSE', source/'src/ncnn/LICENSE.txt'),
                       ('glslang-LICENSE', source/'src/ncnn/glslang/LICENSE.txt')]:
        shutil.copy2(path, dest/name)
    (dest/'manifest.json').write_text(json.dumps({'abi':1, 'model':'rife-v4.6', 'rife':RIFE,
        'ncnn':NCNN, 'glslang':GLSLANG, 'sha256':MODEL,
        'reconstruction':'Original scene-linear premultiplied RGBA, v4.6 flow and mask'}, indent=2)+'\n')

if __name__ == '__main__':
    main()
