#!/usr/bin/env python3
"""Generate CPU references and GLSL from pinned OCIO (isolated venv required)."""
import argparse
import hashlib
import json
from pathlib import Path

import PyOpenColorIO as ocio

CONFIG = "studio-config-v2.2.0_aces-v1.3_ocio-v2.4"
DISPLAY = "sRGB - Display"
VIEW = "ACES 1.0 - SDR Video"
CONFIG_SHA256 = "d8b361f76750ebfbedf0ded0b5e4315b283eed5e095814486b9d7c416cfbbb4c"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    if ocio.__version__ != "2.4.2":
        parser.error("requires opencolorio==2.4.2")
    args.destination.mkdir(parents=True, exist_ok=False)
    config = ocio.Config.CreateFromBuiltinConfig(CONFIG)
    config.validate()
    serialized = config.serialize()
    config_hash = hashlib.sha256(serialized.encode()).hexdigest()
    if config_hash != CONFIG_SHA256:
        raise RuntimeError(f"bundled config identity changed: {config_hash}")
    (args.destination / "config.ocio").write_text(serialized)
    display = ocio.DisplayViewTransform(src="ACEScg", display=DISPLAY, view=VIEW)
    # An asymmetric nonidentity LUT additionally tests OCIO's texture ordering,
    # sampling and binding. It is NOT a modification of the bundled display view.
    lut = ocio.Lut3DTransform(gridSize=33)
    for r in range(33):
        for g in range(33):
            for b in range(33):
                x, y, z = r / 32, g / 32, b / 32
                lut.setValue(r, g, b, x * (0.9 + 0.1 * y),
                             y * (0.8 + 0.2 * z), z * (0.7 + 0.3 * x))
    lut.setInterpolation(ocio.INTERP_LINEAR)
    pixels = []
    for y in range(16):
        for x in range(64):
            value = 2 ** (-12 + x * 18 / 63)
            alpha = [0.0, 0.01, 0.5, 1.0][y % 4]
            rgb = ([value] * 3 if y < 4 else
                   [value, value * (y / 15), value * (1 - y / 16)])
            if y >= 12:
                rgb[0] = -rgb[0]
            # Input buffer is premultiplied, like Fold working frames.
            pixels.append([v * alpha for v in rgb] + [alpha])
    pixels[64] = [0, 0, 0, 1]  # Opaque black must also pass the shader.
    pixels[65] = [1, 1, 1, 1]
    for name, transform in [("display", display),
                            ("lut-display", ocio.GroupTransform([lut, display]))]:
        processor = config.getProcessor(transform)
        cpu = processor.getDefaultCPUProcessor()
        expected = []
        for pixel in pixels:
            a = pixel[3]
            straight = [v / a for v in pixel[:3]] + [a] if a else [0, 0, 0, 0]
            result = cpu.applyRGBA(straight)
            expected.append([v * a for v in result[:3]] + [a])
        desc = ocio.GpuShaderDesc.CreateShaderDesc()
        desc.setLanguage(ocio.GPU_LANGUAGE_GLSL_4_0)
        desc.setFunctionName("fold_ocio")
        processor.getDefaultGPUProcessor().extractGpuShaderInfo(desc)
        assert not list(desc.getTextures()), "1D/2D LUTs not supported by this spike"
        assert not list(desc.getUniforms()), "dynamic properties not supported by this spike"
        shader = desc.getShaderText()
        # Naga 27 rejects stores through nested swizzles emitted by OCIO.
        for channel in "rgb":
            shader = shader.replace(f".rgb.{channel}", f".{channel}")
        textures = []
        for i, texture in enumerate(desc.get3DTextures()):
            assert texture.interpolation == ocio.INTERP_LINEAR
            sampler = texture.samplerName
            shader = shader.replace(
                f"uniform sampler3D {sampler};",
                f"layout(set=0,binding={2 + i * 2}) uniform texture3D tex{i};\n"
                f"layout(set=0,binding={3 + i * 2}) uniform sampler samp{i};")
            shader = shader.replace(f"texture({sampler},", f"textureLod(sampler3D(tex{i}, samp{i}),")
            # OCIO emits one texture lookup line per linear 3D LUT.
            lines = shader.splitlines()
            for j, line in enumerate(lines):
                if f"textureLod(sampler3D(tex{i}, samp{i})," in line:
                    lines[j] = line.replace(").rgb", ", 0.0).rgb")
            shader = "\n".join(lines)
            textures.append({"edge": texture.edgeLen, "values": texture.getValues().reshape(-1).tolist()})
        shader = """#version 450
layout(local_size_x=64) in;
layout(set=0,binding=0,std430) readonly buffer Inputs { vec4 pixels[]; } input_data;
layout(set=0,binding=1,std430) buffer Outputs { vec4 pixels[]; } output_data;
""" + shader + """
void main() {
    uint i = gl_GlobalInvocationID.x;
    vec4 p = input_data.pixels[i];
    if (p.a == 0.0) { output_data.pixels[i] = vec4(0.0); return; }
    vec4 v = fold_ocio(vec4(p.rgb / p.a, p.a));
    output_data.pixels[i] = vec4(v.rgb * p.a, p.a);
}
"""
        (args.destination / f"{name}.json").write_text(json.dumps({
            "ocio": ocio.__version__, "config": CONFIG, "display": DISPLAY, "view": VIEW,
            "config_sha256": config_hash,
            "processor": processor.getCacheID(), "shader": shader,
            "pixels": pixels, "expected": expected, "textures": textures,
        }, indent=2))
    print(args.destination)


if __name__ == "__main__":
    main()
