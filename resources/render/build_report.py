#!/usr/bin/env python3
"""Build the offline compositor report from retained JSONL measurements."""
import html
import json
import math
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parent


def esc(value):
    return html.escape(str(value), quote=True)


def measure(name):
    path = ROOT / "benchmarks" / name
    records = [json.loads(line) for line in path.read_text().splitlines() if line]
    warm = [r for r in records if not r["warmup"]]
    if len(warm) != 60 or len(records) != 70:
        raise ValueError(f"Unexpected sample count: {name}")
    walls = sorted(r["wall_ms"] for r in warm)
    return {
        "median": statistics.median(walls),
        "p95": walls[math.ceil(len(walls) * 0.95) - 1],
        "gpu": statistics.median(r["gpu_ms"] for r in warm),
        "passes": warm[-1]["passes"],
        "peak": max(r["peak_bytes"] for r in records) / (1024 * 1024),
        "file": name,
    }


def link(result):
    return f'<a href="benchmarks/{esc(result["file"])}">JSONL</a>'


def build():
    cases = [
        ("1080p / one solid", "1920-1080-60-0-0"),
        ("1080p / 16 layers", "1920-1080-60-16-0"),
        ("4K / 16 layers", "3840-2160-60-16-0"),
        ("1080p / blur 8", "1920-1080-60-1-8"),
        ("1080p / blur 32", "1920-1080-60-1-32"),
        ("1080p / blur 128", "1920-1080-60-1-128"),
        ("320 × 180 / 64 layers", "320-180-60-64-0"),
    ]
    timings, resources = [], []
    for title, key in cases:
        before = measure(f"baseline-{key}.jsonl")
        after = measure(f"final-{key}.jsonl")
        reduction = 100 * (1 - after["median"] / before["median"])
        timings.append(
            f'<tr><th scope="row">{esc(title)}</th>'
            f'<td>{before["median"]:.3f}</td><td>{after["median"]:.3f}</td>'
            f'<td>{reduction:.1f}%</td><td>{before["p95"]:.3f} → {after["p95"]:.3f}</td>'
            f'<td>{link(before)} / {link(after)}</td></tr>'
        )
        resources.append(
            f'<tr><th scope="row">{esc(title)}</th>'
            f'<td>{before["gpu"]:.3f} → {after["gpu"]:.3f}</td>'
            f'<td>{before["passes"]} → {after["passes"]}</td>'
            f'<td>{before["peak"]:.2f} → {after["peak"]:.2f}</td></tr>'
        )
    phase_rows = []
    for title, filename in [
        ("P1: status pool", "p1-status.jsonl"),
        ("P2: add color fusion", "p2-color.jsonl"),
        ("Final: one solid", "final-1920-1080-60-0-0.jsonl"),
    ]:
        row = measure(filename)
        phase_rows.append(f'<tr><th scope="row">{esc(title)}</th><td>{row["median"]:.3f}</td><td>{row["gpu"]:.3f}</td><td>{link(row)}</td></tr>')
    experiment_rows = []
    for title, filename in [
        ("Blur control / size 128", "blur-before-128.jsonl"),
        ("Reject: 8 × 8 vertical tiles", "rejected-vertical-128.jsonl"),
        ("Reject: transpose each pass", "rejected-transpose-128.jsonl"),
    ]:
        row = measure(filename)
        experiment_rows.append(f'<tr><th scope="row">{esc(title)}</th><td>{row["gpu"]:.3f}</td><td>{row["peak"]:.2f}</td><td>{link(row)}</td></tr>')
    rejected = measure("rejected-cheap-reuse.jsonl")
    final_small = measure("final-320-180-60-64-0.jsonl")
    evidence = json.loads((ROOT / "validation.json").read_text())
    tests = []
    for check in evidence["checks"]:
        if "passed" in check:
            result = f'{check["passed"]} passed; {check["failed"]} failed; {check["ignored"]} ignored in this command'
        else:
            result = check["result"]
        tests.append(f'<li><strong>{esc(check["name"])}</strong>: {esc(result)}<details><summary>Command</summary><pre>{esc(check["command"])}</pre></details></li>')
    limits = "".join(f'<li>{esc(value)}</li>' for value in evidence["not_run"])
    plan = esc((ROOT / "dev-plan.md").read_text())
    return f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Fold compositor optimization report</title>
<style>
:root {{ color-scheme:light; font:16px/1.6 system-ui,sans-serif; color:#182638; background:#f4f6f8; }}
body {{ max-width:1120px; margin:auto; padding:2rem 1.2rem 4rem; }}
header {{ border-bottom:4px solid #215b73; padding-bottom:1rem; }}
h1 {{ font-size:2.2rem; line-height:1.15; }} h2 {{ margin-top:2rem; color:#194b60; }}
a {{ color:#075c86; }} nav {{ display:flex; flex-wrap:wrap; gap:1.2rem; }}
section {{ background:white; border:1px solid #dbe1e6; border-radius:8px; padding:1.2rem; margin-top:1.4rem; }}
.notice {{ border-left:5px solid #aa6900; background:#fff6df; padding:1rem; }}
.scroll {{ overflow-x:auto; }} table {{ border-collapse:collapse; width:100%; font-size:.92rem; }}
caption {{ text-align:left; font-weight:600; margin:1rem 0; }}
th,td {{ padding:.65rem; border-bottom:1px solid #dbe1e6; text-align:right; white-space:nowrap; }}
th:first-child {{ text-align:left; }} thead {{ background:#edf3f6; }}
pre {{ white-space:pre-wrap; overflow-wrap:anywhere; background:#f5f7f9; padding:1rem; font-size:.87rem; }}
li {{ margin:.6rem 0; }} summary {{ cursor:pointer; color:#194b60; }}
.small {{ font-size:.9rem; color:#485568; }}
@media print {{ body {{ max-width:none; padding:0; }} section {{ break-inside:avoid; }} nav {{ display:none; }} }}
</style>
</head>
<body>
<header><p>FOLD / RENDER ENGINE</p><h1>Compositor optimization report</h1>
<p>Measured changes, tests, rejected work, and next steps.</p>
<nav aria-label="Report sections"><a href="#results">Results</a><a href="#changes">Changes</a><a href="#experiments">Experiments</a><a href="#tests">Tests</a><a href="#plan">Development plan</a></nav></header>
<p class="notice"><strong>Partial delivery.</strong> The full plan is not complete. Status storage, color passes, and cache-pressure handling are complete for this change set. Other phases are partial or open. No claim of full engine or desktop acceptance is made.</p>
<section id="results"><h2>1. Benchmark results</h2>
<p>Tests used an NVIDIA GTX 1070 with driver 580.173.02 and Vulkan. Each run used a release build. Each run discarded ten warmup frames and measured 60 frames. The desktop stayed active. GPU clocks and other GPU work were not controlled.</p>
<p>The fixture creates solid layers, opacity, over operations, and an optional Gaussian blur. It measures serial scene and display completion. It does not measure desktop FPS, media decode, Motion, or ACES processing. Equal solid sources favor the new reuse step. Do not apply the layer speed gain to a scene with distinct media sources.</p>
<div class="scroll"><table><caption>Frame latency. All times are milliseconds. Lower is better.</caption><thead><tr><th>Workload</th><th>Before median</th><th>After median</th><th>Reduction</th><th>p95 before → after</th><th>Before / after data</th></tr></thead><tbody>{''.join(timings)}</tbody></table></div>
<div class="scroll"><table><caption>GPU time, compute passes, and peak host-accounted allocation.</caption><thead><tr><th>Workload</th><th>GPU ms before → after</th><th>Passes before → after</th><th>Peak MiB before → after</th></tr></thead><tbody>{''.join(resources)}</tbody></table></div>
<p>The median is the middle frame time. The p95 value is the time that 95 percent of measured frames do not exceed. One MiB is 1,048,576 bytes.</p>
<p>Peak allocation is not total physical VRAM use. Image upload and image readback counters are zero in these synthetic cases. Status bytes and parameter uploads are separate. The filter's large-radius GPU cost remains.</p>
<details><summary>Run the benchmark</summary><pre>cargo build -p fold-render --release --features gpu --example compositor_profile --locked
target/release/examples/compositor_profile 1920 1080 60 16 0
FOLD_PROFILE_MAX_MS=3 target/release/examples/compositor_profile 1920 1080 60 0 0</pre><p>The 3 ms p95 limit is a diagnostic gate. It failed before the changes and passes now. It is not an agreed product limit.</p></details>
</section>
<section id="changes"><h2>2. Implemented changes</h2>
<ul><li><strong>Status storage:</strong> reuse at most eight completed, unmapped readback buffers. Keep GPU completion and failure checks.</li>
<li><strong>Color output:</strong> check the float result in the conversion pass. Store the display result directly. Remove the extra float image and validation scan.</li>
<li><strong>Small resources:</strong> use one uniform arena for generic operators and pointwise chains. Cache at most 16 exact blur kernels.</li>
<li><strong>Pass fusion:</strong> combine eligible masked pointwise chains and one-input channel shuffles. Check every logical intermediate.</li>
<li><strong>Repeated work:</strong> share equal solids and expensive blur results within one request. Do not share cheap opacity results that prevent consumer fusion.</li>
<li><strong>Pressure:</strong> remove individual cache entries. Start above 75 percent live use and stop at two thirds of the budget. Limit each request to 128 attempts.</li></ul>
<div class="scroll"><table><caption>Single-solid phase results at 1080p. These changes are cumulative.</caption><thead><tr><th>Phase</th><th>Median ms</th><th>GPU ms</th><th>Data</th></tr></thead><tbody>{''.join(phase_rows)}</tbody></table></div>
<p>The buffer pool removed the main fixed CPU cost. The color change reduced GPU work. Do not add the percentage gains from separate phases.</p>
<p>General bind-group reuse, general merge fusion, cross-frame caches, per-node tiles, working-format selection, and pooled native ingress are not implemented.</p></section>
<section id="experiments"><h2>3. Rejected experiments</h2>
<div class="scroll"><table><caption>Blur layout tests. One layer at 1080p, size 128.</caption><thead><tr><th>Variant</th><th>Median GPU ms</th><th>Peak MiB</th><th>Data</th></tr></thead><tbody>{''.join(experiment_rows)}</tbody></table></div>
<p>The 8 × 8 vertical layout was slower. Transposing both passes did not give a useful gain. Both shaders were removed. The exact separable shader remains.</p>
<p>Sharing cheap opacity results raised peak allocation in the small 64-layer case to {rejected['peak']:.2f} MiB. The final policy uses {final_small['peak']:.2f} MiB. The final policy keeps opacity-over fusion. <a href="benchmarks/rejected-cheap-reuse.jsonl">Rejected-policy data</a>.</p>
<p>No approximate Gaussian, automatic FP16 storage, or direct native import was added to claim a speed gain.</p></section>
<section id="tests"><h2>4. Validation</h2><ul>{''.join(tests)}</ul>
<p>Pixel checks include signed/HDR values, alpha, OCIO transforms, ProRes alpha, masks, shared inputs, region boundaries, and large filters. Lifecycle checks include cancellation, dropped work, device loss, and memory pressure.</p>
<h3>Not run or not implemented</h3><ul>{limits}</ul>
<p>Existing media, Motion, and UI edits were left unchanged. No desktop rebuild was needed. A real slow project still needs an end-to-end benchmark.</p>
<details><summary>Build identity</summary><pre>Base commit: {esc(evidence['base_commit'])}
Render source SHA-256: {esc(evidence['render_source_sha256'])}
{esc(evidence['hash_method'])}</pre></details>
</section>
<section id="plan"><h2>5. Development plan and notes</h2>
<p><a href="dev-plan.md">Open the plan</a> · <a href="validation.json">Open test evidence</a> · <a href="compositor-audit.json">Open the original audit</a></p>
<details><summary>Read all phase notes and next steps</summary><pre>{plan}</pre></details>
<p class="small">This report uses short, direct English in the style of ASD-STE100. It has not passed a formal language-conformance check. Rebuild it with: <code>python3 resources/render/build_report.py</code>.</p></section>
</body></html>
'''


if __name__ == "__main__":
    (ROOT / "report.html").write_text(build())
    print(ROOT / "report.html")
